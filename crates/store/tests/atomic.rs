use std::sync::Arc;

#[test]
fn concurrent_writes_always_leave_complete_json() {
  let dir = tempfile::tempdir().unwrap();
  std::env::set_var("TABLES_DIR", dir.path());
  let readers_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
  store::paths::write_json("atomic.json", &vec![0; 500]).unwrap();
  std::thread::scope(|scope| {
    let done = readers_done.clone();
    scope.spawn(move || {
      while !done.load(std::sync::atomic::Ordering::Relaxed) {
        let value = store::paths::read_json::<Vec<u32>>("atomic.json")
          .unwrap()
          .unwrap();
        assert_eq!(value.len(), 500);
        assert!(value.iter().all(|n| *n == value[0]));
      }
    });
    let writers: Vec<_> = (0..4)
      .map(|n| {
        scope.spawn(move || {
          for _ in 0..10 {
            store::paths::write_json("atomic.json", &vec![n; 500]).unwrap();
          }
        })
      })
      .collect();
    for writer in writers {
      writer.join().unwrap();
    }
    readers_done.store(true, std::sync::atomic::Ordering::Relaxed);
  });
  std::thread::scope(|scope| {
    for i in 0..32 {
      scope.spawn(move || {
        store::favorites::save(None, &format!("Query {i}"), "SELECT 1", None).unwrap();
        store::history::append(model::HistoryEntry {
          id: format!("{i}"),
          sql: "SELECT 1".into(),
          connection_id: "sample".into(),
          executed_at: model::iso_now(),
          execution_time: 0,
          rows_affected: 1,
          error: None,
        })
        .unwrap();
      });
    }
  });
  assert_eq!(store::favorites::load().unwrap().len(), 32);
  assert_eq!(store::history::load().unwrap().len(), 32);
  std::env::remove_var("TABLES_DIR");
}
