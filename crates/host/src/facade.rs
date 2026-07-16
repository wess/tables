//! The host facade — the async orchestration layer the UI calls.
//!
//! One `Host` is created at startup and shared as `Arc<Host>`. It owns the
//! live-connection registry, the health monitor, and the "active connection"
//! cursor. Every method mirrors one operation of the original IPC surface.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use db::{Dialect, HealthMonitor, Registry, SharedAdapter};
use model::{ConnectionTestResult, Row, StoredConnection};
use store::{connections, keychain};

const KEYCHAIN_FLAG: &str = "secretInKeychain";

pub struct Host {
    pub(crate) registry: Arc<Registry>,
    pub(crate) health: HealthMonitor,
    /// The active connection id.
    active: Mutex<Option<String>>,
}

impl Default for Host {
    fn default() -> Self {
        Host::new()
    }
}

impl Host {
    pub fn new() -> Self {
        Host {
            registry: Arc::new(Registry::default()),
            health: HealthMonitor::default(),
            active: Mutex::new(None),
        }
    }

    // --- active-connection cursor -------------------------------------------

    pub fn active_connection_id(&self) -> Option<String> {
        self.active.lock().unwrap().clone()
    }

    pub(crate) fn set_active(&self, id: &str) {
        *self.active.lock().unwrap() = Some(id.to_string());
    }

    /// The adapter for the active connection, or an error.
    pub(crate) fn active_adapter(&self) -> Result<SharedAdapter, String> {
        let id = self
            .active_connection_id()
            .ok_or_else(|| "No active connection".to_string())?;
        self.registry.adapter(&id)
    }

    // --- connections --------------------------------------------------------

    pub fn list_connections(&self) -> Vec<StoredConnection> {
        connections::load()
    }

    pub fn find_connection(&self, id: &str) -> Option<StoredConnection> {
        connections::find(id)
    }

    /// Persist a connection, storing any password in the OS keychain rather
    /// than the JSON file. If the keychain is unavailable the password stays in
    /// JSON (graceful fallback).
    pub fn save_connection(&self, conn: &StoredConnection) -> Result<StoredConnection, String> {
        let mut conn = conn.clone();
        if conn.id.is_empty() {
            conn.id = model::new_uuid();
        }
        if conn.kind != "sqlite"
            && !conn.password.is_empty()
            && keychain::set_secret(&conn.id, &conn.password).is_ok()
        {
            conn.password = String::new();
            conn.extra.insert(KEYCHAIN_FLAG.into(), serde_json::Value::Bool(true));
        }
        connections::upsert(&conn)
    }

    /// The usable password for a connection: the JSON value if present, else the
    /// keychain secret. Used to prefill the edit form.
    pub fn resolve_password(&self, conn: &StoredConnection) -> String {
        if !conn.password.is_empty() {
            return conn.password.clone();
        }
        keychain::get_secret(&conn.id).unwrap_or_default()
    }

    /// Stop health, disconnect, drop the secret, then drop from the file.
    pub async fn delete_connection(&self, id: &str) -> bool {
        self.health.stop(id);
        self.registry.disconnect(id).await;
        keychain::delete_secret(id);
        connections::remove(id)
    }

    /// Connect a throwaway adapter, read the version, close.
    pub async fn test_connection(&self, conn: &StoredConnection) -> ConnectionTestResult {
        let mut config = conn.config();
        config.id = "test".into();
        let adapter = match db::create(&config) {
            Ok(adapter) => adapter,
            Err(error) => {
                return ConnectionTestResult { ok: false, version: None, error: Some(error) }
            }
        };
        let probe = async {
            adapter.connect().await?;
            let version = adapter.get_version().await?;
            adapter.disconnect().await;
            Ok::<String, String>(version)
        }
        .await;
        match probe {
            Ok(version) => ConnectionTestResult { ok: true, version: Some(version), error: None },
            Err(error) => ConnectionTestResult { ok: false, version: None, error: Some(error) },
        }
    }

