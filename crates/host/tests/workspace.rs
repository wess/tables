use serde_json::{json, Map};

#[tokio::test]
async fn workspace_cursors_exports_and_readonly_connections() {
  let dir = tempfile::tempdir().unwrap();
  std::env::set_var("TABLES_DIR", dir.path());
  let host = host::Host::new();
  for id in ["first", "second"] {
    let conn: model::StoredConnection = serde_json::from_value(json!({
      "id":id, "type":"sqlite", "filepath":dir.path().join(format!("{id}.db"))
    }))
    .unwrap();
    host.save_connection(&conn).unwrap();
    host.connect(id).await.unwrap();
    host
      .execute_query("CREATE TABLE t (id INTEGER PRIMARY KEY, value TEXT)")
      .await
      .unwrap();
  }
  let first = host.scoped("first");
  let second = host.scoped("second");
  first
    .execute_query("INSERT INTO t VALUES (1, 'first')")
    .await
    .unwrap();
  second
    .execute_query("INSERT INTO t VALUES (2, 'second')")
    .await
    .unwrap();
  assert_eq!(
    first
      .execute_query("SELECT value FROM t")
      .await
      .unwrap()
      .rows[0]["value"],
    "first"
  );
  assert_eq!(
    second
      .execute_query("SELECT value FROM t")
      .await
      .unwrap()
      .rows[0]["value"],
    "second"
  );
  first.execute_query("WITH RECURSIVE n(x) AS (VALUES(2) UNION ALL SELECT x+1 FROM n WHERE x<10000) INSERT INTO t SELECT x, 'comma,quote\"cr'||char(13)||'lf'||char(10) FROM n").await.unwrap();
  let output = dir.path().join("export.json");
  let export = first
    .export_file("t", "json", output.to_str(), &Map::new())
    .await
    .unwrap();
  assert_eq!(export.rows, 10000);
  let rows: Vec<serde_json::Value> =
    serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
  assert_eq!(rows.len(), 10000);
  assert_eq!(rows[9999]["id"], 10000);
  let csv = dir.path().join("export.csv");
  first
    .export_file("t", "csv", csv.to_str(), &Map::new())
    .await
    .unwrap();
  let text = std::fs::read_to_string(csv).unwrap();
  assert!(text.starts_with("id,value\n1,first\n"));
  assert!(text.contains("\"comma,quote\"\"cr\r"));
  let sql = dir.path().join("export.sql");
  first
    .export_file("t", "sql", sql.to_str(), &Map::new())
    .await
    .unwrap();
  assert!(std::fs::read_to_string(sql)
    .unwrap()
    .contains("INSERT INTO \"t\""));
  assert!(first
    .export_file("missing", "json", output.to_str(), &Map::new())
    .await
    .is_err());
  assert_eq!(
    serde_json::from_slice::<Vec<serde_json::Value>>(&std::fs::read(&output).unwrap())
      .unwrap()
      .len(),
    10000
  );
  let backup = dir.path().join("backup.sql");
  first
    .backup_database(backup.to_str().unwrap())
    .await
    .unwrap();
  assert!(std::fs::read_to_string(backup).unwrap().contains("10000"));
  let mut confirming = host.find_connection("first").unwrap();
  confirming.safe_mode = Some("confirm".into());
  host.save_connection(&confirming).unwrap();
  assert!(first.execute_query("DELETE FROM t").await.is_err());
  let mut approvals = first.approvals();
  let (outcome, ()) = tokio::join!(first.execute_query("DELETE FROM t"), async {
    let approval = approvals.recv().await.unwrap();
    assert!(approval.detail.contains("DELETE FROM t"));
    approval.answer.send(false).unwrap();
  });
  assert!(outcome.is_err());
  let (outcome, ()) = tokio::join!(first.execute_query("SELECT COUNT(*) AS n FROM t"), async {
    approvals.recv().await.unwrap().answer.send(true).unwrap();
  });
  assert_eq!(outcome.unwrap().rows[0]["n"], 10000);
  host.disconnect("first").await;
  let mut conn = host.find_connection("first").unwrap();
  conn.safe_mode = Some("readonly".into());
  host.save_connection(&conn).unwrap();
  host.connect("first").await.unwrap();
  assert!(host
    .execute_query("DELETE FROM t")
    .await
    .unwrap()
    .error
    .is_some());
  assert_eq!(
    host
      .execute_query("SELECT COUNT(*) AS n FROM t")
      .await
      .unwrap()
      .rows[0]["n"],
    10000
  );
  host.disconnect("first").await;
  host.disconnect("second").await;
  std::env::remove_var("TABLES_DIR");
}
