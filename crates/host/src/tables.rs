//! Table + row handlers: introspection, paging, and row CRUD.

use db::filters::build_filter_clause;
use db::Dialect;
use model::{Row, RowWrite, RowsRequest, RowsResponse, TableInfo, TableStructure};
use store::connections;

use crate::facade::{row_i64, Host};
use crate::values;

/// Whitelist the sort direction so it can never carry injected SQL.
fn sort_dir(dir: &str) -> &'static str {
    if dir.eq_ignore_ascii_case("desc") {
        "DESC"
    } else {
        "ASC"
    }
}

/// Render one row write to an executable statement (identifiers and values
/// escaped through the engine's dialect). Shared by the single-row ops and the
/// atomic reviewed batch so both build identical SQL.
fn write_sql(dialect: Dialect, write: &RowWrite) -> String {
    match write {
        RowWrite::Insert { table, row } => {
            let table = dialect.quote_ident(table);
            if row.is_empty() {
                return format!("INSERT INTO {table} DEFAULT VALUES");
            }
            let cols = row.keys().map(|k| dialect.quote_ident(k)).collect::<Vec<_>>().join(", ");
            let vals =
                row.values().map(|v| values::literal(dialect, v)).collect::<Vec<_>>().join(", ");
            format!("INSERT INTO {table} ({cols}) VALUES ({vals})")
        }
        RowWrite::Update { table, primary_key, changes } => {
            let set = changes
                .iter()
                .map(|(k, v)| format!("{} = {}", dialect.quote_ident(k), values::literal(dialect, v)))
                .collect::<Vec<_>>()
                .join(", ");
            let where_clause = where_all(dialect, primary_key);
            format!("UPDATE {} SET {set} WHERE {where_clause}", dialect.quote_ident(table))
        }
        RowWrite::Delete { table, primary_key } => {
            let where_clause = where_all(dialect, primary_key);
            format!("DELETE FROM {} WHERE {where_clause}", dialect.quote_ident(table))
        }
    }
}

fn where_all(dialect: Dialect, key: &Row) -> String {
    key.iter()
        .map(|(k, v)| values::where_eq(dialect, k, v))
        .collect::<Vec<_>>()
        .join(" AND ")
}

impl Host {
    /// Selecting a connection also makes it active.
    pub async fn list_tables(&self, connection_id: &str) -> Result<Vec<TableInfo>, String> {
        self.set_active(connection_id);
        let adapter = self.active_adapter()?;
        adapter.get_tables().await
    }

    /// A COUNT(*) for the total, then the page. Filter values are always
    /// string-quoted and the sort direction is interpolated raw.
    pub async fn table_rows(&self, req: &RowsRequest) -> Result<RowsResponse, String> {
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();

        let where_clause = match &req.filters {
            Some(filters) if !filters.is_empty() => {
                let logic = req.filter_logic.as_deref().unwrap_or("and");
                build_filter_clause(dialect, filters, logic)
            }
            _ => String::new(),
        };
        let order_by = match &req.sort {
            Some(sort) => {
                format!("ORDER BY {} {}", dialect.quote_ident(&sort.column), sort_dir(&sort.direction))
            }
            None => String::new(),
        };
        let offset = (req.page - 1) * req.page_size;
        let table = dialect.quote_ident(&req.table);

        let count = adapter
            .query(&format!("SELECT COUNT(*) as total FROM {table} {where_clause}"))
            .await?;
        let total = count.rows.first().map(|row| row_i64(row, "total")).unwrap_or(0);

        let result = adapter
            .query(&format!(
                "SELECT * FROM {table} {where_clause} {order_by} LIMIT {} OFFSET {}",
                req.page_size, offset
            ))
            .await?;

        Ok(RowsResponse {
            rows: result.rows,
            columns: result.columns,
            column_types: result.column_types,
            total,
            page: req.page,
            page_size: req.page_size,
        })
    }

    pub async fn table_structure(&self, table: &str) -> Result<TableStructure, String> {
        let adapter = self.active_adapter()?;
        Ok(TableStructure {
            columns: adapter.get_columns(table).await?,
            indexes: adapter.get_indexes(table).await?,
            foreign_keys: adapter.get_foreign_keys(table).await?,
        })
    }

    pub async fn table_ddl(&self, table: &str) -> Result<String, String> {
        let adapter = self.active_adapter()?;
        adapter.get_ddl(table).await
    }

    /// An empty row inserts DEFAULT VALUES.
    pub async fn row_insert(&self, table: &str, row: &Row) -> Result<bool, String> {
        let adapter = self.active_adapter()?;
        let sql = write_sql(
            adapter.dialect(),
            &RowWrite::Insert { table: table.to_string(), row: row.clone() },
        );
        adapter.query(&sql).await?;
        Ok(true)
    }

    pub async fn row_update(
        &self,
        table: &str,
        primary_key: &Row,
        changes: &Row,
    ) -> Result<bool, String> {
        let adapter = self.active_adapter()?;
        let sql = write_sql(
            adapter.dialect(),
            &RowWrite::Update {
                table: table.to_string(),
                primary_key: primary_key.clone(),
                changes: changes.clone(),
            },
        );
        adapter.query(&sql).await?;
        Ok(true)
    }

    pub async fn row_delete(&self, table: &str, primary_key: &Row) -> Result<bool, String> {
        let adapter = self.active_adapter()?;
        let sql = write_sql(
            adapter.dialect(),
            &RowWrite::Delete { table: table.to_string(), primary_key: primary_key.clone() },
        );
        adapter.query(&sql).await?;
        Ok(true)
    }

    /// Apply a reviewed batch of row writes atomically: all succeed or the whole
    /// batch rolls back.
    pub async fn apply_row_writes(&self, writes: &[RowWrite]) -> Result<u64, String> {
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();
        let statements: Vec<String> = writes.iter().map(|w| write_sql(dialect, w)).collect();
        adapter.exec_batch(&statements).await
    }

    /// Reads the named connection directly, not the active.
    pub async fn list_databases(&self, connection_id: &str) -> Result<Vec<String>, String> {
        let adapter = self.registry.adapter(connection_id)?;
        adapter.get_databases().await
    }

    /// Reconnect the same connection to another database and make it active.
    pub async fn switch_database(&self, connection_id: &str, database: &str) -> Result<bool, String> {
        let conn = connections::find(connection_id)
            .ok_or_else(|| format!("Connection not found: {connection_id}"))?;
        self.registry.disconnect(connection_id).await;
        let mut config = conn.config();
        config.database = database.to_string();
        self.registry.connect(&config).await?;
        self.set_active(connection_id);
        Ok(true)
    }
}
