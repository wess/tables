//! The connection workspace: a table-list sidebar, a tabbed main pane (Data /
//! Query / Structure), and a status bar. The panels share one `WorkspaceState`,
//! passed in at construction so several connections never collide.

mod charts;
mod compare;
mod data;
mod dbswitcher;
mod erdiagram;
mod filter;
mod grid;
mod insert;
mod query;
mod review;
mod settings;
mod sidebar;
mod structure;

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, Window};
use guise::prelude::*;
use serde_json::Value;

use crate::bridge;
use crate::state::{AppState, Route, WorkspaceState, WorkspaceTab};
use crate::theme;
use compare::{SchemaCompareEvent, SchemaCompareModal};
use data::DataPanel;
use dbswitcher::DbSwitcher;
use erdiagram::{ErDiagramEvent, ErDiagramModal};
use query::QueryPanel;
use settings::{SettingsEvent, SettingsModal};
use sidebar::Sidebar;
use structure::StructurePanel;

pub struct Workspace {
    app: AppState,
    state: WorkspaceState,
    sidebar: Entity<Sidebar>,
    db_switcher: Entity<DbSwitcher>,
    data: Entity<DataPanel>,
    query: Entity<QueryPanel>,
    structure: Entity<StructurePanel>,
    settings_modal: Option<Entity<SettingsModal>>,
    compare_modal: Option<Entity<SchemaCompareModal>>,
    diagram_modal: Option<Entity<ErDiagramModal>>,
    palette: Entity<Spotlight>,
}

impl Workspace {
    pub fn new(connection_id: String, cx: &mut Context<Self>) -> Self {
        let app = AppState::get(cx);
        let state = WorkspaceState::new(cx, connection_id);
        watch(cx, &state.active_tab);
        watch(cx, &state.active_table);
        watch(cx, &state.connection);
        watch(cx, &state.rows);

        let sidebar = {
            let state = state.clone();
            cx.new(move |cx| Sidebar::new(state, cx))
        };
        let db_switcher = {
            let (app, state) = (app.clone(), state.clone());
            cx.new(move |cx| DbSwitcher::new(app, state, cx))
        };
        let data = {
            let (app, state) = (app.clone(), state.clone());
            cx.new(move |cx| DataPanel::new(app, state, cx))
        };
        let query = {
            let (app, state) = (app.clone(), state.clone());
            cx.new(move |cx| QueryPanel::new(app, state, cx))
        };
        let structure = {
            let (app, state) = (app.clone(), state.clone());
            cx.new(move |cx| StructurePanel::new(app, state, cx))
        };

        let palette = cx.new(Spotlight::new);

        let workspace = Workspace {
            app,
            state,
            sidebar,
            db_switcher,
            data,
            query,
            structure,
            settings_modal: None,
            compare_modal: None,
            diagram_modal: None,
            palette,
        };
        workspace.load(cx);
        workspace
    }

    fn open_diagram(&mut self, cx: &mut Context<Self>) {
        let source = self.state.connection_id.clone();
        let modal = cx.new(|cx| ErDiagramModal::new(source, cx));
        cx.subscribe(&modal, |this, _modal, _event: &ErDiagramEvent, cx| {
            this.diagram_modal = None;
            cx.notify();
        })
        .detach();
        self.diagram_modal = Some(modal);
        cx.notify();
    }

