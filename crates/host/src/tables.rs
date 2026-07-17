//! Table + row handlers: introspection, paging, and row CRUD.

use std::collections::HashMap;

use serde_json::Value;

use db::filters::build_filter_params;
use db::Dialect;
use model::{Row, RowWrite, RowsRequest, RowsResponse, TableInfo, TableStructure};
use store::connections;

use crate::facade::{row_i64, Host};

/// Whitelist the sort direction so it can never carry injected SQL.
fn sort_dir(dir: &str) -> &'static str {
    if dir.eq_ignore_ascii_case("desc") {
        "DESC"
    } else {
        "ASC"
    }
}

/// Builds a statement with placeholders, accumulating the values to bind so no
/// user value is ever interpolated. NULL is a keyword (not a bind target); a
/// JSON container keeps its dialect-escaped literal (Postgres coerces such a
/// literal to jsonb, which a bound text parameter would not). Scalars bind as a
/// placeholder cast to the column's type on Postgres (whose binds are strictly
/// typed). The review view's human-readable SQL is rendered separately (see
/// `app::workspace::review`).
struct Binder<'a> {
    dialect: Dialect,
    col_types: &'a HashMap<String, String>,
    params: Vec<Value>,
}

impl Binder<'_> {
    /// The SQL token for `col`'s value: `NULL`, a jsonb-safe literal, or a bound
    /// placeholder (cast to the column type on Postgres).
    fn token(&mut self, col: &str, v: &Value) -> String {
        match v {
            Value::Null => "NULL".to_string(),
            Value::Object(_) | Value::Array(_) => crate::values::literal(self.dialect, v),
            scalar => {
                self.params.push(scalar.clone());
                let ph = self.dialect.placeholder(self.params.len());
                match (self.dialect, self.col_types.get(col)) {
                    (Dialect::Postgres, Some(ty)) => format!("CAST({ph} AS {ty})"),
                    _ => ph,
                }
            }
        }
    }

    /// `col = <token>`, or `col IS NULL` — the WHERE fragments for update/delete.
    fn where_all(&mut self, key: &Row) -> String {
        key.iter()
            .map(|(k, v)| match v {
                Value::Null => format!("{} IS NULL", self.dialect.quote_ident(k)),
                other => format!("{} = {}", self.dialect.quote_ident(k), self.token(k, other)),
            })
            .collect::<Vec<_>>()
            .join(" AND ")
    }
}

/// The table a write targets.
fn write_table(write: &RowWrite) -> &str {
    match write {
        RowWrite::Insert { table, .. }
        | RowWrite::Update { table, .. }
        | RowWrite::Delete { table, .. } => table,
    }
}

/// Build one row write as an executable statement plus its bound parameters.
/// Shared by the single-row ops and the atomic reviewed batch.
fn write_stmt(
    dialect: Dialect,
    col_types: &HashMap<String, String>,
    write: &RowWrite,
) -> (String, Vec<Value>) {
    let mut b = Binder { dialect, col_types, params: Vec::new() };
    let sql = match write {
        RowWrite::Insert { table, row } => {
            let table = dialect.quote_ident(table);
            if row.is_empty() {
                format!("INSERT INTO {table} DEFAULT VALUES")
            } else {
                let cols = row.keys().map(|k| dialect.quote_ident(k)).collect::<Vec<_>>().join(", ");
                let vals = row.iter().map(|(k, v)| b.token(k, v)).collect::<Vec<_>>().join(", ");
                format!("INSERT INTO {table} ({cols}) VALUES ({vals})")
            }
        }
        RowWrite::Update { table, primary_key, changes } => {
            // SET tokens are bound before the WHERE tokens, matching the order
            // the placeholders appear in the statement.
            let set = changes
                .iter()
                .map(|(k, v)| format!("{} = {}", dialect.quote_ident(k), b.token(k, v)))
                .collect::<Vec<_>>()
                .join(", ");
            let where_clause = b.where_all(primary_key);
            format!("UPDATE {} SET {set} WHERE {where_clause}", dialect.quote_ident(table))
        }
        RowWrite::Delete { table, primary_key } => {
            let where_clause = b.where_all(primary_key);
            format!("DELETE FROM {} WHERE {where_clause}", dialect.quote_ident(table))
        }
    };
    (sql, b.params)
}

