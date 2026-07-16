//! The MySQL/MariaDB adapter, over sqlx.
//!
//! sqlx always uses the prepared (binary) protocol, so flag expressions like
//! `COLUMN_KEY = 'PRI'` arrive as integers — matching the original. Reads vs.
//! writes are split by statement prefix (sqlx needs to know which call to make);
//! multi-statement SQL falls back to `raw_sql`.

use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::{Map, Value};
use sqlx::mysql::{MySqlConnectOptions, MySqlPool, MySqlPoolOptions, MySqlRow, MySqlSslMode};
use sqlx::types::BigDecimal;
use sqlx::{Column, Executor, Row as _, TypeInfo, ValueRef};

use super::engine::Adapter;
use crate::dialect::Dialect;
use model::{
    ColumnInfo, ConnectionConfig, ForeignKeyInfo, IndexInfo, RawResult, SslConfig, TableInfo,
};

type Row = Map<String, Value>;

pub struct MysqlAdapter {
    config: ConnectionConfig,
    pool: Mutex<Option<MySqlPool>>,
}

impl MysqlAdapter {
    pub fn new(config: &ConnectionConfig) -> Self {
        MysqlAdapter {
            config: config.clone(),
            pool: Mutex::new(None),
        }
    }

    fn connect_options(&self) -> MySqlConnectOptions {
        let c = &self.config;
        let mut opts = MySqlConnectOptions::new()
            .host(&c.host)
            .port(c.port)
            .username(&c.username)
            .password(&c.password)
            .database(&c.database);
        if let Some(ssl) = c.ssl.as_ref().filter(|s| s.mode != "disabled") {
            opts = apply_ssl(opts, ssl);
        }
        opts
    }

    fn pool(&self) -> Result<MySqlPool, String> {
        self.pool
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "Not connected".to_string())
    }
}

#[async_trait]
impl Adapter for MysqlAdapter {
    fn dialect(&self) -> Dialect {
        Dialect::Mysql
    }

