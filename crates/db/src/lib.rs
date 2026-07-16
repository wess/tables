//! Async multi-engine database client for Tables, over sqlx.

pub mod dialect;
pub mod filters;

mod engine;
mod health;
mod mysql;
mod postgres;
mod registry;
mod sqlite;
mod tunnel;

pub use dialect::Dialect;
pub use engine::{create, Adapter};
pub use health::HealthMonitor;
pub use registry::{Registry, SharedAdapter};
