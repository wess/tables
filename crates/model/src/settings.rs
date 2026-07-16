//! UI settings (`~/.tables/settings.json`). Every field has a default, so a
//! partial or old file merges over the defaults exactly.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: String, // light | dark | auto
    pub editor_font_size: f32,
    pub editor_tab_size: usize,
    pub editor_word_wrap: bool,
    pub editor_line_numbers: bool,
    pub grid_row_height: String, // compact | normal | comfortable
    pub grid_page_size: u64,
    pub grid_show_row_numbers: bool,
    pub grid_alternate_rows: bool,
    pub date_format: String,
    pub null_display: String,
    /// AI assistant model id (e.g. `claude-opus-4-8`). The secret itself lives
    /// in the OS keychain, never here.
    pub ai_model: String,
    /// AI auth mode: `apiKey` (pay-per-use key) | `subscription` (Claude OAuth).
    pub ai_auth_mode: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "dark".into(),
            editor_font_size: 13.0,
            editor_tab_size: 2,
            editor_word_wrap: true,
            editor_line_numbers: true,
            grid_row_height: "compact".into(),
            grid_page_size: 100,
            grid_show_row_numbers: true,
            grid_alternate_rows: true,
            date_format: "ISO 8601".into(),
            null_display: "NULL".into(),
            ai_model: "claude-opus-4-8".into(),
            ai_auth_mode: "apiKey".into(),
            extra: Map::new(),
        }
    }
}