impl Host {
    /// Selecting a connection also makes it active.
    pub async fn list_tables(&self, connection_id: &str) -> Result<Vec<TableInfo>, String> {
        self.set_active(connection_id);
        let adapter = self.active_adapter()?;
        adapter.get_tables().await
    }

    /// A COUNT(*) for the total, then the page. Filter values are bound
    /// parameters; the sort column is a quoted identifier and the direction is
    /// whitelisted, so no user value is interpolated into the SQL text.
    pub async fn table_rows(&self, req: &RowsRequest) -> Result<RowsResponse, String> {
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();

        // Postgres binds are strictly typed, so a text-bound filter value must be
        // cast to the column's type; fetch the column types to build those casts
        // (empty for MySQL/SQLite, which coerce implicitly).
        let has_filters = req.filters.as_ref().is_some_and(|f| !f.is_empty());
        let col_types = if has_filters {
            self.col_types(&req.table).await
        } else {
            HashMap::new()
        };

        let (where_clause, params) = match &req.filters {
            Some(filters) if !filters.is_empty() => {
                let logic = req.filter_logic.as_deref().unwrap_or("and");
                build_filter_params(dialect, filters, logic, &col_types)
            }
            _ => (String::new(), Vec::new()),
        };
        let order_by = match &req.sort {
            Some(sort) => {
                format!("ORDER BY {} {}", dialect.quote_ident(&sort.column), sort_dir(&sort.direction))
            }
            None => String::new(),
        };
        // Pages are 1-based; guard page 0 so the offset can never underflow.
        let offset = req.page.saturating_sub(1) * req.page_size;
        let table = dialect.quote_ident(&req.table);

        let count = adapter
            .exec_params(&format!("SELECT COUNT(*) as total FROM {table} {where_clause}"), &params)
            .await?;
        let total = count.rows.first().map(|row| row_i64(row, "total")).unwrap_or(0);

        let result = adapter
            .exec_params(
                &format!(
                    "SELECT * FROM {table} {where_clause} {order_by} LIMIT {} OFFSET {}",
                    req.page_size, offset
                ),
                &params,
            )
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
        let dialect = adapter.dialect();
        let col_types = self.col_types(table).await;
        let (sql, params) = write_stmt(
            dialect,
            &col_types,
            &RowWrite::Insert { table: table.to_string(), row: row.clone() },
        );
        adapter.exec_params(&sql, &params).await?;
        Ok(true)
    }

    pub async fn row_update(
        &self,
        table: &str,
        primary_key: &Row,
        changes: &Row,
    ) -> Result<bool, String> {
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();
        let col_types = self.col_types(table).await;
        let (sql, params) = write_stmt(
            dialect,
            &col_types,
            &RowWrite::Update {
                table: table.to_string(),
                primary_key: primary_key.clone(),
                changes: changes.clone(),
            },
        );
        adapter.exec_params(&sql, &params).await?;
        Ok(true)
    }

    pub async fn row_delete(&self, table: &str, primary_key: &Row) -> Result<bool, String> {
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();
        let col_types = self.col_types(table).await;
        let (sql, params) = write_stmt(
            dialect,
            &col_types,
            &RowWrite::Delete { table: table.to_string(), primary_key: primary_key.clone() },
        );
        adapter.exec_params(&sql, &params).await?;
        Ok(true)
    }

