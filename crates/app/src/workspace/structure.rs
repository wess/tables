//! The Structure tab: a sub-tabbed view of the active table's columns, indexes,
//! foreign keys, DDL, and per-column profile. Columns and indexes are editable
//! (add/rename/drop, create index) through the structure-edit modal; the rest
//! are read-only. Structure is fetched when the table changes; DDL and Profile
//! are fetched lazily when first viewed.

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, SharedString, Window};
use guise::prelude::*;

use super::structedit::{EditOp, StructEditEvent, StructEditModal};
use crate::bridge;
use crate::state::{AppState, WorkspaceState};
use model::{ColumnProfile, TableStructure};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Columns,
    Indexes,
    ForeignKeys,
    Ddl,
    Profile,
}

/// A pending drop awaiting confirmation.
enum DropTarget {
    Column(String),
    Index(String),
}

pub struct StructurePanel {
    app: AppState,
    state: WorkspaceState,
    structure: Signal<Option<TableStructure>>,
    ddl: Signal<Option<String>>,
    profile: Signal<Option<Vec<ColumnProfile>>>,
    tab: Signal<Tab>,
    edit: Option<Entity<StructEditModal>>,
    confirm: Option<DropTarget>,
}

impl StructurePanel {
    pub fn new(app: AppState, state: WorkspaceState, cx: &mut Context<Self>) -> Self {
        let structure = Signal::new(cx, None);
        let ddl = Signal::new(cx, None);
        let profile = Signal::new(cx, None);
        let tab = Signal::new(cx, Tab::Columns);
        watch(cx, &structure);
        watch(cx, &ddl);
        watch(cx, &profile);
        watch(cx, &tab);

        let effect_app = app.clone();
        let effect_structure = structure.clone();
        let effect_ddl = ddl.clone();
        let effect_profile = profile.clone();
        let effect_active = state.active_table.clone();
        use_effect(cx, &state.active_table, move |table, cx| {
            effect_ddl.set(cx, None);
            effect_profile.set(cx, None);
            let Some(table) = table.clone() else {
                effect_structure.set(cx, None);
                return;
            };
            // Ignore a completion for a table that is no longer selected.
            fetch_structure(&effect_app, &effect_structure, Some(&effect_active), table, cx);
        });

        StructurePanel {
            app,
            state,
            structure,
            ddl,
            profile,
            tab,
            edit: None,
            confirm: None,
        }
    }

    fn select_tab(&self, tab: Tab, cx: &mut gpui::App) {
        self.tab.set(cx, tab);
        match tab {
            Tab::Ddl if self.ddl.read(cx).is_none() => self.fetch_ddl(cx),
            Tab::Profile if self.profile.read(cx).is_none() => self.fetch_profile(cx),
            _ => {}
        }
    }

    // --- edit entry points ---------------------------------------------------

    fn open_edit(&mut self, modal: Entity<StructEditModal>, cx: &mut Context<Self>) {
        cx.subscribe(&modal, |this, _m, event: &StructEditEvent, cx| match event {
            StructEditEvent::Cancel => {
                this.edit = None;
                cx.notify();
            }
            StructEditEvent::Submit(op) => this.run_op(op, cx),
        })
        .detach();
        self.edit = Some(modal);
        cx.notify();
    }

    fn open_add_column(&mut self, cx: &mut Context<Self>) {
        let Some(table) = self.state.active_table.get(cx) else {
            return;
        };
        let modal = cx.new(|cx| StructEditModal::add_column(table, cx));
        self.open_edit(modal, cx);
    }

    fn open_rename_column(&mut self, column: String, cx: &mut Context<Self>) {
        let Some(table) = self.state.active_table.get(cx) else {
            return;
        };
        let modal = cx.new(|cx| StructEditModal::rename_column(table, column, cx));
        self.open_edit(modal, cx);
    }

