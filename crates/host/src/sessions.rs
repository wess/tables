//! The session/process manager: list live server sessions and kill one.
//! Postgres reads `pg_stat_activity`; MySQL reads `SHOW FULL PROCESSLIST`.
//! SQLite has no server, so its lists are empty and kills are rejected.

use serde_json::Value;

use db::Dialect;
use model::{Row, Session};

use crate::facade::Host;

/// A row cell as a trimmed string (`NULL`/missing → empty).
fn cell(row: &Row, key: &str) -> String {
    match row.get(key) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
    }
}

impl Host {
    pub async fn sessions(&self) -> Result<Vec<Session>, String> {
        let adapter = self.active_adapter()?;
        match adapter.dialect() {
            Dialect::Postgres => {
                let raw = adapter
                    .query(
                        "SELECT pid, usename, datname, state, query, \
                         COALESCE(EXTRACT(EPOCH FROM (now() - query_start))::bigint, 0) AS secs \
                         FROM pg_stat_activity \
                         WHERE pid <> pg_backend_pid() AND state IS NOT NULL \
                         ORDER BY query_start",
                    )
                    .await?;
                Ok(raw
                    .rows
                    .iter()
                    .map(|r| Session {
                        id: cell(r, "pid"),
                        user: cell(r, "usename"),
                        database: cell(r, "datname"),
                        state: cell(r, "state"),
                        query: cell(r, "query"),
                        duration: format!("{}s", cell(r, "secs")),
                    })
                    .collect())
            }
            Dialect::Mysql => {
                let raw = adapter.query("SHOW FULL PROCESSLIST").await?;
                Ok(raw
                    .rows
                    .iter()
                    .map(|r| Session {
                        id: cell(r, "Id"),
                        user: cell(r, "User"),
                        database: cell(r, "db"),
                        state: {
                            let cmd = cell(r, "Command");
                            let st = cell(r, "State");
                            if st.is_empty() { cmd } else { format!("{cmd} · {st}") }
                        },
                        query: cell(r, "Info"),
                        duration: format!("{}s", cell(r, "Time")),
                    })
                    .collect())
            }
            Dialect::Sqlite => Ok(Vec::new()),
        }
    }

    /// Kill a session by its numeric id. The id is parsed as an integer first so
    /// it can never carry injected SQL.
    pub async fn kill_session(&self, id: &str) -> Result<(), String> {
        let pid: i64 = id.trim().parse().map_err(|_| "Invalid session id".to_string())?;
        let adapter = self.active_adapter()?;
        match adapter.dialect() {
            Dialect::Postgres => {
                adapter.query(&format!("SELECT pg_terminate_backend({pid})")).await?;
            }
            Dialect::Mysql => {
                adapter.query(&format!("KILL {pid}")).await?;
            }
            Dialect::Sqlite => return Err("SQLite has no server sessions".into()),
        }
        Ok(())
    }
}
