use gpui::prelude::*;
use gpui::{div, px, App, Div, MouseButton, WindowControlArea};
use guise::prelude::*;

pub const HEIGHT: f32 = 34.0;

pub fn strip(cx: &App) -> Div {
  let colors = crate::theme::palette(cx);
  div()
    .flex()
    .items_center()
    .flex_none()
    .h(px(HEIGHT))
    .w_full()
    .pl(px(if cfg!(target_os = "macos") { 88.0 } else { 8.0 }))
    .pr(px(8.0))
    .bg(colors.bg_titlebar)
    .border_b_1()
    .border_color(colors.border)
}

pub fn drag() -> gpui::Stateful<Div> {
  div()
    .id("window-drag")
    .flex_1()
    .min_w(px(24.0))
    .h_full()
    .flex()
    .items_center()
    .overflow_hidden()
    .window_control_area(WindowControlArea::Drag)
    .on_mouse_down(MouseButton::Left, |event, window, _| {
      if event.click_count == 2 {
        window.zoom_window();
      } else {
        window.start_window_move();
      }
    })
}

pub fn bar(title: String, cx: &App) -> Div {
  strip(cx).child(
    drag().child(
      div()
        .truncate()
        .child(Text::new(title).size(Size::Sm).medium()),
    ),
  )
}

pub fn update_button(cx: &App) -> ActionIcon {
  let checking = crate::update::checking(cx);
  ActionIcon::new(
    "title-update",
    if checking {
      IconName::LoaderCircle
    } else {
      IconName::Download
    },
  )
  .disabled(checking)
  .label(if checking {
    "Checking for updates…"
  } else {
    "Check for updates"
  })
  .size(Size::Sm)
  .on_click(|_, _, cx| crate::update::check_now(cx))
}

pub fn settings_button() -> ActionIcon {
  ActionIcon::new("title-settings", IconName::Settings)
    .label("Settings (⌘,)")
    .size(Size::Sm)
    .on_click(|_, window, cx| window.dispatch_action(Box::new(crate::OpenSettings), cx))
}
