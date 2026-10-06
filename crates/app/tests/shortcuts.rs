#[path = "../src/actions.rs"]
#[allow(dead_code)]
mod actions;
#[path = "../src/shortcuts.rs"]
mod shortcuts;

use gpui::{Keymap, Keystroke, TestAppContext};
use guise::prelude::*;

#[test]
fn app_commands_use_the_platform_shortcut_modifier() {
  assert_eq!(
    shortcuts::hint("cmd", "ctrl"),
    if cfg!(target_os = "macos") {
      "cmd"
    } else {
      "ctrl"
    }
  );
  let map = Keymap::new(shortcuts::bindings());
  let modifier = if cfg!(target_os = "macos") {
    "cmd"
  } else {
    "ctrl"
  };
  for (key, action) in [
    (
      "n",
      Box::new(actions::NewConnection) as Box<dyn gpui::Action>,
    ),
    ("p", Box::new(actions::OpenPalette)),
    ("q", Box::new(actions::Quit)),
    (",", Box::new(actions::OpenSettings)),
    ("shift-r", Box::new(actions::RefreshTables)),
    ("shift-f", Box::new(actions::FormatSql)),
    ("e", Box::new(actions::ExplainQuery)),
  ] {
    let key = Keystroke::parse(&format!("{modifier}-{key}")).unwrap();
    let bindings = map.all_bindings_for_input(&[key]);
    assert_eq!(bindings.len(), 1);
    assert!(bindings[0].action().partial_eq(action.as_ref()));
  }
  if !cfg!(target_os = "macos") {
    assert!(map
      .all_bindings_for_input(&[Keystroke::parse("ctrl-h").unwrap()])
      .is_empty());
  }
}

#[gpui::test]
fn text_field_clipboard_shortcuts_and_menu_actions(cx: &mut TestAppContext) {
  cx.update(|cx| {
    Theme::light().init(cx);
    cx.bind_keys(shortcuts::bindings());
  });
  let (field, cx) = cx.add_window_view(|_, cx| TextInput::new(cx).value("copy me"));
  let focus = field.read_with(cx, |field, _| field.focus_handle());
  cx.update(|window, _| window.focus(&focus));
  cx.run_until_parked();

  cx.simulate_keystrokes("secondary-a secondary-c secondary-x");
  assert_eq!(field.read_with(cx, |field, _| field.text()), "");
  assert_eq!(
    cx.read_from_clipboard().and_then(|item| item.text()),
    Some("copy me".into())
  );
  cx.simulate_keystrokes("secondary-v");
  assert_eq!(field.read_with(cx, |field, _| field.text()), "copy me");
  cx.dispatch_action(guise::actions::SelectAll);
  cx.dispatch_action(guise::actions::Cut);
  assert_eq!(field.read_with(cx, |field, _| field.text()), "");
  cx.dispatch_action(guise::actions::Undo);
  assert_eq!(field.read_with(cx, |field, _| field.text()), "copy me");
  cx.dispatch_action(guise::actions::Redo);
  assert_eq!(field.read_with(cx, |field, _| field.text()), "");
}

#[gpui::test]
fn sql_editor_clipboard_shortcuts(cx: &mut TestAppContext) {
  cx.update(|cx| {
    Theme::light().init(cx);
    cx.bind_keys(shortcuts::bindings());
  });
  let (editor, cx) = cx.add_window_view(|_, cx| Editor::new(cx).value("SELECT 1;\nSELECT 2;"));
  let focus = editor.read_with(cx, |editor, _| editor.focus_handle());
  cx.update(|window, _| window.focus(&focus));
  cx.run_until_parked();

  cx.simulate_keystrokes("secondary-a secondary-c secondary-x");
  assert_eq!(editor.read_with(cx, |editor, _| editor.text()), "");
  cx.simulate_keystrokes("secondary-v");
  assert_eq!(
    editor.read_with(cx, |editor, _| editor.text()),
    "SELECT 1;\nSELECT 2;"
  );
  cx.simulate_keystrokes("secondary-z");
  assert_eq!(editor.read_with(cx, |editor, _| editor.text()), "");
  cx.simulate_keystrokes("secondary-shift-z");
  assert_eq!(
    editor.read_with(cx, |editor, _| editor.text()),
    "SELECT 1;\nSELECT 2;"
  );
}
