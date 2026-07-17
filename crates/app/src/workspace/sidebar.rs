//! The table-list sidebar. A filter box narrows the list; objects are grouped
//! by kind (tables, then views). Selecting one runs
//! `WorkspaceState::select_table`; right-clicking opens a "Generate SQL" menu.

use gpui::prelude::*;
use gpui::{
    div, px, ClipboardItem, Context, Entity, MouseButton, MouseDownEvent, Pixels, Point,
    SharedString, Window,
};
use guise::prelude::*;

use super::structedit::{EditOp, StructEditEvent, StructEditModal};
use crate::bridge;
use crate::state::{AppState, WorkspaceState};
use model::TableInfo;

pub struct Sidebar {
    app: AppState,
    state: WorkspaceState,
    search: Entity<TextInput>,
    /// The per-table "Generate SQL" context menu, rebuilt on each right-click.
    menu: Option<Entity<ContextMenu>>,
    /// The create-table modal.
    edit: Option<Entity<StructEditModal>>,
    /// A table pending drop-confirmation.
    confirm_drop: Option<String>,
}

impl Sidebar {
    pub fn new(app: AppState, state: WorkspaceState, cx: &mut Context<Self>) -> Self {
        watch(cx, &state.tables);
        watch(cx, &state.tables_loading);
        watch(cx, &state.tables_error);
        watch(cx, &state.active_table);

        let search =
            cx.new(|cx| TextInput::new(cx).placeholder("Filter tables…").size(Size::Xs));
        // Re-render as the filter text changes.
        cx.subscribe(&search, |_this, _input, event: &TextInputEvent, cx| {
            if let TextInputEvent::Change(_) = event {
                cx.notify();
            }
        })
        .detach();

        Sidebar { app, state, search, menu: None, edit: None, confirm_drop: None }
    }

    // --- create / drop table -------------------------------------------------

    pub(super) fn open_create_table(&mut self, cx: &mut Context<Self>) {
        let modal = cx.new(StructEditModal::create_table);
        cx.subscribe(&modal, |this, _m, event: &StructEditEvent, cx| match event {
            StructEditEvent::Cancel => {
                this.edit = None;
                cx.notify();
            }
            StructEditEvent::Submit(EditOp::CreateTable { name, columns }) => {
                this.run_create_table(name.clone(), columns.clone(), cx);
            }
            // The create-table modal only ever emits CreateTable.
            StructEditEvent::Submit(_) => {}
        })
        .detach();
        self.edit = Some(modal);
        cx.notify();
    }

    fn run_create_table(&mut self, name: String, columns: Vec<model::NewColumn>, cx: &mut Context<Self>) {
        self.edit = None;
        cx.notify();
        let host = self.app.host.clone();
        let toasts = self.app.toasts.clone();
        let state = self.state.clone();
        bridge::run(
            cx,
            async move { host.create_table(&name, &columns).await },
            move |result, cx| match result {
                Ok(_) => {
                    toasts.success(cx, "Table created", 1500);
                    state.bump_tables(cx);
                }
                Err(e) => toasts.error(cx, "Create failed", &e),
            },
        );
    }

    fn confirm_drop_table(&mut self, cx: &mut Context<Self>) {
        let Some(table) = self.confirm_drop.take() else {
            return;
        };
        let host = self.app.host.clone();
        let toasts = self.app.toasts.clone();
        let state = self.state.clone();
        let was_active = self.state.active_table.get(cx).as_deref() == Some(table.as_str());
        bridge::run(
            cx,
            async move { host.drop_table(&table).await },
            move |result, cx| match result {
                Ok(_) => {
                    toasts.success(cx, "Table dropped", 1500);
                    if was_active {
                        state.active_table.set(cx, None);
                    }
                    state.bump_tables(cx);
                }
                Err(e) => toasts.error(cx, "Drop failed", &e),
            },
        );
        cx.notify();
    }

    /// Build and show the "Generate SQL" menu for `table` at the cursor.
    fn open_table_menu(
        &mut self,
        table: &str,
        pos: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let host = self.app.host.clone();
        let toasts = self.app.toasts.clone();
        let t = table.to_string();
        let this = cx.entity();

        // Copy `sql` to the clipboard and toast; shared by every item.
        let copy = move |label: &'static str, sql: String, cx: &mut gpui::App| {
            cx.write_to_clipboard(ClipboardItem::new_string(sql));
            AppState::get(cx).toasts.success(cx, &format!("{label} copied"), 1200);
        };

