use gpui::prelude::*;
use gpui::{div, px, App, Div};
use guise::prelude::*;

use crate::state::{WorkspaceState, WorkspaceTab};

pub fn selector(state: &WorkspaceState, cx: &App) -> Div {
  let theme = guise::theme::theme(cx);
  let colors = crate::theme::palette(cx);
  let selected = match theme.scheme {
    ColorScheme::Dark => theme.surface_hover().hsla(),
    ColorScheme::Light => theme.body().hsla(),
  };
  let active = state.active_tab.get(cx);
  let mut row = div().flex().flex_none().items_center().gap(px(2.0));
  for (id, label, target) in [
    ("view-data", "Data", WorkspaceTab::Data),
    ("view-structure", "Structure", WorkspaceTab::Structure),
  ] {
    let mouse = state.clone();
    let keyboard = state.clone();
    row = row.child(
      div()
        .id(id)
        .flex()
        .items_center()
        .h(px(28.0))
        .px(px(10.0))
        .rounded(px(4.0))
        .cursor_pointer()
        .bg(if active == target { selected } else { colors.bg_subtle })
        .tab_index(0)
        .focus(move |style| style.bg(selected))
        .on_key_down(move |event, window, cx| {
          if event.keystroke.key == "tab"
            && !event.keystroke.modifiers.platform
            && !event.keystroke.modifiers.control
          {
            if event.keystroke.modifiers.shift {
              window.focus_prev();
            } else {
              window.focus_next();
            }
            cx.stop_propagation();
          }
          if matches!(event.keystroke.key.as_str(), "enter" | "space") {
            keyboard.active_tab.set(cx, target);
            cx.stop_propagation();
          }
        })
        .on_click(move |_, _, cx| mouse.active_tab.set(cx, target))
        .child(Text::new(label).size(Size::Xs)),
    );
  }
  row
}
