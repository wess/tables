//! Small shared helpers: timestamp and id generation.

/// Now as `new Date().toISOString()` produces it: UTC with milliseconds.
pub fn iso_now() -> String {
    chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// Lowercase hyphenated UUID v4, like `crypto.randomUUID()`.
pub fn new_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}
