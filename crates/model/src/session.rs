//! A live server session/query, for the process manager.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// The backend/thread id used to kill the session (numeric as text).
    pub id: String,
    pub user: String,
    pub database: String,
    pub state: String,
    pub query: String,
    /// Human-readable age/running time.
    pub duration: String,
}
