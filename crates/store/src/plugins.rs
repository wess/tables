//! `~/.tables/plugins/` + `~/.tables/plugins.json` — inert plugin files.
//! No loading/execution: only listing, toggling, installing, uninstalling.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde_json::{Map, Value};

use crate::paths;
use model::{InstalledPlugin, PluginManifest};

const CONFIG: &str = "plugins.json";
const REGISTRY_URL: &str = "https://raw.githubusercontent.com/tables-app/plugins/main/registry.json";

fn dir() -> PathBuf {
    let d = paths::tables_dir().join("plugins");
    let _ = fs::create_dir_all(&d);
    d
}

/// Missing or corrupt config → {}.
fn config() -> Map<String, Value> {
    paths::read_json(CONFIG).ok().flatten().unwrap_or_default()
}

/// Every directory with a parseable manifest.json; enabled unless the config
/// says exactly `false`.
pub fn list() -> Vec<InstalledPlugin> {
    let config = config();
    let mut plugins = Vec::new();
    let Ok(entries) = fs::read_dir(dir()) else {
        return plugins;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Ok(text) = fs::read_to_string(path.join("manifest.json")) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<PluginManifest>(&text) else {
            continue;
        };
        let enabled = !matches!(config.get(&manifest.name), Some(Value::Bool(false)));
        plugins.push(InstalledPlugin {
            manifest,
            path: path.to_string_lossy().into_owned(),
            enabled,
        });
    }
    plugins
}

/// Set `config[name] = enabled`.
pub fn toggle(name: &str, enabled: bool) -> bool {
    let mut config = config();
    config.insert(name.into(), Value::Bool(enabled));
    paths::write_json(CONFIG, &config).is_ok()
}

fn fetch(url: &str) -> std::io::Result<std::process::Output> {
    Command::new("curl")
        .args(["-fsSL", "--max-time", "10", url])
        .output()
}

/// HTTP error (curl exit 22) or unparseable body → []; network/spawn failure →
/// the hardcoded fallback list.
pub fn registry() -> Vec<PluginManifest> {
    let Ok(output) = fetch(REGISTRY_URL) else {
        return fallback();
    };
    match output.status.code() {
        Some(0) => serde_json::from_slice(&output.stdout).unwrap_or_default(),
        Some(22) => Vec::new(),
        _ => fallback(),
    }
}

fn fallback() -> Vec<PluginManifest> {
    [
        ("tables-mongodb", "MongoDB driver", "driver"),
        ("tables-redis", "Redis driver", "driver"),
        ("tables-duckdb", "DuckDB driver", "driver"),
        ("tables-xlsx-export", "Excel XLSX export", "export"),
        ("tables-dracula", "Dracula theme", "theme"),
        ("tables-nord", "Nord theme", "theme"),
    ]
    .into_iter()
    .map(|(name, description, kind)| PluginManifest {
        name: name.into(),
        version: "0.1.0".into(),
        description: description.into(),
        kind: kind.into(),
        author: Some("tables".into()),
        entry: None,
        extra: Map::new(),
    })
    .collect()
}

/// mkdir; when a `url` rides along in the manifest's extra fields, download it
/// to `index.ts`; write a default manifest.json only when none exists.
pub fn install(data: &PluginManifest) -> Result<(), String> {
    let plugin_dir = dir().join(&data.name);
    fs::create_dir_all(&plugin_dir).map_err(|e| e.to_string())?;

    if let Some(url) = data.extra.get("url").and_then(Value::as_str) {
        let output = fetch(url).map_err(|e| format!("Failed to download plugin: {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let msg = if stderr.is_empty() {
                format!("curl exited with code {}", output.status.code().unwrap_or(-1))
            } else {
                stderr
            };
            return Err(format!("Failed to download plugin: {msg}"));
        }
        fs::write(plugin_dir.join("index.ts"), &output.stdout)
            .map_err(|e| format!("Failed to download plugin: {e}"))?;
    }

    let manifest_path = plugin_dir.join("manifest.json");
    if !manifest_path.exists() {
        let manifest = PluginManifest {
            name: data.name.clone(),
            version: if data.version.is_empty() { "0.1.0".into() } else { data.version.clone() },
            description: data.description.clone(),
            kind: if data.kind.is_empty() { "driver".into() } else { data.kind.clone() },
            author: None,
            entry: Some("index.ts".into()),
            extra: Map::new(),
        };
        let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
        fs::write(&manifest_path, text).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// rm -rf, errors swallowed.
pub fn uninstall(name: &str) {
    let _ = fs::remove_dir_all(dir().join(name));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connections::testenv;

    fn manifest(name: &str) -> PluginManifest {
        PluginManifest {
            name: name.into(),
            version: String::new(),
            description: String::new(),
            kind: String::new(),
            author: None,
            entry: None,
            extra: Map::new(),
        }
    }

    #[test]
    fn list_skips_bad_manifests_and_reads_enabled() {
        testenv(|| {
            let base = dir();
            fs::create_dir_all(base.join("good")).unwrap();
            fs::write(
                base.join("good/manifest.json"),
                r#"{ "name": "good", "version": "1.0.0", "description": "", "type": "driver", "entry": "index.ts" }"#,
            )
            .unwrap();
            fs::create_dir_all(base.join("bad")).unwrap();
            fs::write(base.join("bad/manifest.json"), "{oops").unwrap();
            fs::create_dir_all(base.join("empty")).unwrap();
            fs::write(base.join("stray.txt"), "not a dir").unwrap();

            let plugins = list();
            assert_eq!(plugins.len(), 1);
            assert_eq!(plugins[0].manifest.name, "good");
            assert!(plugins[0].enabled);

            toggle("good", false);
            assert!(!list()[0].enabled);
            toggle("good", true);
            assert!(list()[0].enabled);
        });
    }

    #[test]
    fn install_writes_default_manifest_once() {
        testenv(|| {
            install(&manifest("p1")).unwrap();
            let path = dir().join("p1/manifest.json");
            let written: PluginManifest =
                serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(written.version, "0.1.0");
            assert_eq!(written.kind, "driver");
            assert_eq!(written.entry.as_deref(), Some("index.ts"));

            let mut again = manifest("p1");
            again.version = "9.9.9".into();
            install(&again).unwrap();
            let kept: PluginManifest =
                serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(kept.version, "0.1.0");
        });
    }

    #[test]
    fn uninstall_removes_and_swallows_missing() {
        testenv(|| {
            install(&manifest("p1")).unwrap();
            uninstall("p1");
            assert!(!dir().join("p1").exists());
            uninstall("p1"); // no panic when absent
        });
    }

    #[test]
    fn fallback_list_is_verbatim() {
        let list = fallback();
        assert_eq!(list.len(), 6);
        assert_eq!(list[0].name, "tables-mongodb");
        assert_eq!(list[3].kind, "export");
        assert_eq!(list[5].name, "tables-nord");
        assert!(list.iter().all(|m| m.version == "0.1.0" && m.author.as_deref() == Some("tables")));
    }
}
