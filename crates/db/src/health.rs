//! Connection health checks.
//!
//! One background tokio task per watched connection, probing `SELECT 1` every
//! 30 s. The status is "healthy" immediately on start; the first probe runs
//! only after one full interval.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::task::AbortHandle;

use super::Registry;
use model::Health;

const INTERVAL_MS: u64 = 30_000;
/// A single probe may not block status indefinitely.
const PROBE_TIMEOUT_MS: u64 = 5_000;

struct Watch {
    status: Arc<Mutex<Health>>,
    handle: AbortHandle,
}

/// Probe the adapter with a bounded timeout; a hung probe reports Degraded
/// rather than blocking forever.
async fn probe(adapter: &super::SharedAdapter) -> Health {
    let timeout = Duration::from_millis(PROBE_TIMEOUT_MS);
    match tokio::time::timeout(timeout, adapter.query("SELECT 1")).await {
        Ok(Ok(_)) => Health::Healthy,
        Ok(Err(_)) | Err(_) => Health::Degraded,
    }
}

#[derive(Default)]
pub struct HealthMonitor {
    watches: Mutex<HashMap<String, Watch>>,
}

impl HealthMonitor {
    /// Start probing `id`. Must be called from within the tokio runtime.
    pub fn start(&self, id: String, registry: Arc<Registry>) {
        self.stop(&id);
        // Unknown until the first probe completes — never claim healthy without
        // evidence.
        let status = Arc::new(Mutex::new(Health::Unknown));
        let task_status = status.clone();
        let task_id = id.clone();
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(INTERVAL_MS));
            loop {
                // The first tick fires immediately, so the first probe runs now.
                interval.tick().await;
                let next = match registry.adapter(&task_id) {
                    Err(_) => Health::Disconnected,
                    Ok(adapter) => probe(&adapter).await,
                };
                *task_status.lock().unwrap() = next;
            }
        })
        .abort_handle();
        self.watches
            .lock()
            .unwrap()
            .insert(id, Watch { status, handle });
    }

    /// Stops the probe task AND drops the status entry.
    pub fn stop(&self, id: &str) {
        if let Some(watch) = self.watches.lock().unwrap().remove(id) {
            watch.handle.abort();
        }
    }

    /// "disconnected" when never started / already stopped.
    pub fn status(&self, id: &str) -> Health {
        self.watches
            .lock()
            .unwrap()
            .get(id)
            .map(|watch| *watch.status.lock().unwrap())
            .unwrap_or(Health::Disconnected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_connection_is_disconnected() {
        let monitor = HealthMonitor::default();
        assert_eq!(monitor.status("nope"), Health::Disconnected);
    }

    #[tokio::test]
    async fn probes_immediately_and_reports_disconnected_without_adapter() {
        let monitor = HealthMonitor::default();
        let registry = Arc::new(Registry::default());
        monitor.start("c1".into(), registry);
        // The first probe runs immediately; with no live adapter it reports
        // Disconnected rather than an unearned Healthy.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(monitor.status("c1"), Health::Disconnected);
        monitor.stop("c1");
        assert_eq!(monitor.status("c1"), Health::Disconnected);
    }
}
