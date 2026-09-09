//! The engine-agnostic adapter contract.
//!
//! Adapters are async and hold their own sqlx pool/connection; the app drives
//! them on a tokio runtime. Errors are plain strings (the driver's message).

use std::sync::Arc;

use async_trait::async_trait;
use model::{ColumnInfo, ConnectionConfig, ForeignKeyInfo, IndexInfo, RawResult, TableInfo};
use serde_json::Value;

use crate::dialect::Dialect;

#[async_trait]
pub trait Adapter: Send + Sync {
    /// The SQL text dialect (identifier/string quoting) for this engine.
    fn dialect(&self) -> Dialect;
    async fn connect(&self) -> Result<(), String>;
    async fn disconnect(&self);
    async fn query(&self, sql: &str) -> Result<RawResult, String>;
    /// Decode incrementally, rejecting results beyond the caller's budget.
    async fn query_bounded(
        &self,
        sql: &str,
        max_rows: usize,
        max_bytes: usize,
    ) -> Result<RawResult, String>;
    /// Stream one result through a bounded channel. Dropping the receiver stops the read.
    async fn stream_rows(
        &self,
        sql: &str,
        tx: tokio::sync::mpsc::Sender<model::Row>,
    ) -> Result<(), String>;
    /// Execute a statement with bound parameters (values sent as typed binds,
    /// never interpolated). Reads return rows; writes return the affected count.
    /// Placeholders are engine-specific — build the SQL with `Dialect::placeholder`.
    async fn exec_params(&self, sql: &str, params: &[Value]) -> Result<RawResult, String>;
    /// Run every statement inside one transaction. On any failure the whole
    /// batch rolls back and an error naming the failing statement is returned.
    async fn exec_batch(&self, statements: &[String]) -> Result<u64, String>;
    /// Like `exec_batch` but each statement carries its own bound parameters.
    async fn exec_batch_params(&self, batch: &[(String, Vec<Value>)]) -> Result<u64, String>;
    async fn get_tables(&self) -> Result<Vec<TableInfo>, String>;
    async fn get_columns(&self, table: &str) -> Result<Vec<ColumnInfo>, String>;
    async fn get_indexes(&self, table: &str) -> Result<Vec<IndexInfo>, String>;
    async fn get_foreign_keys(&self, table: &str) -> Result<Vec<ForeignKeyInfo>, String>;
    async fn get_ddl(&self, table: &str) -> Result<String, String>;
    async fn get_version(&self) -> Result<String, String>;
    async fn get_databases(&self) -> Result<Vec<String>, String>;
}

/// Adapter factory. Unknown types are rejected.
pub fn create(config: &ConnectionConfig) -> Result<Arc<dyn Adapter>, String> {
    match config.kind.as_str() {
        "postgres" => Ok(Arc::new(super::postgres::PostgresAdapter::new(config))),
        "mysql" => Ok(Arc::new(super::mysql::MysqlAdapter::new(config))),
        "sqlite" => Ok(Arc::new(super::sqlite::SqliteAdapter::new(config))),
        other => Err(format!("Unsupported database type: {other}")),
    }
}

/// Open an inspection connection with writes disabled by the engine.
pub fn create_readonly(config: &ConnectionConfig) -> Result<Arc<dyn Adapter>, String> {
    match config.kind.as_str() {
        "postgres" => Ok(Arc::new(
            super::postgres::PostgresAdapter::new(config).readonly(),
        )),
        "mysql" => Ok(Arc::new(super::mysql::MysqlAdapter::new(config).readonly())),
        "sqlite" => Ok(Arc::new(
            super::sqlite::SqliteAdapter::new(config).readonly(),
        )),
        other => Err(format!("Unsupported database type: {other}")),
    }
}