    fn open_create_index(&mut self, cx: &mut Context<Self>) {
        let Some(table) = self.state.active_table.get(cx) else {
            return;
        };
        let columns = self
            .structure
            .read(cx)
            .as_ref()
            .map(|s| s.columns.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default();
        let modal = cx.new(|cx| StructEditModal::create_index(table, columns, cx));
        self.open_edit(modal, cx);
    }

    // `op` is expressed against the active table; the modal already scoped it.
    fn run_op(&mut self, op: &EditOp, cx: &mut Context<Self>) {
        self.edit = None;
        cx.notify();
        let host = self.app.host.clone();
        let toasts = self.app.toasts.clone();
        let state = self.state.clone();
        let app = self.app.clone();
        let structure = self.structure.clone();
        let table = op_table(op);
        let fut = op_future(host, op);
        bridge::run(cx, fut, move |result, cx| match result {
            Ok(_) => {
                toasts.success(cx, "Structure updated", 1500);
                state.bump_tables(cx);
                fetch_structure(&app, &structure, None, table, cx);
            }
            Err(e) => toasts.error(cx, "Change failed", &e),
        });
    }

    fn request_drop(&mut self, target: DropTarget, cx: &mut Context<Self>) {
        self.confirm = Some(target);
        cx.notify();
    }

    fn confirm_drop(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.confirm.take() else {
            return;
        };
        let Some(table) = self.state.active_table.get(cx) else {
            return;
        };
        let host = self.app.host.clone();
        let toasts = self.app.toasts.clone();
        let state = self.state.clone();
        let app = self.app.clone();
        let structure = self.structure.clone();
        let refetch = table.clone();
        let fut = async move {
            match target {
                DropTarget::Column(c) => host.drop_column(&table, &c).await,
                DropTarget::Index(n) => host.drop_index(&table, &n).await,
            }
        };
        bridge::run(cx, fut, move |result, cx| match result {
            Ok(_) => {
                toasts.success(cx, "Dropped", 1500);
                state.bump_tables(cx);
                fetch_structure(&app, &structure, None, refetch, cx);
            }
            Err(e) => toasts.error(cx, "Drop failed", &e),
        });
        cx.notify();
    }

    fn fetch_ddl(&self, cx: &mut gpui::App) {
        let Some(table) = self.state.active_table.get(cx) else {
            return;
        };
        let host = self.app.host.clone();
        let out = self.ddl.clone();
        let toasts = self.app.toasts.clone();
        bridge::run(
            cx,
            async move { host.table_ddl(&table).await },
            move |result, cx| match result {
                Ok(ddl) => out.set(cx, Some(ddl)),
                Err(error) => toasts.error(cx, "DDL failed", &error),
            },
        );
    }

    fn fetch_profile(&self, cx: &mut gpui::App) {
        let Some(table) = self.state.active_table.get(cx) else {
            return;
        };
        let host = self.app.host.clone();
        let out = self.profile.clone();
        let toasts = self.app.toasts.clone();
        bridge::run(
            cx,
            async move { host.profile_table(&table).await },
            move |result, cx| match result {
                Ok(profile) => out.set(cx, Some(profile)),
                Err(error) => toasts.error(cx, "Profile failed", &error),
            },
        );
    }

    // --- editable views ------------------------------------------------------

    fn columns_view(&self, structure: &TableStructure, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = crate::theme::palette(cx);
        let header = div()
            .flex()
            .items_center()
            .px(px(6.0))
            .py(px(4.0))
            .border_b_1()
            .border_color(colors.border)
            .child(hcell("Name"))
            .child(hcell("Type"))
            .child(hcell("Null"))
            .child(hcell("Default"))
            .child(hcell("Key"))
            .child(div().w(px(84.0)));
        let mut list = div().flex().flex_col();
        for col in &structure.columns {
            let name_rename = col.name.clone();
            let name_drop = col.name.clone();
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .px(px(6.0))
                    .py(px(2.0))
                    .border_b_1()
                    .border_color(colors.border_subtle)
                    .child(cell(col.name.clone()))
                    .child(cell(col.data_type.clone()))
                    .child(cell(if col.nullable { "YES" } else { "NO" }.to_string()))
                    .child(cell(col.default_value.clone().unwrap_or_default()))
                    .child(cell(if col.is_primary_key { "PK" } else { "" }.to_string()))
                    .child(
                        div()
                            .w(px(84.0))
                            .flex()
                            .gap(px(2.0))
                            .justify_end()
                            .child(
                                ActionIcon::new(
                                    SharedString::from(format!("ren-{}", col.name)),
                                    "✎",
                                )
                                .size(Size::Xs)
                                .variant(Variant::Subtle)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_rename_column(name_rename.clone(), cx)
                                })),
                            )
                            .child(
                                ActionIcon::new(
                                    SharedString::from(format!("dropcol-{}", col.name)),
                                    "🗑",
                                )
                                .size(Size::Xs)
                                .variant(Variant::Subtle)
                                .color(ColorName::Red)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.request_drop(DropTarget::Column(name_drop.clone()), cx)
                                })),
                            ),
                    ),
            );
        }
        div().child(header).child(list).into_any_element()
    }

    fn indexes_view(&self, structure: &TableStructure, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = crate::theme::palette(cx);
        if structure.indexes.is_empty() {
            return Text::new("No indexes").size(Size::Sm).dimmed().into_any_element();
        }
        let header = div()
            .flex()
            .items_center()
            .px(px(6.0))
            .py(px(4.0))
            .border_b_1()
            .border_color(colors.border)
            .child(hcell("Name"))
            .child(hcell("Columns"))
            .child(hcell("Type"))
            .child(hcell("Unique"))
            .child(div().w(px(44.0)));
        let mut list = div().flex().flex_col();
        for idx in &structure.indexes {
            let name_drop = idx.name.clone();
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .px(px(6.0))
                    .py(px(2.0))
                    .border_b_1()
                    .border_color(colors.border_subtle)
                    .child(cell(idx.name.clone()))
                    .child(cell(idx.columns.join(", ")))
                    .child(cell(idx.kind.clone()))
                    .child(cell(if idx.unique { "YES" } else { "NO" }.to_string()))
                    .child(
                        div().w(px(44.0)).flex().justify_end().child(
                            ActionIcon::new(
                                SharedString::from(format!("dropidx-{}", idx.name)),
                                "🗑",
                            )
                            .size(Size::Xs)
                            .variant(Variant::Subtle)
                            .color(ColorName::Red)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.request_drop(DropTarget::Index(name_drop.clone()), cx)
                            })),
                        ),
                    ),
            );
        }
        div().child(header).child(list).into_any_element()
    }
}