    /// Connect, make it active, begin health probes. Resolves the password from
    /// the keychain, migrating a legacy plaintext password out of the JSON file
    /// on first use (best-effort).
    pub async fn connect(&self, id: &str) -> Result<bool, String> {
        let mut conn =
            connections::find(id).ok_or_else(|| format!("Connection not found: {id}"))?;
        if conn.kind != "sqlite" {
            if conn.password.is_empty() {
                if let Some(secret) = keychain::get_secret(id) {
                    conn.password = secret;
                }
            } else if keychain::set_secret(id, &conn.password).is_ok() {
                // Migrate: blank the plaintext only after the keychain write.
                let mut migrated = conn.clone();
                migrated.password = String::new();
                migrated.extra.insert(KEYCHAIN_FLAG.into(), serde_json::Value::Bool(true));
                let _ = connections::upsert(&migrated);
            }
        }
        self.registry.connect(&conn.config()).await?;
        self.set_active(id);
        self.health.start(id.to_string(), self.registry.clone());
        Ok(true)
    }

    pub async fn disconnect(&self, id: &str) -> bool {
        self.health.stop(id);
        self.registry.disconnect(id).await;
        true
    }

    pub fn health(&self, id: &str) -> model::Health {
        self.health.status(id)
    }

    pub fn is_connected(&self, id: &str) -> bool {
        self.registry.is_connected(id)
    }
}

/// The target table's `column name → type` map, used to cast bound values to
/// the column type on Postgres (whose binds are strictly typed). Empty for
/// MySQL/SQLite — they coerce a text parameter implicitly — and on failure.
pub(crate) async fn pg_col_types(
    adapter: &SharedAdapter,
    dialect: Dialect,
    table: &str,
) -> HashMap<String, String> {
    if dialect != Dialect::Postgres {
        return HashMap::new();
    }
    adapter
        .get_columns(table)
        .await
        .map(|cols| cols.into_iter().map(|c| (c.name, c.data_type)).collect())
        .unwrap_or_default()
}

