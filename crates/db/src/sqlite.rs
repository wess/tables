//! The SQLite adapter, over sqlx.
//!
//! Holds one `SqliteConnection` behind an async mutex — the closest analogue of
//! the original single-connection design, and the only shape under which an
//! in-memory database survives across statements.

use async_trait::async_trait;
use serde_json::{Map, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection, SqliteRow};
use sqlx::{Column, ConnectOptions, Connection, Executor, Row as _, TypeInfo, ValueRef};
use tokio::sync::Mutex;

use super::engine::Adapter;
use crate::dialect::Dialect;
use model::{ColumnInfo, ConnectionConfig, ForeignKeyInfo, IndexInfo, RawResult, TableInfo};

type Row = Map<String, Value>;

pub struct SqliteAdapter {
    path: String,
    conn: Mutex<Option<SqliteConnection>>,
}

impl SqliteAdapter {
    pub fn new(config: &ConnectionConfig) -> Self {
        let path = config
            .filepath
            .clone()
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| config.database.clone());
        SqliteAdapter {
            path,
            conn: Mutex::new(None),
        }
    }
}

#[async_trait]
impl Adapter for SqliteAdapter {
    fn dialect(&self) -> Dialect {
        Dialect::Sqlite
    }

    async fn connect(&self) -> Result<(), String> {
        let mut conn = SqliteConnectOptions::new()
            .filename(&self.path)
            .create_if_missing(true)
            .connect()
            .await
            .map_err(err)?;
        sqlx::query("SELECT 1").execute(&mut conn).await.map_err(err)?;
        *self.conn.lock().await = Some(conn);
        Ok(())
    }

    async fn disconnect(&self) {
        *self.conn.lock().await = None;
    }

    async fn query(&self, sql: &str) -> Result<RawResult, String> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(not_connected)?;
        if is_read(sql) {
            let sqlite_rows = sqlx::query(sql).fetch_all(&mut *conn).await.map_err(err)?;
            let columns = if let Some(first) = sqlite_rows.first() {
                column_names(first)
            } else {
                // Preserve the column headers for an empty result so the grid
                // can still render them.
                match conn.describe(sql).await {
                    Ok(d) => d.columns().iter().map(|c| c.name().to_string()).collect(),
                    Err(_) => Vec::new(),
                }
            };
            let rows: Vec<Row> = sqlite_rows.iter().map(row_to_map).collect();
            let rows_affected = rows.len() as u64;
            Ok(RawResult {
                columns,
                column_types: Map::new(),
                rows,
                rows_affected,
            })
        } else {
            let done = sqlx::query(sql).execute(&mut *conn).await.map_err(err)?;
            Ok(RawResult {
                rows_affected: done.rows_affected(),
                ..Default::default()
            })
        }
    }

    async fn exec_batch(&self, statements: &[String]) -> Result<u64, String> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(not_connected)?;
        let mut tx = conn.begin().await.map_err(err)?;
        let mut affected = 0u64;
        for (i, stmt) in statements.iter().enumerate() {
            match sqlx::query(stmt).execute(&mut *tx).await {
                Ok(done) => affected += done.rows_affected(),
                Err(e) => return Err(format!("Statement {}: {}", i + 1, err(e))),
            }
        }
        tx.commit().await.map_err(err)?;
        Ok(affected)
    }

    async fn get_tables(&self) -> Result<Vec<TableInfo>, String> {
        const SQL: &str = "SELECT name, type FROM sqlite_master
WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%'
ORDER BY name";
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(not_connected)?;
        let rows = fetch_maps(conn, SQL).await?;
        Ok(rows
            .iter()
            .map(|r| TableInfo {
                name: text(r.get("name")),
                kind: text(r.get("type")),
                row_count: None,
            })
            .collect())
    }

    async fn get_columns(&self, table: &str) -> Result<Vec<ColumnInfo>, String> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(not_connected)?;
        let rows = fetch_maps(conn, &format!("PRAGMA table_info(\"{table}\")")).await?;
        Ok(rows
            .iter()
            .map(|r| {
                let data_type = text(r.get("type"));
                ColumnInfo {
                    name: text(r.get("name")),
                    data_type: if data_type.is_empty() { "TEXT".into() } else { data_type },
                    nullable: int(r.get("notnull")) == Some(0),
                    default_value: text_opt(r.get("dflt_value")),
                    is_primary_key: int(r.get("pk")) == Some(1),
                    comment: None,
                }
            })
            .collect())
    }

    async fn get_indexes(&self, table: &str) -> Result<Vec<IndexInfo>, String> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(not_connected)?;
        let list = fetch_maps(conn, &format!("PRAGMA index_list(\"{table}\")")).await?;
        let mut indexes = Vec::with_capacity(list.len());
        for idx in &list {
            let name = text(idx.get("name"));
            let info = fetch_maps(conn, &format!("PRAGMA index_info(\"{name}\")")).await?;
            indexes.push(IndexInfo {
                columns: info.iter().map(|r| text(r.get("name"))).collect(),
                kind: if text(idx.get("origin")) == "pk" { "PRIMARY".into() } else { "BTREE".into() },
                unique: int(idx.get("unique")) == Some(1),
                name,
            });
        }
        Ok(indexes)
    }

    async fn get_foreign_keys(&self, table: &str) -> Result<Vec<ForeignKeyInfo>, String> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(not_connected)?;
        let rows = fetch_maps(conn, &format!("PRAGMA foreign_key_list(\"{table}\")")).await?;
        // Rows grouped by id in encounter order — one FK per id, named fk_<id>.
        let mut groups: Vec<(i64, ForeignKeyInfo)> = Vec::new();
        for r in &rows {
            let id = int(r.get("id")).unwrap_or(0);
            let from = text(r.get("from"));
            let to = text(r.get("to"));
            if let Some((_, fk)) = groups.iter_mut().find(|(gid, _)| *gid == id) {
                fk.columns.push(from);
                fk.referenced_columns.push(to);
            } else {
                groups.push((
                    id,
                    ForeignKeyInfo {
                        name: format!("fk_{id}"),
                        columns: vec![from],
                        referenced_table: text(r.get("table")),
                        referenced_columns: vec![to],
                        on_delete: text(r.get("on_delete")),
                        on_update: text(r.get("on_update")),
                    },
                ));
            }
        }
        Ok(groups.into_iter().map(|(_, fk)| fk).collect())
    }

    async fn get_ddl(&self, table: &str) -> Result<String, String> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(not_connected)?;
        let sqlite_rows = sqlx::query("SELECT sql FROM sqlite_master WHERE name = ?1")
            .bind(table)
            .fetch_all(&mut *conn)
            .await
            .map_err(err)?;
        let rows: Vec<Row> = sqlite_rows.iter().map(row_to_map).collect();
        Ok(rows
            .first()
            .and_then(|r| text_opt(r.get("sql")))
            .unwrap_or_default())
    }

    async fn get_version(&self) -> Result<String, String> {
        let mut guard = self.conn.lock().await;
        let conn = guard.as_mut().ok_or_else(not_connected)?;
        let rows = fetch_maps(conn, "SELECT sqlite_version() as v").await?;
        let v = rows
            .first()
            .and_then(|r| text_opt(r.get("v")))
            .unwrap_or_else(|| "unknown".into());
        Ok(format!("SQLite {v}"))
    }

    async fn get_databases(&self) -> Result<Vec<String>, String> {
        self.conn.lock().await.as_ref().ok_or_else(not_connected)?;
        let name = std::path::Path::new(&self.path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "main".into());
        Ok(vec![name])
    }
}

