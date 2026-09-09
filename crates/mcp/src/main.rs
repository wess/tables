use std::sync::Arc;

use futures::{SinkExt, StreamExt};
use rmcp::service::{RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::{RoleServer, ServiceExt};
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec, LinesCodecError};

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
  if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
    eprintln!("tablesmcp: read-only MCP server over stdio. Uses TABLES_DIR or ~/.tables.");
    return;
  }
  let server = Arc::new(tablesmcp::Server::default());
  let input = FramedRead::new(
    tokio::io::stdin(),
    LinesCodec::new_with_max_length(64 * 1024),
  )
  .take_while(|line| futures::future::ready(line.is_ok()))
  .filter_map(|line| async move {
    let line = line.ok()?;
    match serde_json::from_str::<RxJsonRpcMessage<RoleServer>>(&line) {
      Ok(message) => Some(message),
      Err(_) => {
        eprintln!("Invalid MCP message");
        None
      }
    }
  })
  .boxed();
  let output = FramedWrite::new(tokio::io::stdout(), LinesCodec::new()).with(
    |message: TxJsonRpcMessage<RoleServer>| {
      futures::future::ready(
        serde_json::to_string(&message)
          .map_err(|error| LinesCodecError::Io(std::io::Error::other(error))),
      )
    },
  );
  match server.clone().serve((output, input)).await {
    Ok(service) => {
      let _ = service.waiting().await;
    }
    Err(error) => eprintln!("MCP startup failed: {error}"),
  }
  server.close().await;
}
