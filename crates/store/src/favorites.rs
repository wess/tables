//! `~/.tables/favorites.json` — saved queries.
//!
//! Like history, a corrupt file makes every operation fail.

use crate::paths;
use model::{iso_now, new_uuid, Favorite};

const FILE: &str = "favorites.json";

pub fn load() -> Result<Vec<Favorite>, String> {
    Ok(paths::read_json(FILE)?.unwrap_or_default())
}

/// Fills id/createdAt when absent (empty strings count as absent), upserts by id.
pub fn save(
    id: Option<String>,
    name: &str,
    sql: &str,
    created_at: Option<String>,
) -> Result<Favorite, String> {
    let favorite = Favorite {
        id: id.filter(|s| !s.is_empty()).unwrap_or_else(new_uuid),
        name: name.into(),
        sql: sql.into(),
        created_at: created_at.filter(|s| !s.is_empty()).unwrap_or_else(iso_now),
    };
    let mut list = load()?;
    match list.iter().position(|f| f.id == favorite.id) {
        Some(i) => list[i] = favorite.clone(),
        None => list.push(favorite.clone()),
    }
    paths::write_json(FILE, &list)?;
    Ok(favorite)
}

/// Ok(false) when the id wasn't stored.
pub fn remove(id: &str) -> Result<bool, String> {
    let mut list = load()?;
    let before = list.len();
    list.retain(|f| f.id != id);
    if list.len() == before {
        return Ok(false);
    }
    paths::write_json(FILE, &list)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::testenv;

    #[test]
    fn missing_file_loads_empty() {
        testenv(|| assert!(load().unwrap().is_empty()));
    }

    #[test]
    fn corrupt_file_errors() {
        testenv(|| {
            std::fs::write(paths::file(FILE), "nope").unwrap();
            assert!(load().is_err());
            assert!(save(None, "a", "SELECT 1", None).is_err());
            assert!(remove("x").is_err());
        });
    }

    #[test]
    fn save_fills_id_and_created_at() {
        testenv(|| {
            let fav = save(None, "a", "SELECT 1", None).unwrap();
            assert_eq!(fav.id.len(), 36);
            assert!(fav.created_at.ends_with('Z'));
            let empty = save(Some(String::new()), "b", "SELECT 2", Some(String::new())).unwrap();
            assert_eq!(empty.id.len(), 36);
            assert!(!empty.created_at.is_empty());
        });
    }

    #[test]
    fn save_upserts_by_id() {
        testenv(|| {
            let first = save(Some("f1".into()), "a", "SELECT 1", Some("t1".into())).unwrap();
            save(Some("f2".into()), "b", "SELECT 2", None).unwrap();
            save(Some("f1".into()), "renamed", "SELECT 3", Some(first.created_at)).unwrap();
            let list = load().unwrap();
            assert_eq!(list.len(), 2);
            assert_eq!(list[0].id, "f1");
            assert_eq!(list[0].name, "renamed");
        });
    }

    #[test]
    fn remove_reports_presence() {
        testenv(|| {
            save(Some("f1".into()), "a", "SELECT 1", None).unwrap();
            assert!(!remove("missing").unwrap());
            assert!(remove("f1").unwrap());
            assert!(load().unwrap().is_empty());
        });
    }
}