        let menu = cx.new(|cx| {
            let m = ContextMenu::new(cx).width(220.0).section(t.clone());
            let m = m.item("Copy SELECT", {
                let (host, t, copy) = (host.clone(), t.clone(), copy);
                move |_w, cx| match host.generate_select(&t) {
                    Ok(sql) => copy("SELECT", sql, cx),
                    Err(e) => AppState::get(cx).toasts.error(cx, "Failed", &e),
                }
            });
            let m = m.item("Copy INSERT", {
                let (host, t, copy, toasts) =
                    (host.clone(), t.clone(), copy, toasts.clone());
                move |_w, cx| {
                    let (host, t, copy, toasts) =
                        (host.clone(), t.clone(), copy, toasts.clone());
                    bridge::run(
                        cx,
                        async move { host.generate_insert_template(&t).await },
                        move |result, cx| match result {
                            Ok(sql) => copy("INSERT", sql, cx),
                            Err(e) => toasts.error(cx, "Failed", &e),
                        },
                    );
                }
            });
            let m = m.item("Copy CREATE", {
                let (host, t, copy, toasts) =
                    (host.clone(), t.clone(), copy, toasts.clone());
                move |_w, cx| {
                    let (host, t, copy, toasts) =
                        (host.clone(), t.clone(), copy, toasts.clone());
                    bridge::run(
                        cx,
                        async move { host.table_ddl(&t).await },
                        move |result, cx| match result {
                            Ok(sql) => copy("CREATE", sql, cx),
                            Err(e) => toasts.error(cx, "Failed", &e),
                        },
                    );
                }
            });
            let m = m.item("Copy DROP", {
                let (host, t, copy) = (host.clone(), t.clone(), copy);
                move |_w, cx| match host.generate_drop(&t) {
                    Ok(sql) => copy("DROP", sql, cx),
                    Err(e) => AppState::get(cx).toasts.error(cx, "Failed", &e),
                }
            });
            m.divider().danger_item("Drop Table…", {
                let (this, t) = (this.clone(), t.clone());
                move |_w, cx| {
                    this.update(cx, |s, cx| {
                        s.confirm_drop = Some(t.clone());
                        cx.notify();
                    });
                }
            })
        });
        menu.update(cx, |m, cx| m.show(pos, window, cx));
        self.menu = Some(menu);
        cx.notify();
    }

    /// Render one object group ("Tables" / "Views") with a count header and a
    /// `NavLink` per object. Returns `None` when the group is empty.
    fn group(
        &self,
        label: &str,
        objects: &[&TableInfo],
        active: Option<&str>,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement> {
        if objects.is_empty() {
            return None;
        }
        let section = Stack::new().gap(Size::Xs).child(
            div()
                .px(px(6.0))
                .child(Text::new(format!("{label} · {}", objects.len())).size(Size::Xs).dimmed()),
        );
        let mut list = Stack::new().gap(Size::Xs);
        for table in objects {
            let name = table.name.clone();
            let is_active = active == Some(name.as_str());
            let icon = if table.kind == "view" { "◇" } else { "▤" };
            let for_click = name.clone();
            let for_menu = name.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("tblrow-{name}")))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                            this.open_table_menu(&for_menu, ev.position, window, cx);
                        }),
                    )
                    .child(
                        NavLink::new(SharedString::from(format!("tbl-{name}")), name.clone())
                            .icon(icon)
                            .active(is_active)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.state.select_table(cx, &for_click);
                            })),
                    ),
            );
        }
        Some(section.child(list))
    }
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);

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

        // A failed load is distinct from a genuinely empty schema.
        if self.state.tables.read(cx).is_empty() {
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

        let tables = self.state.tables.read(cx);
        let active = self.state.active_table.read(cx).clone();
        let query = self.search.read(cx).text().trim().to_lowercase();

        // Filter, then split into base tables and views.
        let matched: Vec<&TableInfo> = tables
            .iter()
            .filter(|t| query.is_empty() || t.name.to_lowercase().contains(&query))
            .collect();
        let base: Vec<&TableInfo> = matched.iter().copied().filter(|t| t.kind != "view").collect();
        let views: Vec<&TableInfo> = matched.iter().copied().filter(|t| t.kind == "view").collect();

        let body: gpui::AnyElement = if matched.is_empty() {
            div()
                .p(px(6.0))
                .child(Text::new("No matches").size(Size::Xs).dimmed())
                .into_any_element()
        } else {
            let mut list = Stack::new().gap(Size::Sm);
            if let Some(g) = self.group("TABLES", &base, active.as_deref(), cx) {
                list = list.child(g);
            }
            if let Some(g) = self.group("VIEWS", &views, active.as_deref(), cx) {
                list = list.child(g);
            }
            list.into_any_element()
        };

        let mut root = div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                // Sticky filter header above the scrolling list.
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .px(px(6.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(colors.border)
                    .child(div().flex_1().min_w(px(0.0)).child(self.search.clone()))
                    .child(
                        ActionIcon::new("sb-new-table", "＋")
                            .size(Size::Sm)
                            .variant(Variant::Subtle)
                            .on_click(cx.listener(|this, _, _, cx| this.open_create_table(cx))),
                    ),
            )
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .p(px(6.0))
                    .child(body),
            );
        if let Some(menu) = &self.menu {
            root = root.child(menu.clone());
        }
        if let Some(edit) = &self.edit {
            root = root.child(edit.clone());
        }
        if let Some(table) = &self.confirm_drop {
            root = root.child(
                ConfirmModal::new()
                    .title("Drop Table")
                    .message(format!("Drop table \"{table}\"? This cannot be undone."))
                    .confirm_label("Drop")
                    .cancel_label("Cancel")
                    .danger()
                    .on_confirm(cx.listener(|this, _, _, cx| this.confirm_drop_table(cx)))
                    .on_cancel(cx.listener(|this, _, _, cx| {
                        this.confirm_drop = None;
                        cx.notify();
                    })),
            );
        }
        root.into_any_element()
    }
}