/// `Number(row[key] ?? 0)` for a driver cell — numbers pass through, numeric
/// strings parse, anything else is 0.
pub(crate) fn row_i64(row: &Row, key: &str) -> i64 {
    match row.get(key) {
        Some(serde_json::Value::Number(n)) => {
            n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0)
        }
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().map(|f| f as i64).unwrap_or(0),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::{RowWrite, RowsRequest};
    use serde_json::Map;

    // `TABLES_DIR` is process-wide, so the e2e tests that set it run serially.
    // An async mutex may be held across the tests' await points.
    static E2E_LOCK: std::sync::LazyLock<tokio::sync::Mutex<()>> =
        std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

    fn sqlite_conn(id: &str, dbfile: &std::path::Path) -> StoredConnection {
        StoredConnection {
            id: id.into(),
            name: id.into(),
            kind: "sqlite".into(),
            host: String::new(),
            port: 0,
            database: String::new(),
            username: String::new(),
            password: String::new(),
            color: String::new(),
            filepath: Some(dbfile.to_string_lossy().into_owned()),
            ssl: None,
            ssh: None,
            startup_commands: None,
            safe_mode: None,
            group: None,
            tags: None,
            extra: Map::new(),
        }
    }

    /// The full "connect → browse → rows" path a SQLite workspace exercises,
    /// against a real on-disk database — Host → Registry → async sqlx adapter.
    #[tokio::test]
    async fn end_to_end_sqlite_workspace() {
        let _guard = E2E_LOCK.lock().await;
        let dir = std::env::temp_dir().join(format!("tables_e2e_{}", model::new_uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("TABLES_DIR", &dir);
        let dbfile = dir.join("demo.db");

        // Save a sqlite connection to the store, then connect through the host.
        let conn = StoredConnection {
            id: "e2e".into(),
            name: "E2E".into(),
            kind: "sqlite".into(),
            host: String::new(),
            port: 0,
            database: String::new(),
            username: String::new(),
            password: String::new(),
            color: String::new(),
            filepath: Some(dbfile.to_string_lossy().into_owned()),
            ssl: None,
            ssh: None,
            startup_commands: None,
            safe_mode: None,
            group: None,
            tags: None,
            extra: Map::new(),
        };
        connections::upsert(&conn).unwrap();

        let host = Host::new();
        host.connect("e2e").await.unwrap();

        // Seed schema + rows through the query path.
        host.execute_query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER)")
            .await
            .unwrap();
        host.execute_query("INSERT INTO users (name, age) VALUES ('Alice', 30), ('Bob', 25), ('Charlie', 35)")
            .await
            .unwrap();

        // Browse tables (also makes the connection active).
        let tables = host.list_tables("e2e").await.unwrap();
        assert!(tables.iter().any(|t| t.name == "users" && t.kind == "table"));

        // Structure introspection.
        let structure = host.table_structure("users").await.unwrap();
        assert_eq!(structure.columns.len(), 3);
        assert!(structure.columns.iter().any(|c| c.name == "id" && c.is_primary_key));

        // Paged rows with a sort.
        let rows = host
            .table_rows(&RowsRequest {
                table: "users".into(),
                page: 1,
                page_size: 10,
                sort: Some(model::SortSpec { column: "age".into(), direction: "asc".into() }),
                filters: None,
                filter_logic: None,
            })
            .await
            .unwrap();
        assert_eq!(rows.total, 3);
        assert_eq!(rows.rows.len(), 3);
        assert_eq!(rows.rows[0]["name"], serde_json::json!("Bob")); // youngest first

        // A row edit round-trips.
        let mut pk = Map::new();
        pk.insert("id".into(), serde_json::json!(1));
        let mut changes = Map::new();
        changes.insert("age".into(), serde_json::json!(31));
        host.row_update("users", &pk, &changes).await.unwrap();
        let alice = host
            .table_rows(&RowsRequest {
                table: "users".into(),
                page: 1,
                page_size: 10,
                sort: None,
                filters: Some(vec![model::FilterCondition {
                    id: "1".into(),
                    column: "id".into(),
                    operator: "=".into(),
                    value: "1".into(),
                    value2: None,
                }]),
                filter_logic: Some("and".into()),
            })
            .await
            .unwrap();
        assert_eq!(alice.rows[0]["age"], serde_json::json!(31));

        host.disconnect("e2e").await;
        std::env::remove_var("TABLES_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A reviewed batch whose middle statement fails must leave the database
    /// unchanged (AUD-003).
    #[tokio::test]
    async fn reviewed_batch_rolls_back_on_failure() {
        let _guard = E2E_LOCK.lock().await;
        let dir = std::env::temp_dir().join(format!("tables_tx_{}", model::new_uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("TABLES_DIR", &dir);
        let dbfile = dir.join("tx.db");

        connections::upsert(&sqlite_conn("tx", &dbfile)).unwrap();
        let host = Host::new();
        host.connect("tx").await.unwrap();
        host.execute_query("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .await
            .unwrap();
        host.list_tables("tx").await.unwrap(); // make it active

        // A valid insert followed by one that violates NOT NULL.
        let mut good = Map::new();
        good.insert("id".into(), serde_json::json!(1));
        good.insert("name".into(), serde_json::json!("ok"));
        let mut bad = Map::new();
        bad.insert("id".into(), serde_json::json!(2));
        bad.insert("name".into(), serde_json::Value::Null);
        let writes = vec![
            RowWrite::Insert { table: "t".into(), row: good },
            RowWrite::Insert { table: "t".into(), row: bad },
        ];
        assert!(host.apply_row_writes(&writes).await.is_err());

        // The earlier valid insert must have rolled back.
        let rows = host
            .table_rows(&RowsRequest {
                table: "t".into(),
                page: 1,
                page_size: 10,
                sort: None,
                filters: None,
                filter_logic: None,
            })
            .await
            .unwrap();
        assert_eq!(rows.total, 0, "the whole batch should roll back");

        host.disconnect("tx").await;
        std::env::remove_var("TABLES_DIR");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
