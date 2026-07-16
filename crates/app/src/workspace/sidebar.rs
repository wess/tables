//! The table-list sidebar. Selecting a table runs `WorkspaceState::select_table`,
//! which resets the data-tab state.

use gpui::prelude::*;
use gpui::{div, px, Context, SharedString, Window};
use guise::prelude::*;

use crate::state::WorkspaceState;

pub struct Sidebar {
    state: WorkspaceState,
}

impl Sidebar {
    pub fn new(state: WorkspaceState, cx: &mut Context<Self>) -> Self {
        watch(cx, &state.tables);
        watch(cx, &state.tables_loading);
        watch(cx, &state.tables_error);
        watch(cx, &state.active_table);
        Sidebar { state }
    }
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if *self.state.tables_loading.read(cx) {
            return div()
                .p(px(12.0))
                .child(
                    Group::new()
                        .gap(Size::Xs)
                        .align(Align::Center)
                        .child(Loader::new().size(Size::Sm))
                        .child(Text::new("Loading…").size(Size::Xs).dimmed()),
                )
                .into_any_element();
        }

        let tables = self.state.tables.get(cx);
        let active = self.state.active_table.get(cx);

        if tables.is_empty() {
            // A failed load is distinct from a genuinely empty schema.
            if let Some(error) = self.state.tables_error.read(cx).clone() {
                let red = guise::theme::theme(cx).color(ColorName::Red, 6);
                return div()
                    .p(px(12.0))
                    .child(
                        Stack::new()
                            .gap(Size::Xs)
                            .child(Text::new("Failed to load tables").size(Size::Xs).color(red))
                            .child(Text::new(error).size(Size::Xs).dimmed()),
                    )
                    .into_any_element();
            }
            return div()
                .p(px(12.0))
                .child(Text::new("No tables").size(Size::Xs).dimmed())
                .into_any_element();
        }

        let mut list = Stack::new().gap(Size::Xs);
        for table in &tables {
            let name = table.name.clone();
            let is_active = active.as_deref() == Some(name.as_str());
            let icon = if table.kind == "view" { "◇" } else { "▤" };
            let for_click = name.clone();
            list = list.child(
                NavLink::new(SharedString::from(format!("tbl-{name}")), name.clone())
                    .icon(icon)
                    .active(is_active)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.select_table(cx, &for_click);
                    })),
            );
        }

        div()
            .id("sidebar-scroll")
            .size_full()
            .overflow_y_scroll()
            .p(px(6.0))
            .child(list)
            .into_any_element()
    }
}
