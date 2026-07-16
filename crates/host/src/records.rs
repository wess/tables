//! Settings, macros, and plugins handlers — thin pass-throughs to the `store`
//! modules plus a generic file read.

use serde_json::Value;

use model::{InstalledPlugin, Macro, MacroStep, PluginManifest, Settings};
use store::{keychain, macros, plugins, settings};

use crate::facade::Host;

/// Keychain slot for the assistant secret, keyed by auth mode so an API key and
/// a subscription token never clobber each other.
fn ai_slot(mode: &str) -> String {
    match mode {
        "subscription" => "ai:anthropic:oauth".to_string(),
        _ => "ai:anthropic:key".to_string(),
    }
}

impl Host {
    /// The file's text, or None when it doesn't exist.
    pub fn read_file(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    /// The raw stored settings object (None when missing/corrupt).
    pub fn settings_raw(&self) -> Option<Value> {
        settings::load()
    }

    /// The typed settings the UI reads, saved fields merged over defaults.
    pub fn settings(&self) -> Settings {
        settings::load_settings()
    }

    pub fn save_settings(&self, value: &Value) -> bool {
        settings::save(value)
    }

    /// Store the assistant secret (API key or subscription token) in the OS
    /// keychain. Degrades gracefully when the keychain is unavailable.
    pub fn save_ai_secret(&self, mode: &str, secret: &str) -> Result<(), String> {
        keychain::set_secret(&ai_slot(mode), secret)
    }

    /// The stored assistant secret for `mode`, or None when absent.
    pub fn ai_secret(&self, mode: &str) -> Option<String> {
        keychain::get_secret(&ai_slot(mode)).filter(|s| !s.is_empty())
    }

    /// Whether a secret is stored for `mode`.
    pub fn has_ai_secret(&self, mode: &str) -> bool {
        self.ai_secret(mode).is_some()
    }

    pub fn list_macros(&self) -> Vec<Macro> {
        macros::load()
    }

    pub fn save_macro(
        &self,
        id: Option<String>,
        name: &str,
        steps: Vec<MacroStep>,
        parameters: Option<Vec<String>>,
        shortcut: Option<String>,
    ) -> Macro {
        macros::save(id, name, steps, parameters, shortcut, None)
    }

    pub fn delete_macro(&self, id: &str) -> bool {
        macros::remove(id)
    }

    pub fn export_macro(&self, id: &str) -> Result<String, String> {
        macros::export(id)
    }

    pub fn import_macro(&self, data: &Value) -> Result<Macro, String> {
        macros::import(data)
    }

    pub fn list_plugins(&self) -> Vec<InstalledPlugin> {
        plugins::list()
    }

    pub fn toggle_plugin(&self, name: &str, enabled: bool) -> bool {
        plugins::toggle(name, enabled)
    }

    pub fn plugin_registry(&self) -> Vec<PluginManifest> {
        plugins::registry()
    }

    pub fn install_plugin(&self, manifest: &PluginManifest) -> Result<(), String> {
        plugins::install(manifest)
    }

    pub fn uninstall_plugin(&self, name: &str) {
        plugins::uninstall(name)
    }
}
