use crate::actions::*;
use gpui::KeyBinding;

pub fn bindings() -> Vec<KeyBinding> {
  let mut bindings = guise::actions::key_bindings();
  bindings.extend([
    KeyBinding::new("secondary-n", NewConnection, None),
    KeyBinding::new("secondary-p", OpenPalette, None),
    KeyBinding::new("secondary-q", Quit, None),
    KeyBinding::new("secondary-,", OpenSettings, None),
    KeyBinding::new("secondary-shift-r", RefreshTables, None),
    KeyBinding::new("secondary-shift-f", FormatSql, None),
    KeyBinding::new("secondary-e", ExplainQuery, None),
  ]);
  // hiding apps is a macOS convention; ctrl-h is editing elsewhere.
  #[cfg(target_os = "macos")]
  bindings.extend([
    KeyBinding::new("cmd-h", Hide, None),
    KeyBinding::new("alt-cmd-h", HideOthers, None),
  ]);
  bindings
}

pub fn hint(mac: &'static str, other: &'static str) -> &'static str {
  if cfg!(target_os = "macos") {
    mac
  } else {
    other
  }
}
