//! `~/.tables/history.json` — query history, newest first, capped at 500.
//!
//! A corrupt file makes load (and append) fail.

use crate::paths;
use model::HistoryEntry;

const FILE: &str = "history.json";
const MAX: usize = 500;

pub fn load() -> Result<Vec<HistoryEntry>, String> {
    Ok(paths::read_json(FILE)?.unwrap_or_default())
}

/// Prepend, truncate to 500, write.
pub fn append(entry: HistoryEntry) -> Result<(), String> {
    let mut list = load()?;
    list.insert(0, entry);
    list.truncate(MAX);
    paths::write_json(FILE, &list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::testenv;

    fn entry(sql: &str) -> HistoryEntry {
        HistoryEntry {
            id: model::new_uuid(),
            sql: sql.into(),
            connection_id: "c1".into(),
            executed_at: model::iso_now(),
            execution_time: 1,
            rows_affected: 0,
            error: None,
        }
    }

    #[test]
    fn missing_file_loads_empty() {
        testenv(|| assert!(load().unwrap().is_empty()));
    }

    #[test]
    fn corrupt_file_errors() {
        testenv(|| {
            std::fs::write(paths::file(FILE), "[oops").unwrap();
            assert!(load().is_err());
            assert!(append(entry("SELECT 1")).is_err());
        });
    }

    #[test]
    fn append_prepends() {
        testenv(|| {
            append(entry("first")).unwrap();
            append(entry("second")).unwrap();
            let list = load().unwrap();
            assert_eq!(list[0].sql, "second");
            assert_eq!(list[1].sql, "first");
        });
    }

    #[test]
    fn append_truncates_to_max() {
        testenv(|| {
            let full: Vec<HistoryEntry> = (0..MAX).map(|i| entry(&i.to_string())).collect();
            paths::write_json(FILE, &full).unwrap();
            append(entry("newest")).unwrap();
            let list = load().unwrap();
            assert_eq!(list.len(), MAX);
            assert_eq!(list[0].sql, "newest");
            assert_eq!(list[MAX - 1].sql, (MAX - 2).to_string());
        });
    }
}
