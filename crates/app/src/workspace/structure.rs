//! The Structure tab: a sub-tabbed view of the active table's columns, indexes,
//! foreign keys, DDL, and per-column profile. Structure is fetched when the
//! table changes; DDL and Profile are fetched lazily when first viewed.

use gpui::prelude::*;
use gpui::{div, px, Context, Window};
use guise::prelude::*;

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

pub struct StructurePanel {
    app: AppState,
    state: WorkspaceState,
    structure: Signal<Option<TableStructure>>,
    ddl: Signal<Option<String>>,
    profile: Signal<Option<Vec<ColumnProfile>>>,
    tab: Signal<Tab>,
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
            let host = effect_app.host.clone();
            let out = effect_structure.clone();
            let toasts = effect_app.toasts.clone();
            // Ignore a completion for a table that is no longer selected.
            let owner = effect_active.clone();
            let want = table.clone();
            bridge::run(
                cx,
                async move { host.table_structure(&table).await },
                move |result, cx| {
                    if owner.get(cx).as_deref() != Some(want.as_str()) {
                        return;
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
        });

        StructurePanel { app, state, structure, ddl, profile, tab }
    }

    fn select_tab(&self, tab: Tab, cx: &mut gpui::App) {
        self.tab.set(cx, tab);
        match tab {
            Tab::Ddl if self.ddl.read(cx).is_none() => self.fetch_ddl(cx),
            Tab::Profile if self.profile.read(cx).is_none() => self.fetch_profile(cx),
            _ => {}
        }
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
            );

        let content = match active {
            Tab::Columns | Tab::Indexes | Tab::ForeignKeys => {
                let Some(structure) = structure else {
                    return loading(colors, tabbar);
                };
                match active {
                    Tab::Columns => columns_table(&structure),
                    Tab::Indexes => indexes_table(&structure),
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

        div()
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
            )
            .into_any_element()
    }
}

fn loading(colors: crate::theme::Palette, tabbar: impl IntoElement) -> gpui::AnyElement {
    let _ = colors;
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

fn columns_table(structure: &TableStructure) -> gpui::AnyElement {
    let mut table = Table::new()
        .with_border(true)
        .striped(true)
        .head(["Name", "Type", "Nullable", "Default", "Key", "Comment"]);
    for col in &structure.columns {
        table = table.row([
            col.name.clone(),
            col.data_type.clone(),
            if col.nullable { "YES" } else { "NO" }.to_string(),
            col.default_value.clone().unwrap_or_default(),
            if col.is_primary_key { "PK" } else { "" }.to_string(),
            col.comment.clone().unwrap_or_default(),
        ]);
    }
    table.into_any_element()
}

fn indexes_table(structure: &TableStructure) -> gpui::AnyElement {
    if structure.indexes.is_empty() {
        return Text::new("No indexes").size(Size::Sm).dimmed().into_any_element();
    }
    let mut table = Table::new()
        .with_border(true)
        .striped(true)
        .head(["Name", "Columns", "Type", "Unique"]);
    for idx in &structure.indexes {
        table = table.row([
            idx.name.clone(),
            idx.columns.join(", "),
            idx.kind.clone(),
            if idx.unique { "YES" } else { "NO" }.to_string(),
        ]);
    }
    table.into_any_element()
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