/// Fetch the structure for `table` into `structure`. When `owner` is given, a
/// stale completion (the active table changed meanwhile) is dropped.
fn fetch_structure(
    app: &AppState,
    structure: &Signal<Option<TableStructure>>,
    owner: Option<&Signal<Option<String>>>,
    table: String,
    cx: &mut gpui::App,
) {
    let host = app.host.clone();
    let out = structure.clone();
    let toasts = app.toasts.clone();
    let owner = owner.cloned();
    let want = table.clone();
    bridge::run(
        cx,
        async move { host.table_structure(&table).await },
        move |result, cx| {
            if let Some(owner) = &owner {
                if owner.get(cx).as_deref() != Some(want.as_str()) {
                    return;
                }
            }
            match result {
                Ok(structure) => out.set(cx, Some(structure)),
                Err(error) => {
                    out.set(cx, None);
                    toasts.error(cx, "Structure failed", &error);
                }
            }
        },
    );
}

/// The table an op targets (for the post-change structure refetch).
fn op_table(op: &EditOp) -> String {
    match op {
        EditOp::AddColumn { table, .. }
        | EditOp::RenameColumn { table, .. }
        | EditOp::CreateIndex { table, .. } => table.clone(),
        EditOp::CreateTable { name, .. } => name.clone(),
    }
}

