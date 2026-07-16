//! `~/.tables/` — the storage directory for all local metadata.

use std::fs;
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::Serialize;

/// The storage directory, created lazily. The `TABLES_DIR` env var overrides
/// the location (read per call; tests use it).
pub fn tables_dir() -> PathBuf {
    let dir = std::env::var_os("TABLES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".tables")
        });
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn file(name: &str) -> PathBuf {
    tables_dir().join(name)
}

/// Read + parse a JSON file. `Ok(None)` when the file is missing;
/// `Err` when it exists but won't parse (callers pick the recovery).
pub fn read_json<T: DeserializeOwned>(name: &str) -> Result<Option<T>, String> {
    let path = file(name);
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(None);
    };
    serde_json::from_str(&text).map(Some).map_err(|e| e.to_string())
}

/// Pretty-print with 2-space indent.
pub fn write_json<T: Serialize>(name: &str, value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(file(name), text).map_err(|e| e.to_string())
}