    async fn connect(&self) -> Result<(), String> {
        let pool = MySqlPoolOptions::new()
            .max_connections(5)
            .connect_with(self.connect_options())
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
                    let columns = my_columns(&pool, &rows, sql).await;
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

    async fn get_tables(&self) -> Result<Vec<TableInfo>, String> {
        const SQL: &str = "SELECT
  TABLE_NAME as name,
  CASE TABLE_TYPE WHEN 'BASE TABLE' THEN 'table' ELSE 'view' END as type,
  TABLE_ROWS as row_count
FROM information_schema.TABLES
WHERE TABLE_SCHEMA = DATABASE()
ORDER BY TABLE_NAME";
        let pool = self.pool()?;
        let rows = fetch_maps(&pool, sqlx::query(SQL)).await?;
        let mut tables: Vec<TableInfo> = rows
            .iter()
            .map(|r| TableInfo {
                name: text(r.get("name")),
                kind: text(r.get("type")),
                row_count: int(r.get("row_count")),
            })
            .collect();
        // TABLE_ROWS is an estimate and often 0 — fall back to COUNT(*).
        for table in &mut tables {
            if table.kind == "table" && table.row_count.unwrap_or(0) == 0 {
                let sql = format!("SELECT COUNT(*) AS c FROM `{}`", table.name.replace('`', "``"));
                table.row_count = match fetch_maps(&pool, sqlx::query(&sql)).await {
                    Ok(rows) => rows.first().and_then(|r| int(r.get("c"))),
                    Err(_) => None,
                };
            }
        }
        Ok(tables)
    }

    async fn get_columns(&self, table: &str) -> Result<Vec<ColumnInfo>, String> {
        const SQL: &str = "SELECT
  COLUMN_NAME as name,
  COLUMN_TYPE as data_type,
  IS_NULLABLE = 'YES' as nullable,
  COLUMN_DEFAULT as default_value,
  COLUMN_KEY = 'PRI' as is_primary_key,
  COLUMN_COMMENT as comment
FROM information_schema.COLUMNS
WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?
ORDER BY ORDINAL_POSITION";
        let pool = self.pool()?;
        let rows = fetch_maps(&pool, sqlx::query(SQL).bind(table)).await?;
        Ok(rows
            .iter()
            .map(|r| ColumnInfo {
                name: text(r.get("name")),
                data_type: text(r.get("data_type")),
                nullable: truthy(r.get("nullable")),
                default_value: text_opt(r.get("default_value")),
                is_primary_key: truthy(r.get("is_primary_key")),
                comment: text_opt(r.get("comment")).filter(|c| !c.is_empty()),
            })
            .collect())
    }

    async fn get_indexes(&self, table: &str) -> Result<Vec<IndexInfo>, String> {
        const SQL: &str = "SELECT
  INDEX_NAME as name,
  GROUP_CONCAT(COLUMN_NAME ORDER BY SEQ_IN_INDEX) as columns_str,
  INDEX_TYPE as type,
  NOT NON_UNIQUE as is_unique
FROM information_schema.STATISTICS
WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ?
GROUP BY INDEX_NAME, INDEX_TYPE, NON_UNIQUE
ORDER BY INDEX_NAME";
        let pool = self.pool()?;
        let rows = fetch_maps(&pool, sqlx::query(SQL).bind(table)).await?;
        Ok(rows
            .iter()
            .map(|r| IndexInfo {
                name: text(r.get("name")),
                columns: split_list(&text(r.get("columns_str"))),
                kind: text(r.get("type")),
                unique: truthy(r.get("is_unique")),
            })
            .collect())
    }

    async fn get_foreign_keys(&self, table: &str) -> Result<Vec<ForeignKeyInfo>, String> {
        const SQL: &str = "SELECT
  CONSTRAINT_NAME as name,
  GROUP_CONCAT(DISTINCT COLUMN_NAME) as columns_str,
  REFERENCED_TABLE_NAME as referenced_table,
  GROUP_CONCAT(DISTINCT REFERENCED_COLUMN_NAME) as ref_columns_str
FROM information_schema.KEY_COLUMN_USAGE
WHERE TABLE_SCHEMA = DATABASE()
  AND TABLE_NAME = ?
  AND REFERENCED_TABLE_NAME IS NOT NULL
GROUP BY CONSTRAINT_NAME, REFERENCED_TABLE_NAME";
        const ACTIONS_SQL: &str = "SELECT CONSTRAINT_NAME as name, DELETE_RULE as on_delete, UPDATE_RULE as on_update
FROM information_schema.REFERENTIAL_CONSTRAINTS
WHERE CONSTRAINT_SCHEMA = DATABASE() AND TABLE_NAME = ?";
        let pool = self.pool()?;
        let fks = fetch_maps(&pool, sqlx::query(SQL).bind(table)).await?;
        let actions = fetch_maps(&pool, sqlx::query(ACTIONS_SQL).bind(table)).await?;
        Ok(fks
            .iter()
            .map(|r| {
                let name = text(r.get("name"));
                let action = actions.iter().find(|a| text(a.get("name")) == name);
                ForeignKeyInfo {
                    columns: split_list(&text(r.get("columns_str"))),
                    referenced_table: text(r.get("referenced_table")),
                    referenced_columns: split_list(&text(r.get("ref_columns_str"))),
                    on_delete: action
                        .map(|a| text(a.get("on_delete")))
                        .unwrap_or_else(|| "NO ACTION".into()),
                    on_update: action
                        .map(|a| text(a.get("on_update")))
                        .unwrap_or_else(|| "NO ACTION".into()),
                    name,
                }
            })
            .collect())
    }

    async fn get_ddl(&self, table: &str) -> Result<String, String> {
        let pool = self.pool()?;
        let rows = fetch_maps(&pool, sqlx::query(&format!("SHOW CREATE TABLE `{table}`"))).await?;
        Ok(rows
            .first()
            .and_then(|r| {
                text_opt(r.get("Create Table"))
                    .filter(|s| !s.is_empty())
                    .or_else(|| text_opt(r.get("Create View")).filter(|s| !s.is_empty()))
            })
            .unwrap_or_default())
    }

    async fn get_version(&self) -> Result<String, String> {
        let pool = self.pool()?;
        let rows = fetch_maps(&pool, sqlx::query("SELECT VERSION() as v")).await?;
        let v = rows
            .first()
            .and_then(|r| text_opt(r.get("v")))
            .unwrap_or_else(|| "unknown".into());
        Ok(format!("MySQL {v}"))
    }

    async fn get_databases(&self) -> Result<Vec<String>, String> {
        let pool = self.pool()?;
        let rows = fetch_maps(&pool, sqlx::query("SHOW DATABASES")).await?;
        Ok(rows.iter().map(|r| text(r.get("Database"))).collect())
    }
}

fn apply_ssl(opts: MySqlConnectOptions, ssl: &SslConfig) -> MySqlConnectOptions {
    let mode = match ssl.mode.as_str() {
        "verify-identity" => MySqlSslMode::VerifyIdentity,
        "verify-ca" => MySqlSslMode::VerifyCa,
        "required" => MySqlSslMode::Required,
        _ => MySqlSslMode::Preferred,
    };
    let mut opts = opts.ssl_mode(mode);
    if let Some(ca) = ssl.ca.as_deref().filter(|p| !p.is_empty()) {
        opts = opts.ssl_ca(ca);
    }
    let cert = ssl.cert.as_deref().filter(|p| !p.is_empty());
    let key = ssl.key.as_deref().filter(|p| !p.is_empty());
    if let (Some(cert), Some(key)) = (cert, key) {
        opts = opts.ssl_client_cert(cert).ssl_client_key(key);
    }
    opts
}

fn err(e: sqlx::Error) -> String {
    match e.as_database_error() {
        Some(db) => db.message().to_string(),
        None => e.to_string(),
    }
}

fn is_multi(e: &sqlx::Error) -> bool {
    // MySQL rejects multi-statement text in a prepared call with error 1064/1295.
    e.as_database_error().is_some_and(|d| {
        let m = d.message();
        m.contains("multiple statements") || m.contains("right syntax")
    })
}

fn is_read(sql: &str) -> bool {
    let upper = sql.trim().to_uppercase();
    [
        "SELECT", "WITH", "SHOW", "EXPLAIN", "DESCRIBE", "DESC", "VALUES", "TABLE", "CALL",
    ]
    .iter()
    .any(|p| upper.starts_with(p))
}

async fn my_columns(pool: &MySqlPool, rows: &[MySqlRow], sql: &str) -> Vec<String> {
    if let Some(first) = rows.first() {
        return first.columns().iter().map(|c| c.name().to_string()).collect();
    }
    match pool.describe(sql).await {
        Ok(d) => d.columns().iter().map(|c| c.name().to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

async fn fetch_maps<'q>(
    pool: &MySqlPool,
    query: sqlx::query::Query<'q, sqlx::MySql, sqlx::mysql::MySqlArguments>,
) -> Result<Vec<Row>, String> {
    let rows = query.fetch_all(pool).await.map_err(err)?;
    Ok(rows.iter().map(row_to_map).collect())
}

fn read_result(rows: &[MySqlRow], columns: Vec<String>) -> RawResult {
    let out: Vec<Row> = rows.iter().map(row_to_map).collect();
    let rows_affected = out.len() as u64;
    RawResult {
        columns,
        column_types: Map::new(),
        rows: out,
        rows_affected,
    }
}

async fn simple(pool: &MySqlPool, sql: &str) -> Result<RawResult, String> {
    let rows = sqlx::raw_sql(sql).fetch_all(pool).await.map_err(err)?;
    let columns = rows
        .first()
        .map(|r| r.columns().iter().map(|c| c.name().to_string()).collect())
        .unwrap_or_default();
    Ok(read_result(&rows, columns))
}

fn row_to_map(row: &MySqlRow) -> Row {
    let mut map = Map::new();
    for col in row.columns() {
        map.insert(col.name().to_string(), my_value(row, col.ordinal()));
    }
    map
}

/// Decode one cell to JSON by its MySQL type name.
fn my_value(row: &MySqlRow, i: usize) -> Value {
    let Ok(raw) = row.try_get_raw(i) else {
        return Value::Null;
    };
    if raw.is_null() {
        return Value::Null;
    }
    let ty = raw.type_info().name().to_uppercase();
    if ty.contains("DECIMAL") {
        row.try_get::<BigDecimal, _>(i).map(numeric_json).unwrap_or(Value::Null)
    } else if ty.contains("DATETIME") || ty.contains("TIMESTAMP") {
        row.try_get::<chrono::NaiveDateTime, _>(i)
            .map(|t| Value::String(t.format("%Y-%m-%d %H:%M:%S").to_string()))
            .unwrap_or(Value::Null)
    } else if ty == "DATE" {
        row.try_get::<chrono::NaiveDate, _>(i)
            .map(|t| Value::String(t.format("%Y-%m-%d").to_string()))
            .unwrap_or(Value::Null)
    } else if ty == "TIME" {
        row.try_get::<chrono::NaiveTime, _>(i)
            .map(|t| Value::String(t.format("%H:%M:%S").to_string()))
            .unwrap_or(Value::Null)
    } else if ty == "YEAR" {
        row.try_get::<i64, _>(i).map(Value::from).unwrap_or(Value::Null)
    } else if ty.contains("INT") {
        row.try_get::<i64, _>(i)
            .map(Value::from)
            .or_else(|_| row.try_get::<u64, _>(i).map(Value::from))
            .unwrap_or(Value::Null)
    } else if ty.contains("DOUBLE") || ty.contains("REAL") {
        row.try_get::<f64, _>(i).map(json_f64).unwrap_or(Value::Null)
    } else if ty.contains("FLOAT") {
        row.try_get::<f32, _>(i).map(|f| json_f64(f as f64)).unwrap_or(Value::Null)
    } else if ty == "JSON" {
        row.try_get::<Value, _>(i).unwrap_or(Value::Null)
    } else if ty.contains("BLOB") || ty.contains("BINARY") || ty == "BIT" {
        row.try_get::<Vec<u8>, _>(i)
            .map(|b| Value::String(String::from_utf8_lossy(&b).into_owned()))
            .unwrap_or(Value::Null)
    } else {
        row.try_get::<String, _>(i)
            .map(Value::String)
            .or_else(|_| {
                row.try_get::<Vec<u8>, _>(i)
                    .map(|b| Value::String(String::from_utf8_lossy(&b).into_owned()))
            })
            .unwrap_or(Value::Null)
    }
}

fn json_f64(f: f64) -> Value {
    serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null)
}

fn numeric_json(d: BigDecimal) -> Value {
    let s = d.to_string();
    s.parse::<f64>()
        .ok()
        .filter(|f| f.is_finite())
        .and_then(serde_json::Number::from_f64)
        .map(Value::Number)
        .unwrap_or(Value::String(s))
}

fn split_list(s: &str) -> Vec<String> {
    s.split(',').map(str::to_string).collect()
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

fn truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0) != 0.0,
        Some(Value::String(s)) => !s.is_empty(),
        _ => false,
    }
}

fn int(v: Option<&Value>) -> Option<i64> {
    match v? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s
            .trim()
            .parse::<i64>()
            .ok()
            .or_else(|| s.trim().parse::<f64>().ok().map(|f| f as i64)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_reads_and_writes() {
        assert!(is_read("select 1"));
        assert!(is_read("SHOW TABLES"));
        assert!(is_read("DESCRIBE users"));
        assert!(!is_read("INSERT INTO t VALUES (1)"));
        assert!(!is_read("UPDATE t SET a = 1"));
    }

    #[test]
    fn splits_group_concat() {
        assert_eq!(split_list("a,b"), vec!["a", "b"]);
        assert_eq!(split_list(""), vec![""]);
    }

    #[test]
    fn value_helpers() {
        assert_eq!(text(Some(&Value::String("x".into()))), "x");
        assert_eq!(text(Some(&Value::Null)), "");
        assert!(truthy(Some(&Value::Number(1.into()))));
        assert!(!truthy(Some(&Value::Number(0.into()))));
        assert_eq!(int(Some(&serde_json::json!(7))), Some(7));
    }
}