/// The host call for an op, cloned out so the borrow of `op` doesn't cross the
/// await point.
fn op_future(
    host: std::sync::Arc<host::Host>,
    op: &EditOp,
) -> impl std::future::Future<Output = Result<(), String>> {
    let op = clone_op(op);
    async move {
        match op {
            EditOp::AddColumn { table, column } => {
                host.add_column(
                    &table,
                    &column.name,
                    &column.data_type,
                    column.nullable,
                    column.default_value.as_deref(),
                )
                .await
            }
            EditOp::RenameColumn { table, from, to } => host.rename_column(&table, &from, &to).await,
            EditOp::CreateIndex { table, name, columns, unique } => {
                host.create_index(&table, &name, &columns, unique).await
            }
            EditOp::CreateTable { name, columns } => host.create_table(&name, &columns).await,
        }
    }
}

fn clone_op(op: &EditOp) -> EditOp {
    match op {
        EditOp::AddColumn { table, column } => {
            EditOp::AddColumn { table: table.clone(), column: column.clone() }
        }
        EditOp::RenameColumn { table, from, to } => {
            EditOp::RenameColumn { table: table.clone(), from: from.clone(), to: to.clone() }
        }
        EditOp::CreateIndex { table, name, columns, unique } => EditOp::CreateIndex {
            table: table.clone(),
            name: name.clone(),
            columns: columns.clone(),
            unique: *unique,
        },
        EditOp::CreateTable { name, columns } => {
            EditOp::CreateTable { name: name.clone(), columns: columns.clone() }
        }
    }
}

fn hcell(label: &str) -> impl IntoElement {
    div().flex_1().min_w(px(0.0)).px(px(6.0)).child(Text::new(label.to_string()).size(Size::Xs).dimmed())
}

fn cell(text: String) -> impl IntoElement {
    div()
        .flex_1()
        .min_w(px(0.0))
        .px(px(6.0))
        .py(px(3.0))
        .text_size(px(12.0))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(SharedString::from(text))
}

fn table_tab(
    id: &'static str,
    label: &'static str,
    this_tab: Tab,
    active: Tab,
    cx: &mut Context<StructurePanel>,
) -> impl IntoElement {
    Button::new(id, label)
        .size(Size::Xs)
        .variant(if active == this_tab { Variant::Light } else { Variant::Subtle })
        .on_click(cx.listener(move |this, _, _, cx| this.select_tab(this_tab, cx)))
}

