//! SSH tunnels — spawns the system `ssh` binary.
//!
//! Host keys are verified by default (`accept-new`: known hosts are checked,
//! an unknown host is trusted on first use, and a changed key is rejected).
//! Password auth is NOT wired through (no sshpass/askpass): with authMethod
//! "password" the tunnel only works if an agent or default identity satisfies
//! auth.

use std::collections::HashMap;
use std::net::TcpListener;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::time::{sleep, Instant};

use model::SshConfig;

/// How long to wait for the local forward to start accepting connections.
const READY_TIMEOUT_MS: u64 = 5000;
/// Cap on captured stderr so a chatty ssh cannot balloon an error message.
const STDERR_CAP: usize = 4096;

#[derive(Default)]
pub struct Tunnels {
    children: HashMap<String, Child>,
}

impl Tunnels {
    /// Open a tunnel for a connection id, closing any existing one first.
    /// Returns the local port the adapter should connect to.
    pub async fn open(
        &mut self,
        id: &str,
        ssh: &SshConfig,
        remote_host: &str,
        remote_port: u16,
    ) -> Result<u16, String> {
        self.close(id).await;
        // Reserve a free local port from the OS, then release it just before
        // ssh binds it — far less collision-prone than an unchecked random port.
        let local_port = reserve_port()?;

        let mut cmd = Command::new("ssh");
        cmd.arg("-N")
            .arg("-L")
            .arg(format!("{local_port}:{remote_host}:{remote_port}"))
            .arg("-p")
            .arg(ssh.port.to_string())
            .arg("-o")
            .arg("StrictHostKeyChecking=accept-new")
            .arg("-o")
            .arg("ExitOnForwardFailure=yes")
            .arg("-o")
            .arg("BatchMode=yes");
        if ssh.auth_method == "key" {
            if let Some(key) = ssh.key_path.as_deref().filter(|k| !k.is_empty()) {
                cmd.arg("-i").arg(key);
            }
        }
        cmd.arg(format!("{}@{}", ssh.username, ssh.host));
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| format!("Failed to start ssh: {e}"))?;

        // Probe the local forward until it accepts a connection. An early exit or
        // a timeout fails with the ssh diagnostic output rather than a bare code.
        let deadline = Instant::now() + Duration::from_millis(READY_TIMEOUT_MS);
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                let stderr = read_stderr(&mut child).await;
                return Err(format!(
                    "SSH tunnel exited ({}): {}",
                    status.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".into()),
                    stderr.trim()
                ));
            }
            if tokio::net::TcpStream::connect(("127.0.0.1", local_port)).await.is_ok() {
                self.children.insert(id.to_string(), child);
                return Ok(local_port);
            }
            if Instant::now() >= deadline {
                let _ = child.kill().await;
                let stderr = read_stderr(&mut child).await;
                return Err(format!(
                    "SSH tunnel did not become ready in {}ms: {}",
                    READY_TIMEOUT_MS,
                    stderr.trim()
                ));
            }
            sleep(Duration::from_millis(50)).await;
        }
    }

    /// Kill the tunnel process, if any. Called on every disconnect.
    pub async fn close(&mut self, id: &str) {
        if let Some(mut child) = self.children.remove(id) {
            let _ = child.kill().await;
        }
    }
}

/// Ask the OS for an unused loopback port, then release it for ssh to bind.
fn reserve_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    drop(listener);
    Ok(port)
}

/// Read the child's captured stderr (bounded), best-effort.
async fn read_stderr(child: &mut Child) -> String {
    let Some(mut stderr) = child.stderr.take() else {
        return String::new();
    };
    let mut buf = Vec::new();
    let _ = stderr.read_to_end(&mut buf).await;
    buf.truncate(STDERR_CAP);
    String::from_utf8_lossy(&buf).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_port_is_in_range() {
        let port = reserve_port().unwrap();
        assert!(port >= 1024, "OS should hand back a usable port");
    }

    #[tokio::test]
    async fn close_without_tunnel_is_noop() {
        let mut tunnels = Tunnels::default();
        tunnels.close("nope").await;
    }
}
