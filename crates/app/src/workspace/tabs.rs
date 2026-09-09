use gpui::prelude::*;
use gpui::{div, px, Context};
use guise::prelude::*;

use super::Workspace;
use crate::state::WorkspaceTab;

impl Workspace {
  pub(super) fn documents(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
    let colors = crate::theme::palette(cx);
    let tab = self.state.active_tab.get(cx);
    let active_table = self.state.active_table.get(cx);
    let tables = self.state.open_tables.get(cx);
    let selected_key = if tab == WorkspaceTab::Query {
      None
    } else {
      active_table.clone()
    };
    if self.followed_tab != selected_key {
      let index = selected_key
        .as_ref()
        .and_then(|selected| tables.iter().position(|table| table == selected))
        .map_or(0, |index| index + 1);
      self.tab_scroll.scroll_to_item(index);
      self.followed_tab = selected_key;
    }
    let selected_fill = match guise::theme::theme(cx).scheme {
      ColorScheme::Dark => guise::theme::theme(cx).surface_hover().hsla(),
      ColorScheme::Light => guise::theme::theme(cx).body().hsla(),
    };
    let tab_shell = |selected| {
      div()
        .flex()
        .items_center()
        .flex_shrink()
        .h_full()
        .px(px(8.0))
        .gap(px(4.0))
        .min_w(px(96.0))
        .max_w(px(240.0))
        .bg(if selected {
          selected_fill
        } else {
          colors.bg_titlebar
        })
        .cursor_pointer()
        .hover(move |style| style.bg(selected_fill))
    };
    let mut table_tabs = div()
      .id("table-tabs")
      .flex()
      .flex_shrink()
      .min_w(px(0.0))
      .h_full()
      .items_center()
      .overflow_x_scroll()
      .track_scroll(&self.tab_scroll)
      .child(
        tab_shell(tab == WorkspaceTab::Query)
          .id("query-tab")
          .tab_index(0)
          .focus(move |style| style.bg(selected_fill))
          .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
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
              this.state.active_tab.set(cx, WorkspaceTab::Query);
              cx.stop_propagation();
            }
          }))
          .on_click(
            cx.listener(|this, _, _, cx| this.state.active_tab.set(cx, WorkspaceTab::Query)),
          )
          .child(Icon::new(IconName::SquareTerminal).size(Size::Xs))
          .child(Text::new("SQL editor").size(Size::Xs)),
      );
    for table in tables {
      let hover_group: gpui::SharedString = format!("document-{table}").into();
      let selected = tab != WorkspaceTab::Query && active_table.as_ref() == Some(&table);
      let dirty = self.state.table_dirty(&table, cx);
      let select = table.clone();
      let keyboard = table.clone();
      let close = table.clone();
      table_tabs = table_tabs.child(
        tab_shell(selected)
          .group(hover_group.clone())
          .id(gpui::ElementId::Name(format!("table-{table}").into()))
          .tab_index(0)
          .focus(move |style| style.bg(selected_fill))
          .on_key_down(
            cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
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
                this.state.select_table(cx, &keyboard);
                cx.stop_propagation();
              }
            }),
          )
          .on_click(cx.listener(move |this, _, _, cx| this.state.select_table(cx, &select)))
          .child(Icon::new(IconName::Table2).size(Size::Xs))
          .child(
            div().flex_1().min_w(px(0.0)).truncate().child(
              Text::new(if dirty {
                format!("{table} •")
              } else {
                table.clone()
              })
              .size(Size::Xs),
            ),
          )
          .child(
            div()
              .flex_none()
              .when(!selected && !dirty, |slot| slot.invisible())
              .group_hover(hover_group, |slot| slot.visible())
              .child(
                ActionIcon::new(
                  gpui::ElementId::Name(format!("close-{table}").into()),
                  IconName::X,
                )
                .size(Size::Xs)
                .label(if dirty {
                  "Commit or discard edits to close"
                } else {
                  "Close table"
                })
                .disabled(dirty)
                .on_click(cx.listener(move |this, _, _, cx| {
                  cx.stop_propagation();
                  this.state.close_table(&close, cx);
                })),
              ),
          ),
      );
    }
    table_tabs
  }
}
