//! `~/.tables/connections.json` — stored connections.
//!
//! A corrupt file is backed up to `connections.json.corrupt` and treated as
//! empty, unlike history/favorites which throw.

use crate::paths;
use model::{new_uuid, StoredConnection};

const FILE: &str = "connections.json";

pub fn load() -> Vec<StoredConnection> {
    match paths::read_json(FILE) {
        Ok(Some(list)) => list,
        Ok(None) => Vec::new(),
        Err(_) => {
            let _ = std::fs::copy(paths::file(FILE), paths::file("connections.json.corrupt"));
            Vec::new()
        }
    }
}

pub fn save_all(list: &[StoredConnection]) -> Result<(), String> {
    paths::write_json(FILE, &list)
}

/// Fills a uuid when the id is empty, replaces in place by id or appends,
/// returns the saved connection.
pub fn upsert(conn: &StoredConnection) -> Result<StoredConnection, String> {
    let mut saved = conn.clone();
    if saved.id.is_empty() {
        saved.id = new_uuid();
    }
    let mut list = load();
    match list.iter().position(|c| c.id == saved.id) {
        Some(i) => list[i] = saved.clone(),
        None => list.push(saved.clone()),
    }
    save_all(&list)?;
    Ok(saved)
}

/// The delete file step — false when the id wasn't stored.
pub fn remove(id: &str) -> bool {
    let mut list = load();
    let before = list.len();
    list.retain(|c| c.id != id);
    if list.len() == before {
        return false;
    }
    let _ = save_all(&list);
    true
}

pub fn find(id: &str) -> Option<StoredConnection> {
    load().into_iter().find(|c| c.id == id)
}

/// Test-only: run `f` with `TABLES_DIR` pointed at a fresh temp dir. The env
/// var is process-wide, so a global lock serializes all store tests.
#[cfg(test)]
pub(crate) fn testenv<T>(f: impl FnOnce() -> T) -> T {
    use std::sync::Mutex;
    static LOCK: Mutex<()> = Mutex::new(());
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("tablestest{}", new_uuid()));
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("TABLES_DIR", &dir);
    let result = f();
    std::env::remove_var("TABLES_DIR");
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    fn conn(id: &str, name: &str) -> StoredConnection {
        StoredConnection {
            id: id.into(),
            name: name.into(),
            kind: "sqlite".into(),
            host: String::new(),
            port: 0,
            database: String::new(),
            username: String::new(),
            password: String::new(),
            color: String::new(),
            filepath: None,
            ssl: None,
            ssh: None,
            startup_commands: None,
            safe_mode: None,
            group: None,
            tags: None,
            extra: Map::new(),
        }
    }

    #[test]
    fn missing_file_loads_empty() {
        testenv(|| assert!(load().is_empty()));
    }

    #[test]
    fn upsert_fills_uuid_and_appends() {
        testenv(|| {
            let saved = upsert(&conn("", "a")).unwrap();
            assert_eq!(saved.id.len(), 36);
            assert_eq!(load().len(), 1);
        });
    }

    #[test]
    fn upsert_replaces_in_place() {
        testenv(|| {
            upsert(&conn("one", "a")).unwrap();
            upsert(&conn("two", "b")).unwrap();
            upsert(&conn("one", "renamed")).unwrap();
            let list = load();
            assert_eq!(list.len(), 2);
            assert_eq!(list[0].id, "one");
            assert_eq!(list[0].name, "renamed");
        });
    }

    #[test]
    fn remove_reports_presence() {
        testenv(|| {
            upsert(&conn("one", "a")).unwrap();
            assert!(!remove("missing"));
            assert!(remove("one"));
            assert!(load().is_empty());
        });
    }

    #[test]
    fn corrupt_file_backed_up_then_empty() {
        testenv(|| {
            std::fs::write(paths::file(FILE), "not json").unwrap();
            assert!(load().is_empty());
            assert!(paths::file("connections.json.corrupt").exists());
        });
    }

    #[test]
    fn extra_fields_round_trip() {
        testenv(|| {
            let mut c = conn("one", "a");
            c.extra.insert("customField".into(), serde_json::json!(42));
            upsert(&c).unwrap();
            assert_eq!(load()[0].extra.get("customField"), Some(&serde_json::json!(42)));
        });
    }
}
