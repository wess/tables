//! `~/.tables/settings.json` — the UI's settings object.

use serde_json::Value;

use crate::paths;
use model::Settings;

const FILE: &str = "settings.json";

/// Missing or unparseable file → None.
pub fn load() -> Option<Value> {
    paths::read_json(FILE).ok().flatten()
}

/// Writes the object verbatim, pretty-printed.
pub fn save(value: &Value) -> bool {
    paths::write_json(FILE, value).is_ok()
}

/// Typed view for the UI: saved fields merged over defaults.
pub fn load_settings() -> Settings {
    load()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::testenv;

    #[test]
    fn missing_file_is_none() {
        testenv(|| assert_eq!(load(), None));
    }

    #[test]
    fn corrupt_file_is_none() {
        testenv(|| {
            std::fs::write(paths::file(FILE), "{oops").unwrap();
            assert_eq!(load(), None);
        });
    }

    #[test]
    fn save_round_trips_verbatim() {
        testenv(|| {
            let value = serde_json::json!({ "theme": "light", "custom": [1, 2] });
            assert!(save(&value));
            assert_eq!(load(), Some(value));
        });
    }

    #[test]
    fn typed_load_merges_defaults() {
        testenv(|| {
            save(&serde_json::json!({ "theme": "light" }));
            let settings = load_settings();
            assert_eq!(settings.theme, "light");
            assert_eq!(settings.grid_page_size, 100);
        });
    }

    #[test]
    fn typed_load_falls_back_to_defaults() {
        testenv(|| {
            let settings = load_settings();
            assert_eq!(settings.theme, "dark");
            assert_eq!(settings.null_display, "NULL");
        });
    }
}
