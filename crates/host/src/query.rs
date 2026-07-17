//! Query, history, favorites, and editor-tab handlers.

use std::time::Instant;

use db::filters::split_statements;
use db::Dialect;
use model::{iso_now, new_uuid, Favorite, HistoryEntry, QueryResult, SavedTab};
use store::{favorites, history, tabs};

use crate::facade::Host;

fn elapsed_ms(start: Instant) -> u64 {
    (start.elapsed().as_secs_f64() * 1000.0).round() as u64
}

impl Host {
    /// Run one statement, record it in history (with the error on failure), and
    /// return the result. A failed statement is an `Ok` `QueryResult` carrying
    /// `error`; only a missing connection is an `Err`.
    pub async fn execute_query(&self, sql: &str) -> Result<QueryResult, String> {
        let id = self
            .active_connection_id()
            .ok_or_else(|| "No active connection".to_string())?;
        let adapter = self.registry.adapter(&id)?;

        let start = Instant::now();
        let outcome = adapter.query(sql).await;
        let ms = elapsed_ms(start);
        // The statement may have been DDL — drop cached column types.
        self.invalidate_schema_cache();

        match outcome {
            Ok(raw) => {
                let result = QueryResult::from_raw(raw, ms);
                self.record_history(sql, &id, ms, result.rows_affected, None);
                Ok(result)
            }
            Err(error) => {
                self.record_history(sql, &id, ms, 0, Some(error.clone()));
                Ok(QueryResult::failure(error, ms))
            }
        }
    }

    /// Split on statement boundaries, run in order, stopping at the first
    /// failure. Each result carries its own `sql`.
    pub async fn execute_multi(&self, sql: &str) -> Result<Vec<QueryResult>, String> {
        let id = self
            .active_connection_id()
            .ok_or_else(|| "No active connection".to_string())?;
        let adapter = self.registry.adapter(&id)?;

        let mut results = Vec::new();
        for stmt in split_statements(sql) {
            let start = Instant::now();
            let outcome = adapter.query(&stmt).await;
            let ms = elapsed_ms(start);
            match outcome {
                Ok(raw) => {
                    let mut result = QueryResult::from_raw(raw, ms);
                    result.sql = Some(stmt.clone());
                    self.record_history(&stmt, &id, ms, result.rows_affected, None);
                    results.push(result);
                }
                Err(error) => {
                    let mut result = QueryResult::failure(error.clone(), ms);
                    result.sql = Some(stmt.clone());
                    self.record_history(&stmt, &id, ms, 0, Some(error));
                    results.push(result);
                    break;
                }
            }
        }
        // Any statement in the batch may have been DDL — drop cached column types.
        self.invalidate_schema_cache();
        Ok(results)
    }

    /// Run the query planner for `sql` and return the plan as a result set.
    /// SQLite uses `EXPLAIN QUERY PLAN`; Postgres/MySQL use plain `EXPLAIN`
    /// (never `ANALYZE`, which would execute the statement).
    pub async fn explain(&self, sql: &str) -> Result<QueryResult, String> {
        let adapter = self.active_adapter()?;
        let prefix = match adapter.dialect() {
            Dialect::Sqlite => "EXPLAIN QUERY PLAN ",
            _ => "EXPLAIN ",
        };
        let start = Instant::now();
        let raw = adapter.query(&format!("{prefix}{}", sql.trim().trim_end_matches(';'))).await?;
        Ok(QueryResult::from_raw(raw, elapsed_ms(start)))
    }

    fn record_history(
        &self,
        sql: &str,
        connection_id: &str,
        ms: u64,
        rows_affected: u64,
        error: Option<String>,
    ) {
        let _ = history::append(HistoryEntry {
            id: new_uuid(),
            sql: sql.to_string(),
            connection_id: connection_id.to_string(),
            executed_at: iso_now(),
            execution_time: ms,
            rows_affected,
            error,
        });
    }

    pub fn query_history(&self) -> Result<Vec<HistoryEntry>, String> {
        history::load()
    }

    pub fn list_favorites(&self) -> Result<Vec<Favorite>, String> {
        favorites::load()
    }

    pub fn save_favorite(
        &self,
        id: Option<String>,
        name: &str,
        sql: &str,
    ) -> Result<Favorite, String> {
        favorites::save(id, name, sql, None)
    }

    pub fn delete_favorite(&self, id: &str) -> Result<bool, String> {
        favorites::remove(id)
    }

    pub fn load_tabs(&self) -> Vec<SavedTab> {
        tabs::load()
    }

    pub fn save_tabs(&self, tabs: &[SavedTab]) -> bool {
        tabs::save_all(tabs)
    }
}
