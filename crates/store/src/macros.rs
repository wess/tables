//! `~/.tables/macros.json` — stored macros. Storage only; execution lives in
//! the UI.

use serde_json::Value;

use crate::paths;
use model::{iso_now, new_uuid, Macro, MacroStep};

const FILE: &str = "macros.json";

/// Missing or corrupt file → [].
pub fn load() -> Vec<Macro> {
    paths::read_json(FILE).ok().flatten().unwrap_or_default()
}

/// Fills id/createdAt when absent, upserts by id.
pub fn save(
    id: Option<String>,
    name: &str,
    steps: Vec<MacroStep>,
    parameters: Option<Vec<String>>,
    shortcut: Option<String>,
    created_at: Option<String>,
) -> Macro {
    let saved = Macro {
        id: id.filter(|s| !s.is_empty()).unwrap_or_else(new_uuid),
        name: name.into(),
        steps,
        parameters,
        shortcut,
        created_at: created_at.filter(|s| !s.is_empty()).unwrap_or_else(iso_now),
    };
    let mut list = load();
    match list.iter().position(|m| m.id == saved.id) {
        Some(i) => list[i] = saved.clone(),
        None => list.push(saved.clone()),
    }
    let _ = paths::write_json(FILE, &list);
    saved
}

/// Filter-out; always returns true.
pub fn remove(id: &str) -> bool {
    let list: Vec<Macro> = load().into_iter().filter(|m| m.id != id).collect();
    let _ = paths::write_json(FILE, &list);
    true
}

/// The macro pretty-printed.
pub fn export(id: &str) -> Result<String, String> {
    let found = load()
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| "Macro not found".to_string())?;
    serde_json::to_string_pretty(&found).map_err(|e| e.to_string())
}

/// Parse when given a JSON string; the imported macro always gets a fresh id
/// and createdAt, then is appended.
pub fn import(data: &Value) -> Result<Macro, String> {
    let mut value = match data {
        Value::String(text) => serde_json::from_str(text).map_err(|e| e.to_string())?,
        other => other.clone(),
    };
    if let Value::Object(map) = &mut value {
        map.insert("id".into(), Value::String(new_uuid()));
        map.insert("createdAt".into(), Value::String(iso_now()));
    }
    let imported: Macro = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let mut list = load();
    list.push(imported.clone());
    paths::write_json(FILE, &list)?;
    Ok(imported)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::testenv;

    #[test]
    fn missing_and_corrupt_load_empty() {
        testenv(|| {
            assert!(load().is_empty());
            std::fs::write(paths::file(FILE), "{").unwrap();
            assert!(load().is_empty());
        });
    }

    #[test]
    fn save_fills_and_upserts() {
        testenv(|| {
            let saved = save(None, "m", vec![], None, None, None);
            assert_eq!(saved.id.len(), 36);
            save(Some(saved.id.clone()), "renamed", vec![], None, None, Some(saved.created_at));
            let list = load();
            assert_eq!(list.len(), 1);
            assert_eq!(list[0].name, "renamed");
        });
    }

    #[test]
    fn remove_always_true() {
        testenv(|| {
            save(Some("m1".into()), "m", vec![], None, None, None);
            assert!(remove("missing"));
            assert!(remove("m1"));
            assert!(load().is_empty());
        });
    }

    #[test]
    fn export_missing_errors() {
        testenv(|| {
            assert_eq!(export("nope"), Err("Macro not found".into()));
            save(Some("m1".into()), "m", vec![], None, None, None);
            let json = export("m1").unwrap();
            assert!(json.contains("\"id\": \"m1\""));
        });
    }

    #[test]
    fn import_overwrites_id_and_created_at() {
        testenv(|| {
            let text = r#"{ "id": "old", "name": "m", "steps": [], "createdAt": "old-time" }"#;
            let imported = import(&Value::String(text.into())).unwrap();
            assert_ne!(imported.id, "old");
            assert_ne!(imported.created_at, "old-time");
            assert_eq!(load().len(), 1);
        });
    }

    #[test]
    fn import_accepts_object_without_id() {
        testenv(|| {
            let value = serde_json::json!({ "name": "m", "steps": [] });
            let imported = import(&value).unwrap();
            assert_eq!(imported.id.len(), 36);
            assert!(import(&Value::String("not json".into())).is_err());
        });
    }
}
