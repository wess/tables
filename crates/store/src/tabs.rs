//! `~/.tables/tabs.json` — open editor tabs, overwritten wholesale by the UI.

use crate::paths;
use model::SavedTab;

const FILE: &str = "tabs.json";

/// Missing or corrupt file → [].
pub fn load() -> Vec<SavedTab> {
    paths::read_json(FILE).ok().flatten().unwrap_or_default()
}

/// Replaces the whole array.
pub fn save_all(tabs: &[SavedTab]) -> bool {
    paths::write_json(FILE, &tabs).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::testenv;

    #[test]
    fn missing_and_corrupt_load_empty() {
        testenv(|| {
            assert!(load().is_empty());
            std::fs::write(paths::file(FILE), "]]").unwrap();
            assert!(load().is_empty());
        });
    }

    #[test]
    fn save_all_overwrites() {
        testenv(|| {
            let tabs = vec![
                SavedTab { id: "1".into(), title: "one".into(), sql: "SELECT 1".into() },
                SavedTab { id: "2".into(), title: "two".into(), sql: "SELECT 2".into() },
            ];
            assert!(save_all(&tabs));
            assert_eq!(load().len(), 2);
            assert!(save_all(&[]));
            assert!(load().is_empty());
        });
    }
}
