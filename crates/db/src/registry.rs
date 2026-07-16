//! The connection registry: one live adapter per connection id.
//!
//! Opens/closes the SSH tunnel around connect/disconnect and runs startup
//! commands after connecting. Adapters are shared as `Arc<dyn Adapter>`; the
//! adapters map is a plain mutex (locked only for sync map ops), while tunnels
//! sit behind an async mutex since opening one awaits.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::Mutex as AsyncMutex;

use super::engine::{self, Adapter};
use super::tunnel::Tunnels;
use model::ConnectionConfig;

/// A live adapter, shareable across tasks.
pub type SharedAdapter = Arc<dyn Adapter>;

#[derive(Default)]
pub struct Registry {
    adapters: Mutex<HashMap<String, SharedAdapter>>,
    tunnels: AsyncMutex<Tunnels>,
}

impl Registry {
    /// Connect a configured connection. No-op when already connected. SSH tunnel
    /// first (non-sqlite, when enabled), then the adapter, then startup commands
    /// — each command's failure silently ignored.
    pub async fn connect(&self, config: &ConnectionConfig) -> Result<(), String> {
        if self.adapters.lock().unwrap().contains_key(&config.id) {
            return Ok(());
        }
        let mut config = config.clone();
        let wants_tunnel =
            config.kind != "sqlite" && config.ssh.as_ref().is_some_and(|ssh| ssh.enabled);
        if wants_tunnel {
            let ssh = config.ssh.clone().unwrap();
            let local_port = self
                .tunnels
                .lock()
                .await
                .open(&config.id, &ssh, &config.host, config.port)
                .await?;
            config.host = "127.0.0.1".into();
            config.port = local_port;
        }

        // From here on the tunnel is open; any failure before the adapter is
        // registered must close it so no SSH child is leaked. Startup-command
        // failures are surfaced (not silent) and fail the connection.
        let setup = async {
            let adapter = engine::create(&config)?;
            adapter.connect().await?;
            if let Some(commands) = &config.startup_commands {
                for command in commands.lines().map(str::trim).filter(|c| !c.is_empty()) {
                    adapter
                        .query(command)
                        .await
                        .map_err(|e| format!("Startup command failed ({command}): {e}"))?;
                }
            }
            Ok::<_, String>(adapter)
        }
        .await;
        let adapter = match setup {
            Ok(adapter) => adapter,
            Err(error) => {
                if wants_tunnel {
                    self.tunnels.lock().await.close(&config.id).await;
                }
                return Err(error);
            }
        };

        self.adapters
            .lock()
            .unwrap()
            .insert(config.id.clone(), adapter.clone());
        Ok(())
    }

    /// Disconnect and drop the adapter, then close the tunnel — always, even
    /// when no adapter was live.
    pub async fn disconnect(&self, id: &str) {
        let adapter = self.adapters.lock().unwrap().remove(id);
        if let Some(adapter) = adapter {
            adapter.disconnect().await;
        }
        self.tunnels.lock().await.close(id).await;
    }

    /// The adapter for a connected id, or an error.
    pub fn adapter(&self, id: &str) -> Result<SharedAdapter, String> {
        self.adapters
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| format!("No active connection: {id}"))
    }

    pub fn is_connected(&self, id: &str) -> bool {
        self.adapters.lock().unwrap().contains_key(id)
    }
}