impl Render for StructurePanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        if self.state.active_table.read(cx).is_none() {
            return div()
                .flex()
                .size_full()
                .child(Center::new().child(Text::new("Select a table").size(Size::Sm).dimmed()))
                .into_any_element();
        }

        let active = *self.tab.read(cx);
        let structure = self.structure.get(cx);
        let col_count = structure.as_ref().map(|s| s.columns.len()).unwrap_or(0);
        let idx_count = structure.as_ref().map(|s| s.indexes.len()).unwrap_or(0);
        let fk_count = structure.as_ref().map(|s| s.foreign_keys.len()).unwrap_or(0);

        // Contextual edit action for the active sub-tab.
        let action = match active {
            Tab::Columns => Some(
                Button::new("st-add-col", "＋ Column")
                    .size(Size::Xs)
                    .variant(Variant::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| this.open_add_column(cx))),
            ),
            Tab::Indexes => Some(
                Button::new("st-add-idx", "＋ Index")
                    .size(Size::Xs)
                    .variant(Variant::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| this.open_create_index(cx))),
            ),
            _ => None,
        };

        let tabbar = div()
            .flex()
            .items_center()
            .gap_1()
            .px(px(8.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(colors.border)
            .child(table_tab("st-cols", "Columns", Tab::Columns, active, cx))
            .child(table_tab("st-idx", "Indexes", Tab::Indexes, active, cx))
            .child(table_tab("st-fk", "Foreign Keys", Tab::ForeignKeys, active, cx))
            .child(table_tab("st-ddl", "DDL", Tab::Ddl, active, cx))
            .child(table_tab("st-prof", "Profile", Tab::Profile, active, cx))
            .child(
                Text::new(format!("{col_count} cols · {idx_count} idx · {fk_count} fk"))
                    .size(Size::Xs)
                    .dimmed(),
            )
            .child(div().flex_1())
            .children(action);

        let content = match active {
            Tab::Columns | Tab::Indexes | Tab::ForeignKeys => {
                let Some(structure) = structure else {
                    return loading(tabbar);
                };
                match active {
                    Tab::Columns => self.columns_view(&structure, cx),
                    Tab::Indexes => self.indexes_view(&structure, cx),
                    _ => foreign_keys_table(&structure),
                }
            }
            Tab::Ddl => match self.ddl.get(cx) {
                Some(ddl) => div()
                    .p(px(12.0))
                    .font_family(crate::theme::MONO_FAMILY)
                    .text_size(px(12.0))
                    .child(gpui::SharedString::from(if ddl.is_empty() {
                        "-- no DDL available".to_string()
                    } else {
                        ddl
                    }))
                    .into_any_element(),
                None => center_loader(),
            },
            Tab::Profile => match self.profile.get(cx) {
                Some(profile) => profile_table(&profile),
                None => center_loader(),
            },
        };

        let mut root = div()
            .flex()
            .flex_col()
            .size_full()
            .child(tabbar)
            .child(
                div()
                    .id("structure-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .overflow_x_scroll()
                    .p(px(12.0))
                    .child(content),
            );

        if let Some(edit) = &self.edit {
            root = root.child(edit.clone());
        }
        if let Some(target) = &self.confirm {
            let what = match target {
                DropTarget::Column(c) => format!("column \"{c}\""),
                DropTarget::Index(n) => format!("index \"{n}\""),
            };
            root = root.child(
                ConfirmModal::new()
                    .title("Drop")
                    .message(format!("Drop {what}? This cannot be undone."))
                    .confirm_label("Drop")
                    .cancel_label("Cancel")
                    .danger()
                    .on_confirm(cx.listener(|this, _, _, cx| this.confirm_drop(cx)))
                    .on_cancel(cx.listener(|this, _, _, cx| {
                        this.confirm = None;
                        cx.notify();
                    })),
            );
        }

        root.into_any_element()
    }
}

fn loading(tabbar: impl IntoElement) -> gpui::AnyElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .child(tabbar)
        .child(center_loader())
        .into_any_element()
}

fn center_loader() -> gpui::AnyElement {
    div()
        .flex()
        .flex_1()
        .child(Center::new().child(Loader::new().size(Size::Sm)))
        .into_any_element()
}

fn foreign_keys_table(structure: &TableStructure) -> gpui::AnyElement {
    if structure.foreign_keys.is_empty() {
        return Text::new("No foreign keys").size(Size::Sm).dimmed().into_any_element();
    }
    let mut table = Table::new()
        .with_border(true)
        .striped(true)
        .head(["Name", "Columns", "References", "On Delete", "On Update"]);
    for fk in &structure.foreign_keys {
        table = table.row([
            fk.name.clone(),
            fk.columns.join(", "),
            format!("{}({})", fk.referenced_table, fk.referenced_columns.join(", ")),
            fk.on_delete.clone(),
            fk.on_update.clone(),
        ]);
    }
    table.into_any_element()
}

fn profile_table(profile: &[ColumnProfile]) -> gpui::AnyElement {
    if profile.is_empty() {
        return Text::new("No profile data").size(Size::Sm).dimmed().into_any_element();
    }
    let mut table = Table::new()
        .with_border(true)
        .striped(true)
        .head(["Column", "Type", "Nulls %", "Distinct", "Min", "Max", "Avg"]);
    for p in profile {
        table = table.row([
            p.column.clone(),
            p.data_type.clone(),
            format!("{:.1}%", p.null_percent),
            p.distinct_count.to_string(),
            p.min_value.clone().unwrap_or_default(),
            p.max_value.clone().unwrap_or_default(),
            p.avg_value.clone().unwrap_or_default(),
        ]);
    }
    table.into_any_element()
}