    /// Rebuild the command palette with the current tables + actions, then open.
    fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tables = self.state.tables.get(cx);
        let state = self.state.clone();
        let route = self.app.route.clone();
        let palette = cx.new(move |cx| {
            let mut s = Spotlight::new(cx);
            for table in &tables {
                let st = state.clone();
                let target = table.name.clone();
                s = s.item(table.name.clone(), move |_w, cx| st.select_table(cx, &target));
            }
            let st_filter = state.clone();
            s = s.item_hint("Toggle Filters", "data", move |_w, cx| {
                st_filter.filter_panel_open.update(cx, |o| *o = !*o);
            });
            let st_inspect = state.clone();
            s = s.item_hint("Toggle Inspector", "data", move |_w, cx| {
                st_inspect.inspector_open.update(cx, |o| *o = !*o);
            });
            s = s.item_hint("Back to Connections", "esc", move |_w, cx| {
                route.set(cx, Route::Home);
            });
            s
        });
        palette.update(cx, |s, cx| s.open(window, cx));
        self.palette = palette;
        cx.notify();
    }

    fn open_settings(&mut self, cx: &mut Context<Self>) {
        let modal = cx.new(SettingsModal::new);
        cx.subscribe(&modal, |this, _modal, _event: &SettingsEvent, cx| {
            this.settings_modal = None;
            cx.notify();
        })
        .detach();
        self.settings_modal = Some(modal);
        cx.notify();
    }

    fn open_compare(&mut self, cx: &mut Context<Self>) {
        let source = self.state.connection_id.clone();
        let modal = cx.new(|cx| SchemaCompareModal::new(source, cx));
        cx.subscribe(&modal, |this, _modal, _event: &SchemaCompareEvent, cx| {
            this.compare_modal = None;
            cx.notify();
        })
        .detach();
        self.compare_modal = Some(modal);
        cx.notify();
    }

    /// Fetch the connection's display info and its table list (which also makes
    /// this the host's active connection).
    fn load(&self, cx: &mut gpui::App) {
        let host = self.app.host.clone();
        let id = self.state.connection_id.clone();

        let conn = self.state.connection.clone();
        let (host_c, id_c) = (host.clone(), id.clone());
        bridge::run(
            cx,
            async move { host_c.find_connection(&id_c) },
            move |found, cx| conn.set(cx, found),
        );

        let databases = self.state.databases.clone();
        let (host_d, id_d) = (host.clone(), id.clone());
        bridge::run(
            cx,
            async move { host_d.list_databases(&id_d).await.unwrap_or_default() },
            move |list, cx| databases.set(cx, list),
        );

        let tables = self.state.tables.clone();
        let loading = self.state.tables_loading.clone();
        let error = self.state.tables_error.clone();
        loading.set(cx, true);
        bridge::run(
            cx,
            async move { host.list_tables(&id).await },
            move |result, cx| {
                loading.set(cx, false);
                match result {
                    Ok(list) => {
                        error.set(cx, None);
                        tables.set(cx, list);
                    }
                    // Keep any previously loaded tables; surface the failure.
                    Err(e) => error.set(cx, Some(e)),
                }
            },
        );
    }

    fn leave(&self, cx: &mut Context<Self>) {
        let host = self.app.host.clone();
        let id = self.state.connection_id.clone();
        bridge::run(cx, async move { host.disconnect(&id).await }, |_, _| {});
        self.app.route.set(cx, Route::Home);
    }
}

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::palette(cx);
        let conn = self.state.connection.get(cx);
        let name = conn
            .as_ref()
            .map(|c| c.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Connection".to_string());
        let kind = conn.as_ref().map(|c| c.kind.clone()).unwrap_or_default();
        let active_table = self.state.active_table.get(cx);
        let tab = self.state.active_tab.get(cx);
        let total = self.state.rows.read(cx).as_ref().map(|r| r.total);

        // --- sidebar column ---
        let sidebar_header = div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(colors.border)
            .child(
                Group::new()
                    .gap(Size::Xs)
                    .align(Align::Center)
                    .child(
                        Button::new("ws-back", "←")
                            .size(Size::Xs)
                            .variant(Variant::Subtle)
                            .on_click(cx.listener(|this, _, _, cx| this.leave(cx))),
                    )
                    .child(Text::new(name.clone()).size(Size::Xs).medium()),
            )
            .child(
                Group::new()
                    .gap(Size::Xs)
                    .child(
                        ActionIcon::new("ws-search", "⌘")
                            .variant(Variant::Subtle)
                            .size(Size::Sm)
                            .on_click(cx.listener(|this, _, window, cx| this.open_palette(window, cx))),
                    )
                    .child(
                        ActionIcon::new("ws-settings", "⚙")
                            .variant(Variant::Subtle)
                            .size(Size::Sm)
                            .on_click(cx.listener(|this, _, _, cx| this.open_settings(cx))),
                    ),
            );

        let sidebar_col = div()
            .flex()
            .flex_col()
            .w(px(220.0))
            .h_full()
            .border_r_1()
            .border_color(colors.border)
            .bg(colors.bg_surface)
            .child(sidebar_header)
            .child(self.db_switcher.clone())
            .child(div().flex_1().min_h(px(0.0)).child(self.sidebar.clone()));

        // --- tab bar ---
        let tab_button = |id: &'static str, label: &'static str, this_tab: WorkspaceTab| {
            Button::new(id, label)
                .size(Size::Xs)
                .variant(if tab == this_tab { Variant::Light } else { Variant::Subtle })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.state.active_tab.set(cx, this_tab);
                }))
        };

        let mut tabs = Group::new()
            .gap(Size::Xs)
            .child(tab_button("tab-data", "Data", WorkspaceTab::Data))
            .child(tab_button("tab-query", "Query", WorkspaceTab::Query));
        if active_table.is_some() {
            tabs = tabs.child(tab_button("tab-structure", "Structure", WorkspaceTab::Structure));
        }

        let tabbar = div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.bg_surface)
            .child(tabs)
            .child(
                Group::new()
                    .gap(Size::Xs)
                    .align(Align::Center)
                    .child(Text::new(active_table.clone().unwrap_or_default()).size(Size::Xs).dimmed())
                    .child(
                        Button::new("ws-compare", "⇄ Compare")
                            .size(Size::Xs)
                            .variant(Variant::Subtle)
                            .on_click(cx.listener(|this, _, _, cx| this.open_compare(cx))),
                    )
                    .child(
                        Button::new("ws-erd", "⊟ ER")
                            .size(Size::Xs)
                            .variant(Variant::Subtle)
                            .on_click(cx.listener(|this, _, _, cx| this.open_diagram(cx))),
                    ),
            );

        // --- active panel ---
        let mut body = div().flex().flex_1().min_h(px(0.0)).overflow_hidden();
        body = match tab {
            WorkspaceTab::Data => body.child(self.data.clone()),
            WorkspaceTab::Query => body.child(self.query.clone()),
            WorkspaceTab::Structure => body.child(self.structure.clone()),
        };

        // --- status bar ---
        let mut status = StatusBar::new()
            .left(Text::new(name).size(Size::Xs))
            .left(Badge::new(theme::type_label(&kind)).size(Size::Sm).color(theme::type_color(&kind)));
        if let Some(table) = &active_table {
            status = status.center(Text::new(table.clone()).size(Size::Xs).dimmed());
        }
        if let Some(total) = total {
            status = status.right(Text::new(format!("{total} rows")).size(Size::Xs).dimmed());
        }

        let main_col = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .child(tabbar)
            .child(body)
            .child(status);

        let mut root = div()
            .relative()
            .flex()
            .size_full()
            .on_action(cx.listener(|this, _: &crate::OpenPalette, window, cx| {
                this.open_palette(window, cx)
            }))
            .child(sidebar_col)
            .child(main_col);
        if let Some(modal) = &self.settings_modal {
            root = root.child(modal.clone());
        }
        if let Some(modal) = &self.compare_modal {
            root = root.child(modal.clone());
        }
        if let Some(modal) = &self.diagram_modal {
            root = root.child(modal.clone());
        }
        root.child(self.palette.clone())
    }
}

/// Render a driver cell as display text, honoring the null placeholder.
pub(crate) fn cell_text(value: Option<&Value>, null_display: &str) -> String {
    match value {
        None | Some(Value::Null) => null_display.to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(other) => other.to_string(),
    }
}
