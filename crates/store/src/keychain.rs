//! Secret storage in the OS keychain (macOS Keychain via `keyring`).
//!
//! Connection passwords live here rather than in `connections.json`; only a
//! `secretInKeychain` flag stays in the JSON. Every function degrades
//! gracefully: if the keychain is unavailable, `set` reports an error (so the
//! caller can keep the plaintext), `get` returns `None`, and `delete` is silent.

const SERVICE: &str = "dev.tables.app";

fn entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, id).map_err(|e| e.to_string())
}

/// Store (or replace) the secret for a connection id.
pub fn set_secret(id: &str, secret: &str) -> Result<(), String> {
    entry(id)?.set_password(secret).map_err(|e| e.to_string())
}

/// The stored secret, or None when absent or the keychain is unavailable.
pub fn get_secret(id: &str) -> Option<String> {
    entry(id).ok()?.get_password().ok()
}

/// Remove the secret for a connection id, if any (best-effort).
pub fn delete_secret(id: &str) {
    if let Ok(entry) = entry(id) {
        let _ = entry.delete_credential();
    }
}