fn not_connected() -> String {
    "Not connected".to_string()
}

fn err(e: sqlx::Error) -> String {
    e.to_string()
}

fn is_read(sql: &str) -> bool {
    let upper = sql.trim().to_uppercase();
    ["SELECT", "PRAGMA", "WITH"].iter().any(|p| upper.starts_with(p))
}

async fn fetch_maps(conn: &mut SqliteConnection, sql: &str) -> Result<Vec<Row>, String> {
    let rows = sqlx::query(sql).fetch_all(&mut *conn).await.map_err(err)?;
    Ok(rows.iter().map(row_to_map).collect())
}

fn column_names(row: &SqliteRow) -> Vec<String> {
    row.columns().iter().map(|c| c.name().to_string()).collect()
}

fn row_to_map(row: &SqliteRow) -> Row {
    let mut map = Map::new();
    for col in row.columns() {
        map.insert(col.name().to_string(), sqlite_value(row, col.ordinal()));
    }
    map
}

/// Decode one cell by its storage class: INTEGER → number, REAL → number,
/// TEXT → string, BLOB → utf8-lossy string.
fn sqlite_value(row: &SqliteRow, i: usize) -> Value {
    let Ok(raw) = row.try_get_raw(i) else {
        return Value::Null;
    };
    if raw.is_null() {
        return Value::Null;
    }
    let ty = raw.type_info().name().to_uppercase();
    if ty.contains("INT") {
        row.try_get::<i64, _>(i).map(Value::from).unwrap_or(Value::Null)
    } else if ty.contains("REAL") || ty.contains("FLOA") || ty.contains("DOUB") || ty == "NUMERIC" {
        row.try_get::<f64, _>(i)
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    } else if ty.contains("BLOB") {
        row.try_get::<Vec<u8>, _>(i)
            .map(|b| Value::String(String::from_utf8_lossy(&b).into_owned()))
            .unwrap_or(Value::Null)
    } else {
        row.try_get::<String, _>(i)
            .map(Value::String)
            .unwrap_or(Value::Null)
    }
}

