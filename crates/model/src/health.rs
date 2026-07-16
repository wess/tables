//! Connection health status.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    /// No probe has completed yet.
    Unknown,
    Healthy,
    Degraded,
    Disconnected,
}
