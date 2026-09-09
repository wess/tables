use tokio::sync::{mpsc, oneshot};

use crate::Host;

pub struct WriteApproval {
  pub connection: String,
  pub detail: String,
  pub answer: oneshot::Sender<bool>,
}

impl Host {
  pub fn approvals(&self) -> mpsc::Receiver<WriteApproval> {
    let (tx, rx) = mpsc::channel(4);
    *self.approval.lock().unwrap() = Some(tx);
    rx
  }

  pub(crate) async fn confirm_write(&self, detail: &str) -> Result<(), String> {
    let id = self.active_connection_id().ok_or("No active connection")?;
    let connection = self
      .find_connection(&id)
      .ok_or("Saved connection not found")?;
    if connection.safe_mode.as_deref() != Some("confirm") {
      return Ok(());
    }
    let tx = self
      .approval
      .lock()
      .unwrap()
      .clone()
      .ok_or("This connection requires write confirmation")?;
    let (answer, result) = oneshot::channel();
    tx.send(WriteApproval {
      connection: connection.name,
      detail: detail.chars().take(4000).collect(),
      answer,
    })
    .await
    .map_err(|_| "Write confirmation is unavailable")?;
    if result.await.unwrap_or(false) {
      Ok(())
    } else {
      Err("Write cancelled".into())
    }
  }
}
