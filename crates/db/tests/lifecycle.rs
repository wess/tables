use std::sync::Arc;

use db::{HealthMonitor, Registry};

fn config(path: &str) -> model::ConnectionConfig {
  model::ConnectionConfig {
    id: "test".into(),
    kind: "sqlite".into(),
    host: String::new(),
    port: 0,
    database: path.into(),
    username: String::new(),
    password: String::new(),
    filepath: Some(path.into()),
    ssl: None,
    ssh: None,
    startup_commands: None,
  }
}

#[tokio::test]
async fn concurrent_connects_keep_one_database() {
  let registry = Registry::default();
  let config = config(":memory:");
  let (a, b) = tokio::join!(registry.connect(&config), registry.connect(&config));
  a.unwrap();
  b.unwrap();
  assert!(registry.connect_readonly(&config).await.is_err());
  let adapter = registry.adapter("test").unwrap();
  adapter.query("CREATE TABLE t (id INTEGER)").await.unwrap();
  registry.connect(&config).await.unwrap();
  assert!(registry
    .adapter("test")
    .unwrap()
    .get_tables()
    .await
    .unwrap()
    .iter()
    .any(|t| t.name == "t"));
  registry.disconnect("test").await;
  assert!(!registry.is_connected("test"));
}

#[tokio::test]
async fn dropping_monitor_releases_registry() {
  let registry = Arc::new(Registry::default());
  let weak = Arc::downgrade(&registry);
  let monitor = HealthMonitor::default();
  monitor.start("test".into(), registry.clone());
  drop(registry);
  drop(monitor);
  tokio::time::timeout(std::time::Duration::from_secs(1), async {
    while weak.upgrade().is_some() {
      tokio::task::yield_now().await;
    }
  })
  .await
  .unwrap();
}

#[tokio::test]
async fn result_budget_stops_decoding_and_connection_remains_usable() {
  let registry = Registry::default();
  registry.connect(&config(":memory:")).await.unwrap();
  let adapter = registry.adapter("test").unwrap();
  let sql =
    "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<10000) SELECT x FROM n";
  assert!(adapter.query_bounded(sql, 20, 1024).await.is_err());
  assert!(adapter
    .query_bounded("SELECT 'abcdefgh'", 1, 2)
    .await
    .is_err());
  assert_eq!(
    adapter
      .query("-- hello\n/* comment */ SELECT 42 AS n")
      .await
      .unwrap()
      .rows[0]["n"],
    42
  );
  assert!(adapter.query("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<20000) SELECT x FROM n").await.is_err());
}

#[tokio::test]
async fn readonly_connection_rejects_writes_and_startup_commands() {
  let path = std::env::temp_dir().join(format!("tables-{}.db", model::new_uuid()));
  let mut config = config(path.to_str().unwrap());
  let registry = Registry::default();
  registry.connect(&config).await.unwrap();
  registry
    .adapter("test")
    .unwrap()
    .query("CREATE TABLE t (id INTEGER)")
    .await
    .unwrap();
  registry.disconnect("test").await;
  config.startup_commands = Some("DROP TABLE t".into());
  registry.connect_readonly(&config).await.unwrap();
  let adapter = registry.adapter("test").unwrap();
  assert!(adapter.query("INSERT INTO t VALUES (1)").await.is_err());
  assert!(adapter
    .get_tables()
    .await
    .unwrap()
    .iter()
    .any(|t| t.name == "t"));
  registry.disconnect("test").await;
  std::fs::remove_file(path).unwrap();
}
