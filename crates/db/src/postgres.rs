//! The Postgres adapter, over sqlx.
//!
//! Statement classification by prefix: SELECT/WITH/SHOW/EXPLAIN/VALUES/TABLE go
//! through the read path (rows_affected = rows.len()), everything else through
//! the write path (rows_affected = the driver count). Multi-statement SQL (which
//! the extended protocol rejects) falls back to the simple protocol via
//! `raw_sql`. Cells are decoded to JSON by matching the column's Postgres type.
//!
//! Credentials go through `PgConnectOptions` (not a hand-built URL), so special
//! characters in passwords are handled correctly.

use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::{Map, Value};
use sqlx::postgres::types::Oid;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions, PgRow, PgSslMode};
use sqlx::types::BigDecimal;
use sqlx::{Column, Executor, Row as _, TypeInfo, ValueRef};

use super::engine::Adapter;
use crate::dialect::Dialect;
use model::{
    ColumnInfo, ConnectionConfig, ForeignKeyInfo, IndexInfo, RawResult, SslConfig, TableInfo,
};

type Row = Map<String, Value>;

pub struct PostgresAdapter {
    config: ConnectionConfig,
    pool: Mutex<Option<PgPool>>,
}

impl PostgresAdapter {
    pub fn new(config: &ConnectionConfig) -> Self {
        PostgresAdapter {
            config: config.clone(),
            pool: Mutex::new(None),
        }
    }

    fn connect_options(&self) -> Result<PgConnectOptions, String> {
        let c = &self.config;
        let mut opts = PgConnectOptions::new()
            .host(&c.host)
            .port(c.port)
            .username(&c.username)
            .password(&c.password)
            .database(&c.database);
        if let Some(ssl) = c.ssl.as_ref().filter(|s| s.mode != "disabled") {
            opts = apply_ssl(opts, ssl);
        }
        Ok(opts)
    }

    fn pool(&self) -> Result<PgPool, String> {
        self.pool
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "Not connected".to_string())
    }
}

#[async_trait]
impl Adapter for PostgresAdapter {
    fn dialect(&self) -> Dialect {
        Dialect::Postgres
    }

    async fn connect(&self) -> Result<(), String> {
        let opts = self.connect_options()?;
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await
            .map_err(err)?;
        sqlx::query("SELECT 1").execute(&pool).await.map_err(err)?;
        *self.pool.lock().unwrap() = Some(pool);
        Ok(())
    }

    async fn disconnect(&self) {
        let pool = self.pool.lock().unwrap().take();
        if let Some(pool) = pool {
            pool.close().await;
        }
    }

    async fn query(&self, sql: &str) -> Result<RawResult, String> {
        let pool = self.pool()?;
        if is_read(sql) {
            match sqlx::query(sql).fetch_all(&pool).await {
                Ok(rows) => {
                    let columns = pg_columns(&pool, &rows, sql).await;
                    Ok(read_result(&rows, columns))
                }
                Err(e) if is_multi(&e) => simple(&pool, sql).await,
                Err(e) => Err(err(e)),
            }
        } else {
            match sqlx::query(sql).execute(&pool).await {
                Ok(done) => Ok(RawResult {
                    rows_affected: done.rows_affected(),
                    ..Default::default()
                }),
                Err(e) if is_multi(&e) => simple(&pool, sql).await,
                Err(e) => Err(err(e)),
            }
        }
    }

    async fn exec_params(&self, sql: &str, params: &[Value]) -> Result<RawResult, String> {
        let pool = self.pool()?;
        if is_read(sql) {
            let q = bind_params!(sqlx::query(sql), params);
            let rows = q.fetch_all(&pool).await.map_err(err)?;
            let columns = pg_columns(&pool, &rows, sql).await;
            Ok(read_result(&rows, columns))
        } else {
            let q = bind_params!(sqlx::query(sql), params);
            let done = q.execute(&pool).await.map_err(err)?;
            Ok(RawResult { rows_affected: done.rows_affected(), ..Default::default() })
        }
    }

