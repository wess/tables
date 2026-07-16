//! The filter panel above the data grid. Rows of (column, operator, value)
//! build an applied filter set that feeds the rows request. Editing is a draft
//! held in local widget entities; Apply copies it into `applied_filters`.

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, Window};
use guise::prelude::*;

use crate::state::{AppState, FilterState, WorkspaceState};
use model::FilterCondition;

/// (label, operator value) pairs — the 14 operators the filter clause supports.
const OPS: [(&str, &str); 14] = [
    ("=", "="),
    ("≠", "!="),
    ("contains", "contains"),
    ("not contains", "not_contains"),
    ("starts with", "starts_with"),
    ("ends with", "ends_with"),
    (">", ">"),
    ("<", "<"),
    ("≥", ">="),
    ("≤", "<="),
    ("is null", "is_null"),
    ("is not null", "is_not_null"),
    ("in", "in"),
    ("between", "between"),
];

struct FilterRow {
    id: String,
    column: Entity<Select>,
    operator: Entity<Select>,
    value: Entity<TextInput>,
}

pub struct FilterPanel {
    #[allow(dead_code)]
    app: AppState,
    state: WorkspaceState,
    rows: Vec<FilterRow>,
    logic_or: bool,
    built_table: Option<String>,
}

impl FilterPanel {
    pub fn new(app: AppState, state: WorkspaceState, cx: &mut Context<Self>) -> Self {
        watch(cx, &state.active_table);
        watch(cx, &state.rows);
        FilterPanel { app, state, rows: Vec::new(), logic_or: false, built_table: None }
    }

    fn columns(&self, cx: &gpui::App) -> Vec<String> {
        self.state
            .rows
            .read(cx)
            .as_ref()
            .map(|r| r.columns.clone())
            .unwrap_or_default()
    }

    /// Rebuild the draft rows from the applied filters when the table changes.
    fn ensure_synced(&mut self, cx: &mut Context<Self>) {
        let table = self.state.active_table.get(cx);
        if self.built_table != table {
            self.built_table = table;
            let applied = self.state.applied_filters.get(cx);
            self.logic_or = applied.logic == "or";
            let seeds = applied.conditions.clone();
            self.rows = seeds.iter().map(|c| self.make_row(Some(c), cx)).collect();
        }
    }

    fn make_row(&self, seed: Option<&FilterCondition>, cx: &mut Context<Self>) -> FilterRow {
        let columns = self.columns(cx);
        let col_idx = seed
            .and_then(|s| columns.iter().position(|c| c == &s.column))
            .unwrap_or(0);
        let op_idx = seed
            .and_then(|s| OPS.iter().position(|(_, v)| *v == s.operator))
            .unwrap_or(0);
        let op_labels: Vec<&str> = OPS.iter().map(|(l, _)| *l).collect();
        let value_text = seed.map(|s| s.value.clone()).unwrap_or_default();

        let column = cx.new(move |cx| Select::new(cx).data(columns).selected(col_idx).size(Size::Xs));
        let operator = cx.new(move |cx| Select::new(cx).data(op_labels).selected(op_idx).size(Size::Xs));
        let value = cx.new(move |cx| TextInput::new(cx).value(&value_text).placeholder("value").size(Size::Xs));
        FilterRow { id: model::new_uuid(), column, operator, value }
    }

    fn add_row(&mut self, cx: &mut Context<Self>) {
        let row = self.make_row(None, cx);
        self.rows.push(row);
        cx.notify();
    }

    fn remove_row(&mut self, id: &str, cx: &mut Context<Self>) {
        self.rows.retain(|r| r.id != id);
        cx.notify();
    }

    fn apply(&self, cx: &mut gpui::App) {
        let columns = self.columns(cx);
        let conditions: Vec<FilterCondition> = self
            .rows
            .iter()
            .filter_map(|row| {
                let col_idx = row.column.read(cx).selected_index().unwrap_or(0);
                let column = columns.get(col_idx).cloned().unwrap_or_default();
                if column.is_empty() {
                    return None;
                }
                let op_idx = row.operator.read(cx).selected_index().unwrap_or(0);
                let operator = OPS[op_idx].1.to_string();
                let value = row.value.read(cx).text();
                Some(FilterCondition { id: row.id.clone(), column, operator, value, value2: None })
            })
            .collect();
        let logic = if self.logic_or { "or" } else { "and" }.to_string();
        self.state.applied_filters.set(cx, FilterState { conditions, logic });
        self.state.page.set(cx, 1);
        self.state.bump_rows(cx);
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        self.rows.clear();
        self.state.applied_filters.set(cx, FilterState::default());
        self.state.page.set(cx, 1);
        self.state.bump_rows(cx);
        cx.notify();
    }
}

impl Render for FilterPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.ensure_synced(cx);
        let colors = crate::theme::palette(cx);

        let mut rows_col = Stack::new().gap(Size::Xs);
        for row in &self.rows {
            let id = row.id.clone();
            rows_col = rows_col.child(
                Group::new()
                    .gap(Size::Xs)
                    .align(Align::Center)
                    .child(div().w(px(160.0)).child(row.column.clone()))
                    .child(div().w(px(120.0)).child(row.operator.clone()))
                    .child(div().flex_1().child(row.value.clone()))
                    .child(
                        ActionIcon::new(gpui::SharedString::from(format!("fr-{}", row.id)), "✕")
                            .variant(Variant::Subtle)
                            .color(ColorName::Red)
                            .size(Size::Xs)
                            .on_click(cx.listener(move |this, _, _, cx| this.remove_row(&id, cx))),
                    ),
            );
        }

        let mut controls = Group::new()
            .gap(Size::Xs)
            .align(Align::Center)
            .child(
                Button::new("filter-add", "+ Add filter")
                    .size(Size::Xs)
                    .variant(Variant::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| this.add_row(cx))),
            );

        if self.rows.len() > 1 {
            let logic_or = self.logic_or;
            controls = controls
                .child(
                    Button::new("filter-and", "AND")
                        .size(Size::Xs)
                        .variant(if logic_or { Variant::Subtle } else { Variant::Light })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.logic_or = false;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("filter-or", "OR")
                        .size(Size::Xs)
                        .variant(if logic_or { Variant::Light } else { Variant::Subtle })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.logic_or = true;
                            cx.notify();
                        })),
                );
        }

        controls = controls
            .child(Divider::vertical())
            .child(
                Button::new("filter-apply", "Apply")
                    .size(Size::Xs)
                    .on_click(cx.listener(|this, _, _, cx| this.apply(cx))),
            )
            .child(
                Button::new("filter-clear", "Clear")
                    .size(Size::Xs)
                    .variant(Variant::Subtle)
                    .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
            );

        div()
            .flex()
            .flex_col()
            .gap_2()
            .px(px(8.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.bg_subtle)
            .child(rows_col)
            .child(controls)
    }
}