fn text(v: Option<&Value>) -> String {
    text_opt(v).unwrap_or_default()
}

fn text_opt(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(database: &str, filepath: Option<&str>) -> ConnectionConfig {
        ConnectionConfig {
            id: "test".into(),
            kind: "sqlite".into(),
            host: String::new(),
            port: 0,
            database: database.into(),
            username: String::new(),
            password: String::new(),
            filepath: filepath.map(String::from),
            ssl: None,
            ssh: None,
            startup_commands: None,
        }
    }

    async fn memory() -> SqliteAdapter {
        let adapter = SqliteAdapter::new(&config(":memory:", None));
        adapter.connect().await.unwrap();
        adapter
    }

    #[tokio::test]
    async fn requires_connect() {
        let adapter = SqliteAdapter::new(&config(":memory:", None));
        assert_eq!(adapter.query("SELECT 1").await.unwrap_err(), "Not connected");
    }

    #[tokio::test]
    async fn classifies_reads_and_writes() {
        let db = memory().await;
        let created = db.query("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)").await.unwrap();
        assert_eq!(created.rows_affected, 0);
        let inserted = db.query("INSERT INTO t (name) VALUES ('a'), ('b')").await.unwrap();
        assert_eq!(inserted.rows_affected, 2);
        let read = db.query("select name from t order by name").await.unwrap();
        assert_eq!(read.rows_affected, 2);
        assert_eq!(read.columns, vec!["name"]);
        assert_eq!(read.rows[0]["name"], serde_json::json!("a"));
        let empty = db.query("SELECT * FROM t WHERE id = 99").await.unwrap();
        assert_eq!(empty.columns, vec!["id", "name"]);
        assert!(empty.rows.is_empty());
        assert_eq!(empty.rows_affected, 0);
    }

    #[tokio::test]
    async fn introspects_schema() {
        let db = memory().await;
        db.query("CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL, age INT DEFAULT 21)")
            .await
            .unwrap();
        db.query("CREATE UNIQUE INDEX idx_users_email ON users(email)").await.unwrap();
        db.query("CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INT REFERENCES users(id) ON DELETE CASCADE)")
            .await
            .unwrap();
        db.query("CREATE VIEW v_users AS SELECT * FROM users").await.unwrap();

        let tables = db.get_tables().await.unwrap();
        let names: Vec<_> = tables.iter().map(|t| (t.name.as_str(), t.kind.as_str())).collect();
        assert_eq!(names, vec![("posts", "table"), ("users", "table"), ("v_users", "view")]);
        assert!(tables.iter().all(|t| t.row_count.is_none()));

        let columns = db.get_columns("users").await.unwrap();
        assert_eq!(columns[0].name, "id");
        assert!(columns[0].is_primary_key);
        assert!(columns[0].nullable);
        assert!(!columns[1].nullable);
        assert_eq!(columns[2].default_value.as_deref(), Some("21"));
        assert!(columns.iter().all(|c| c.comment.is_none()));

        let indexes = db.get_indexes("users").await.unwrap();
        let email = indexes.iter().find(|i| i.name == "idx_users_email").unwrap();
        assert_eq!(email.columns, vec!["email"]);
        assert!(email.unique);
        assert_eq!(email.kind, "BTREE");

        let fks = db.get_foreign_keys("posts").await.unwrap();
        assert_eq!(fks.len(), 1);
        assert_eq!(fks[0].name, "fk_0");
        assert_eq!(fks[0].columns, vec!["user_id"]);
        assert_eq!(fks[0].referenced_table, "users");
        assert_eq!(fks[0].referenced_columns, vec!["id"]);
        assert_eq!(fks[0].on_delete, "CASCADE");
        assert_eq!(fks[0].on_update, "NO ACTION");

        let ddl = db.get_ddl("users").await.unwrap();
        assert!(ddl.starts_with("CREATE TABLE users"));
        assert_eq!(db.get_ddl("missing").await.unwrap(), "");

        assert!(db.get_version().await.unwrap().starts_with("SQLite "));
        assert_eq!(db.get_databases().await.unwrap(), vec![":memory:"]);
    }

    #[tokio::test]
    async fn databases_uses_basename() {
        let path = std::env::temp_dir().join(format!("tables_test_{}.sqlite", model::new_uuid()));
        let adapter = SqliteAdapter::new(&config("app", Some(&path.to_string_lossy())));
        adapter.connect().await.unwrap();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(adapter.get_databases().await.unwrap(), vec![name]);
        let _ = std::fs::remove_file(&path);
    }
}