    async fn exec_batch(&self, statements: &[String]) -> Result<u64, String> {
        let pool = self.pool()?;
        let mut tx = pool.begin().await.map_err(err)?;
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

    async fn exec_batch_params(&self, batch: &[(String, Vec<Value>)]) -> Result<u64, String> {
        let pool = self.pool()?;
        let mut tx = pool.begin().await.map_err(err)?;
        let mut affected = 0u64;
        for (i, (sql, params)) in batch.iter().enumerate() {
            let q = bind_params!(sqlx::query(sql), params);
            match q.execute(&mut *tx).await {
                Ok(done) => affected += done.rows_affected(),
                Err(e) => return Err(format!("Statement {}: {}", i + 1, err(e))),
            }
        }
        tx.commit().await.map_err(err)?;
        Ok(affected)
    }

    async fn get_tables(&self) -> Result<Vec<TableInfo>, String> {
        const SQL: &str = "SELECT
  t.table_name as name,
  CASE t.table_type WHEN 'BASE TABLE' THEN 'table' ELSE 'view' END as type,
  s.n_live_tup as row_count
FROM information_schema.tables t
LEFT JOIN pg_stat_user_tables s ON s.relname = t.table_name
WHERE t.table_schema = 'public'
ORDER BY t.table_name";
        let pool = self.pool()?;
        let rows = sqlx::query(SQL).fetch_all(&pool).await.map_err(err)?;
        let mut tables: Vec<TableInfo> = rows
            .iter()
            .map(|r| TableInfo {
                name: get_string(r, "name"),
                kind: get_string(r, "type"),
                row_count: r.try_get::<Option<i64>, _>("row_count").ok().flatten(),
            })
            .collect();
        // n_live_tup is 0 until ANALYZE runs — fall back to COUNT(*).
        for table in &mut tables {
            if table.kind == "table" && table.row_count.unwrap_or(0) == 0 {
                let sql = format!(
                    "SELECT COUNT(*)::bigint AS c FROM public.\"{}\"",
                    table.name.replace('"', "\"\"")
                );
                table.row_count = match sqlx::query(&sql).fetch_one(&pool).await {
                    Ok(row) => row.try_get::<i64, _>("c").ok(),
                    Err(_) => None,
                };
            }
        }
        Ok(tables)
    }

    async fn get_columns(&self, table: &str) -> Result<Vec<ColumnInfo>, String> {
        const SQL: &str = "SELECT
  c.column_name as name,
  c.data_type as data_type,
  c.is_nullable = 'YES' as nullable,
  c.column_default as default_value,
  COALESCE(
    (SELECT true FROM information_schema.table_constraints tc
     JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name
     WHERE tc.table_name = $1 AND tc.constraint_type = 'PRIMARY KEY'
     AND kcu.column_name = c.column_name LIMIT 1), false
  ) as is_primary_key,
  pgd.description as comment
FROM information_schema.columns c
LEFT JOIN pg_catalog.pg_statio_all_tables st ON st.relname = c.table_name AND st.schemaname = c.table_schema
LEFT JOIN pg_catalog.pg_description pgd ON pgd.objoid = st.relid AND pgd.objsubid = c.ordinal_position
WHERE c.table_name = $1 AND c.table_schema = 'public'
ORDER BY c.ordinal_position";
        let pool = self.pool()?;
        let rows = sqlx::query(SQL).bind(table).fetch_all(&pool).await.map_err(err)?;
        Ok(rows
            .iter()
            .map(|r| ColumnInfo {
                name: get_string(r, "name"),
                data_type: get_string(r, "data_type"),
                nullable: r.try_get::<Option<bool>, _>("nullable").ok().flatten().unwrap_or(false),
                default_value: get_string_opt(r, "default_value"),
                is_primary_key: r
                    .try_get::<Option<bool>, _>("is_primary_key")
                    .ok()
                    .flatten()
                    .unwrap_or(false),
                comment: get_string_opt(r, "comment"),
            })
            .collect())
    }

    async fn get_indexes(&self, table: &str) -> Result<Vec<IndexInfo>, String> {
        const SQL: &str = "SELECT
  i.relname as name,
  array_agg(a.attname ORDER BY array_position(ix.indkey, a.attnum)) as columns,
  am.amname as type,
  ix.indisunique as is_unique
FROM pg_index ix
JOIN pg_class t ON t.oid = ix.indrelid
JOIN pg_class i ON i.oid = ix.indexrelid
JOIN pg_am am ON am.oid = i.relam
JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
WHERE t.relname = $1
GROUP BY i.relname, am.amname, ix.indisunique
ORDER BY i.relname";
        let pool = self.pool()?;
        let rows = sqlx::query(SQL).bind(table).fetch_all(&pool).await.map_err(err)?;
        Ok(rows
            .iter()
            .map(|r| IndexInfo {
                name: get_string(r, "name"),
                columns: get_string_array(r, "columns"),
                kind: get_string(r, "type"),
                unique: r.try_get::<Option<bool>, _>("is_unique").ok().flatten().unwrap_or(false),
            })
            .collect())
    }

    async fn get_foreign_keys(&self, table: &str) -> Result<Vec<ForeignKeyInfo>, String> {
        const SQL: &str = "SELECT
  tc.constraint_name as name,
  array_agg(DISTINCT kcu.column_name) as columns,
  ccu.table_name as referenced_table,
  array_agg(DISTINCT ccu.column_name) as referenced_columns,
  rc.delete_rule as on_delete,
  rc.update_rule as on_update
FROM information_schema.table_constraints tc
JOIN information_schema.key_column_usage kcu ON tc.constraint_name = kcu.constraint_name
JOIN information_schema.constraint_column_usage ccu ON tc.constraint_name = ccu.constraint_name
JOIN information_schema.referential_constraints rc ON tc.constraint_name = rc.constraint_name
WHERE tc.table_name = $1 AND tc.constraint_type = 'FOREIGN KEY'
GROUP BY tc.constraint_name, ccu.table_name, rc.delete_rule, rc.update_rule";
        let pool = self.pool()?;
        let rows = sqlx::query(SQL).bind(table).fetch_all(&pool).await.map_err(err)?;
        Ok(rows
            .iter()
            .map(|r| ForeignKeyInfo {
                name: get_string(r, "name"),
                columns: get_string_array(r, "columns"),
                referenced_table: get_string(r, "referenced_table"),
                referenced_columns: get_string_array(r, "referenced_columns"),
                on_delete: get_string(r, "on_delete"),
                on_update: get_string(r, "on_update"),
            })
            .collect())
    }

    async fn get_ddl(&self, table: &str) -> Result<String, String> {
        const SQL: &str = "SELECT column_name, data_type, is_nullable, column_default FROM information_schema.columns WHERE table_name = $1 AND table_schema = 'public' ORDER BY ordinal_position";
        let pool = self.pool()?;
        let rows = sqlx::query(SQL).bind(table).fetch_all(&pool).await.map_err(err)?;
        let lines: Vec<String> = rows
            .iter()
            .map(|r| {
                let mut line = format!(
                    "  \"{}\" {}",
                    get_string(r, "column_name"),
                    get_string(r, "data_type")
                );
                if get_string(r, "is_nullable") == "NO" {
                    line.push_str(" NOT NULL");
                }
                if let Some(d) = get_string_opt(r, "column_default").filter(|d| !d.is_empty()) {
                    line.push_str(&format!(" DEFAULT {d}"));
                }
                line
            })
            .collect();
        Ok(format!("CREATE TABLE \"{}\" (\n{}\n);", table, lines.join(",\n")))
    }

    async fn get_version(&self) -> Result<String, String> {
        let pool = self.pool()?;
        let row = sqlx::query("SELECT version()").fetch_one(&pool).await.map_err(err)?;
        Ok(get_string_opt(&row, "version").unwrap_or_else(|| "unknown".into()))
    }

    async fn get_databases(&self) -> Result<Vec<String>, String> {
        const SQL: &str =
            "SELECT datname FROM pg_database WHERE datistemplate = false ORDER BY datname";
        let pool = self.pool()?;
        let rows = sqlx::query(SQL).fetch_all(&pool).await.map_err(err)?;
        Ok(rows.iter().map(|r| get_string(r, "datname")).collect())
    }
}

fn apply_ssl(opts: PgConnectOptions, ssl: &SslConfig) -> PgConnectOptions {
    let mode = match ssl.mode.as_str() {
        "verify-identity" => PgSslMode::VerifyFull,
        "verify-ca" => PgSslMode::VerifyCa,
        "required" => PgSslMode::Require,
        _ => PgSslMode::Prefer,
    };
    let mut opts = opts.ssl_mode(mode);
    if let Some(ca) = ssl.ca.as_deref().filter(|p| !p.is_empty()) {
        opts = opts.ssl_root_cert(ca);
    }
    let cert = ssl.cert.as_deref().filter(|p| !p.is_empty());
    let key = ssl.key.as_deref().filter(|p| !p.is_empty());
    if let (Some(cert), Some(key)) = (cert, key) {
        opts = opts.ssl_client_cert(cert).ssl_client_key(key);
    }
    opts
}

fn err(e: sqlx::Error) -> String {
    // Strip the driver's wrapper; keep just the server message when present.
    match e.as_database_error() {
        Some(db) => db.message().to_string(),
        None => e.to_string(),
    }
}

fn is_multi(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .is_some_and(|d| d.message().contains("cannot insert multiple commands"))
}

fn is_read(sql: &str) -> bool {
    let upper = sql.trim().to_uppercase();
    ["SELECT", "WITH", "SHOW", "EXPLAIN", "VALUES", "TABLE"]
        .iter()
        .any(|p| upper.starts_with(p))
}

async fn pg_columns(pool: &PgPool, rows: &[PgRow], sql: &str) -> Vec<String> {
    if let Some(first) = rows.first() {
        return first.columns().iter().map(|c| c.name().to_string()).collect();
    }
    match pool.describe(sql).await {
        Ok(d) => d.columns().iter().map(|c| c.name().to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

fn read_result(rows: &[PgRow], columns: Vec<String>) -> RawResult {
    let out: Vec<Row> = rows.iter().map(row_to_map).collect();
    let rows_affected = out.len() as u64;
    RawResult {
        columns,
        column_types: Map::new(),
        rows: out,
        rows_affected,
    }
}

/// Multi-statement fallback: run the whole batch over the simple protocol and
/// return every row it produced.
async fn simple(pool: &PgPool, sql: &str) -> Result<RawResult, String> {
    let rows = sqlx::raw_sql(sql).fetch_all(pool).await.map_err(err)?;
    let columns = rows
        .first()
        .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();
    Ok(read_result(&rows, columns))
}

fn row_to_map(row: &PgRow) -> Row {
    let mut map = Map::new();
    for col in row.columns() {
        map.insert(col.name().to_string(), pg_value(row, col.ordinal()));
    }
    map
}

/// Decode one cell to JSON by its Postgres type name.
fn pg_value(row: &PgRow, i: usize) -> Value {
    let Ok(raw) = row.try_get_raw(i) else {
        return Value::Null;
    };
    if raw.is_null() {
        return Value::Null;
    }
    let ty = raw.type_info().name().to_uppercase();
    match ty.as_str() {
        "BOOL" => try_json::<bool>(row, i),
        "INT2" => row.try_get::<i16, _>(i).map(|v| Value::from(v as i64)).unwrap_or(Value::Null),
        "INT4" => row.try_get::<i32, _>(i).map(|v| Value::from(v as i64)).unwrap_or(Value::Null),
        "INT8" => try_json::<i64>(row, i),
        "OID" => row.try_get::<Oid, _>(i).map(|v| Value::from(v.0 as i64)).unwrap_or(Value::Null),
        "FLOAT4" => row.try_get::<f32, _>(i).map(|v| json_f64(v as f64)).unwrap_or(Value::Null),
        "FLOAT8" => row.try_get::<f64, _>(i).map(json_f64).unwrap_or(Value::Null),
        "NUMERIC" => row
            .try_get::<BigDecimal, _>(i)
            .map(numeric_json)
            .unwrap_or(Value::Null),
        "JSON" | "JSONB" => row.try_get::<Value, _>(i).unwrap_or(Value::Null),
        "UUID" => row
            .try_get::<uuid::Uuid, _>(i)
            .map(|u| Value::String(u.to_string()))
            .unwrap_or(Value::Null),
        "TIMESTAMP" => row
            .try_get::<chrono::NaiveDateTime, _>(i)
            .map(|t| Value::String(t.format("%Y-%m-%dT%H:%M:%S%.3f").to_string()))
            .unwrap_or(Value::Null),
        "TIMESTAMPTZ" => row
            .try_get::<chrono::DateTime<chrono::Utc>, _>(i)
            .map(|t| Value::String(t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()))
            .unwrap_or(Value::Null),
        "DATE" => row
            .try_get::<chrono::NaiveDate, _>(i)
            .map(|t| Value::String(t.format("%Y-%m-%d").to_string()))
            .unwrap_or(Value::Null),
        "TIME" => row
            .try_get::<chrono::NaiveTime, _>(i)
            .map(|t| Value::String(t.format("%H:%M:%S%.f").to_string()))
            .unwrap_or(Value::Null),
        "BYTEA" => row
            .try_get::<Vec<u8>, _>(i)
            .map(|b| Value::String(String::from_utf8_lossy(&b).into_owned()))
            .unwrap_or(Value::Null),
        n if n.ends_with("[]") => pg_array(row, i, n),
        _ => try_json::<String>(row, i),
    }
}

fn pg_array(row: &PgRow, i: usize, name: &str) -> Value {
    let element = name.trim_end_matches("[]");
    let arr = match element {
        "INT2" => row.try_get::<Vec<i16>, _>(i).map(|v| v.into_iter().map(|n| Value::from(n as i64)).collect()),
        "INT4" => row.try_get::<Vec<i32>, _>(i).map(|v| v.into_iter().map(|n| Value::from(n as i64)).collect()),
        "INT8" => row.try_get::<Vec<i64>, _>(i).map(|v| v.into_iter().map(Value::from).collect()),
        "FLOAT4" => row.try_get::<Vec<f32>, _>(i).map(|v| v.into_iter().map(|n| json_f64(n as f64)).collect()),
        "FLOAT8" => row.try_get::<Vec<f64>, _>(i).map(|v| v.into_iter().map(json_f64).collect()),
        "BOOL" => row.try_get::<Vec<bool>, _>(i).map(|v| v.into_iter().map(Value::from).collect()),
        _ => row
            .try_get::<Vec<String>, _>(i)
            .map(|v| v.into_iter().map(Value::from).collect()),
    };
    arr.map(Value::Array).unwrap_or(Value::Null)
}

fn try_json<'r, T>(row: &'r PgRow, i: usize) -> Value
where
    T: sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Into<Value>,
{
    row.try_get::<T, _>(i).map(Into::into).unwrap_or(Value::Null)
}

fn json_f64(f: f64) -> Value {
    serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null)
}

/// NUMERIC → a JSON number when it parses finitely, else its decimal string
/// (preserving precision the way the original did).
fn numeric_json(d: BigDecimal) -> Value {
    let s = d.to_string();
    s.parse::<f64>()
        .ok()
        .filter(|f| f.is_finite())
        .and_then(serde_json::Number::from_f64)
        .map(Value::Number)
        .unwrap_or(Value::String(s))
}

fn get_string(row: &PgRow, name: &str) -> String {
    get_string_opt(row, name).unwrap_or_default()
}

fn get_string_opt(row: &PgRow, name: &str) -> Option<String> {
    row.try_get::<Option<String>, _>(name).ok().flatten()
}

fn get_string_array(row: &PgRow, name: &str) -> Vec<String> {
    row.try_get::<Vec<String>, _>(name).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_reads_and_writes() {
        assert!(is_read("  select 1"));
        assert!(is_read("WITH x AS (SELECT 1) SELECT * FROM x"));
        assert!(is_read("SHOW server_version"));
        assert!(is_read("EXPLAIN SELECT 1"));
        assert!(is_read("VALUES (1)"));
        assert!(is_read("TABLE users"));
        assert!(!is_read("INSERT INTO t VALUES (1)"));
        assert!(!is_read("UPDATE t SET a = 1"));
        assert!(!is_read("SET search_path TO app"));
    }

    #[test]
    fn numeric_json_parses_or_falls_back() {
        use std::str::FromStr;
        let d = BigDecimal::from_str("12345.678").unwrap();
        assert_eq!(numeric_json(d), serde_json::json!(12345.678));
    }
}
