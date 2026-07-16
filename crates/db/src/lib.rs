//! Async multi-engine database client for Tables, over sqlx.

/// Bind a slice of JSON values onto a sqlx query, each as its natural type
/// (bool, i64, f64, or text). Shared by every adapter's parameterized execution
/// path so user values are sent as bound parameters, never interpolated into
/// SQL text. Defined before the adapter modules so they can use it.
macro_rules! bind_params {
    ($query:expr, $params:expr) => {{
        let mut q = $query;
        for p in $params {
            q = match p {
                serde_json::Value::Bool(b) => q.bind(*b),
                serde_json::Value::Number(n) => {
                    if let Some(i) = n.as_i64() {
                        q.bind(i)
                    } else {
                        q.bind(n.as_f64().unwrap_or(0.0))
                    }
                }
                serde_json::Value::String(s) => q.bind(s.clone()),
                serde_json::Value::Null => q.bind(None::<String>),
                other => q.bind(other.to_string()),
            };
        }
        q
    }};
}

pub mod dialect;
pub mod filters;

mod engine;
mod health;
mod mysql;
mod postgres;
mod registry;
mod sqlite;
mod tunnel;

#[cfg(test)]
mod roundtrip;

pub use dialect::Dialect;
pub use engine::{create, Adapter};
pub use health::HealthMonitor;
pub use registry::{Registry, SharedAdapter};
