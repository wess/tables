use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

struct Client {
  child: Child,
  input: ChildStdin,
  output: Lines<BufReader<ChildStdout>>,
}

impl Client {
  async fn send(&mut self, value: Value) {
    let line = format!("{value}\n");
    self.input.write_all(line.as_bytes()).await.unwrap();
    self.input.flush().await.unwrap();
  }

  async fn request(&mut self, method: &str, params: Value) -> Value {
    self
      .send(json!({"jsonrpc":"2.0", "id":1, "method":method, "params":params}))
      .await;
    let line = tokio::time::timeout(Duration::from_secs(10), self.output.next_line())
      .await
      .expect("response deadline")
      .unwrap()
      .expect("response");
    serde_json::from_str(&line).unwrap()
  }

  async fn call(&mut self, name: &str, arguments: Value) -> Value {
    self
      .request("tools/call", json!({"name":name, "arguments":arguments}))
      .await
  }
}

#[tokio::test]
async fn saved_sqlite_connection_over_real_stdio() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("test.db");
  let conn: model::StoredConnection = serde_json::from_value(json!({
    "id":"fixture", "name":"Test database", "type":"sqlite", "filepath":path,
    "password":"never-expose-this", "startupCommands":"DROP TABLE items"
  }))
  .unwrap();
  let adapter = db::create(&conn.config()).unwrap();
  adapter.connect().await.unwrap();
  adapter
    .query("CREATE TABLE items (id INTEGER PRIMARY KEY, label TEXT)")
    .await
    .unwrap();
  adapter
    .query("INSERT INTO items VALUES (1, 'first'), (2, 'second'), (3, 'third')")
    .await
    .unwrap();
  adapter.disconnect().await;
  std::fs::write(
    dir.path().join("connections.json"),
    serde_json::to_vec(&vec![conn]).unwrap(),
  )
  .unwrap();

  let mut child = Command::new(env!("CARGO_BIN_EXE_tablesmcp"))
    .env("TABLES_DIR", dir.path())
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .kill_on_drop(true)
    .spawn()
    .unwrap();
  let mut client = Client {
    input: child.stdin.take().unwrap(),
    output: BufReader::new(child.stdout.take().unwrap()).lines(),
    child,
  };
  let init = client
    .request(
      "initialize",
      json!({"protocolVersion":"2025-06-18",
    "capabilities":{}, "clientInfo":{"name":"test", "version":"1"}}),
    )
    .await;
  assert_eq!(init["result"]["serverInfo"]["name"], "tables");
  client
    .send(json!({"jsonrpc":"2.0", "method":"notifications/initialized"}))
    .await;
  let tools = client.request("tools/list", json!({})).await;
  assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 4);
  let saved = client.call("list_connections", json!({})).await;
  assert!(!saved.to_string().contains("never-expose-this"));
  assert!(!saved.to_string().contains("filepath"));
  assert_eq!(
    saved["result"]["structuredContent"]["connections"][0]["id"],
    "fixture"
  );
  let tables = client
    .call("list_tables", json!({"connection":"fixture"}))
    .await;
  assert_eq!(
    tables["result"]["structuredContent"]["tables"][0]["name"],
    "items"
  );
  let schema = client
    .call(
      "table_schema",
      json!({"connection":"fixture", "table":"items"}),
    )
    .await;
  assert_eq!(
    schema["result"]["structuredContent"]["columns"]
      .as_array()
      .unwrap()
      .len(),
    2
  );
  let rows = client
    .call(
      "table_rows",
      json!({"connection":"fixture", "table":"items", "limit":2}),
    )
    .await;
  let page = &rows["result"]["structuredContent"];
  assert_eq!(page["rows"].as_array().unwrap().len(), 2);
  assert_eq!(page["nextOffset"], 2);
  let last = client
    .call(
      "table_rows",
      json!({"connection":"fixture", "table":"items", "limit":2, "offset":2}),
    )
    .await;
  assert_eq!(last["result"]["structuredContent"]["rows"][0]["id"], 3);
  assert!(last["result"]["structuredContent"]["nextOffset"].is_null());
  for arguments in [
    json!({"connection":"fixture", "table":"items", "limit":201}),
    json!({"connection":"fixture", "table":"items", "offset":-1}),
    json!({"connection":"fixture", "table":"items", "sql":"DROP TABLE items"}),
    json!({"connection":"fixture", "table":"items; DROP TABLE items"}),
    json!({"connection":"missing", "table":"items"}),
  ] {
    assert_eq!(
      client.call("table_rows", arguments).await["result"]["isError"],
      true
    );
  }
  assert!(client
    .call("execute_query", json!({"sql":"DROP TABLE items"}))
    .await
    .get("error")
    .is_some());
  assert!(client
    .request("ping", json!({}))
    .await
    .get("result")
    .is_some());
  drop(client.input);
  let status = tokio::time::timeout(Duration::from_secs(10), client.child.wait())
    .await
    .unwrap()
    .unwrap();
  assert!(status.success());
}
