//! The engine-agnostic adapter contract.
//!
//! Adapters are async and hold their own sqlx pool/connection; the app drives
//! them on a tokio runtime. Errors are plain strings (the driver's message).

use std::sync::Arc;

use async_trait::async_trait;
use model::{ColumnInfo, ConnectionConfig, ForeignKeyInfo, IndexInfo, RawResult, TableInfo};

use crate::dialect::Dialect;

#[async_trait]
pub trait Adapter: Send + Sync {
    /// The SQL text dialect (identifier/string quoting) for this engine.
    fn dialect(&self) -> Dialect;
    async fn connect(&self) -> Result<(), String>;
    async fn disconnect(&self);
    async fn query(&self, sql: &str) -> Result<RawResult, String>;
    /// Run every statement inside one transaction. On any failure the whole
    /// batch rolls back and an error naming the failing statement is returned.
    async fn exec_batch(&self, statements: &[String]) -> Result<u64, String>;
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