    /// Apply a reviewed batch of row writes atomically: all succeed or the whole
    /// batch rolls back. Column types are fetched once per distinct table.
    pub async fn apply_row_writes(&self, writes: &[RowWrite]) -> Result<u64, String> {
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();
        let mut cache: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut batch: Vec<(String, Vec<Value>)> = Vec::with_capacity(writes.len());
        for write in writes {
            let table = write_table(write);
            if !cache.contains_key(table) {
                cache.insert(table.to_string(), self.col_types(table).await);
            }
            batch.push(write_stmt(dialect, &cache[table], write));
        }
        adapter.exec_batch_params(&batch).await
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
        self.invalidate_schema_cache();
        let mut config = conn.config();
        config.database = database.to_string();
        self.registry.connect(&config).await?;
        self.set_active(connection_id);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map};

    fn row(pairs: &[(&str, Value)]) -> Row {
        let mut m = Map::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), v.clone());
        }
        m
    }

    #[test]
    fn insert_binds_scalars_and_keeps_null_as_keyword() {
        let write = RowWrite::Insert {
            table: "t".into(),
            row: row(&[("id", json!(1)), ("name", json!("Bob")), ("note", Value::Null)]),
        };
        let (sql, params) = write_stmt(Dialect::Sqlite, &HashMap::new(), &write);
        assert_eq!(sql, r#"INSERT INTO "t" ("id", "name", "note") VALUES (?, ?, NULL)"#);
        assert_eq!(params, vec![json!(1), json!("Bob")]);
    }

    #[test]
    fn insert_casts_each_value_on_postgres() {
        let types = [("id", "integer"), ("name", "text")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let write = RowWrite::Insert {
            table: "t".into(),
            row: row(&[("id", json!(5)), ("name", json!("x"))]),
        };
        let (sql, params) = write_stmt(Dialect::Postgres, &types, &write);
        assert_eq!(
            sql,
            r#"INSERT INTO "t" ("id", "name") VALUES (CAST($1 AS integer), CAST($2 AS text))"#
        );
        assert_eq!(params, vec![json!(5), json!("x")]);
    }

    #[test]
    fn update_binds_set_before_where() {
        let write = RowWrite::Update {
            table: "t".into(),
            primary_key: row(&[("id", json!(1))]),
            changes: row(&[("age", json!(31))]),
        };
        let (sql, params) = write_stmt(Dialect::Sqlite, &HashMap::new(), &write);
        assert_eq!(sql, r#"UPDATE "t" SET "age" = ? WHERE "id" = ?"#);
        assert_eq!(params, vec![json!(31), json!(1)]);
    }

    #[test]
    fn delete_where_null_is_a_keyword() {
        let write = RowWrite::Delete {
            table: "t".into(),
            primary_key: row(&[("id", Value::Null)]),
        };
        let (sql, params) = write_stmt(Dialect::Postgres, &HashMap::new(), &write);
        assert_eq!(sql, r#"DELETE FROM "t" WHERE "id" IS NULL"#);
        assert!(params.is_empty());
    }

    #[test]
    fn crafted_value_is_bound_verbatim_not_interpolated() {
        let write = RowWrite::Insert {
            table: "t".into(),
            row: row(&[("name", json!("x'); DROP TABLE t;--"))]),
        };
        let (sql, params) = write_stmt(Dialect::Sqlite, &HashMap::new(), &write);
        assert_eq!(sql, r#"INSERT INTO "t" ("name") VALUES (?)"#);
        assert_eq!(params, vec![json!("x'); DROP TABLE t;--")]);
    }

    #[test]
    fn json_containers_stay_literal_for_jsonb_safety() {
        let write = RowWrite::Insert {
            table: "t".into(),
            row: row(&[("payload", json!({"a": 1}))]),
        };
        let (sql, params) = write_stmt(Dialect::Postgres, &HashMap::new(), &write);
        assert_eq!(sql, r#"INSERT INTO "t" ("payload") VALUES ('{"a":1}')"#);
        assert!(params.is_empty());
    }
}
