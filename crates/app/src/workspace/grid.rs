//! The data grid. A custom grid — not guise's static `Table` — so cells can be
//! selected, sorted, and edited inline. Editing a cell queues a
//! `PendingChange::Update`; the commit flow lives in the Data panel.

use std::collections::HashMap;

use gpui::prelude::*;
use gpui::{
    div, px, Context, Entity, Hsla, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    SharedString, Window,
};
use guise::prelude::*;
use serde_json::Value;

use crate::state::{AppState, PendingChange, WorkspaceState};
use crate::workspace::cell_text;
use model::{Row, SortSpec};

const ROW_NUM_W: f32 = 48.0;
const COL_W: f32 = 168.0;
const MIN_COL_W: f32 = 60.0;

pub struct DataGrid {
    app: AppState,
    state: WorkspaceState,
    editing: Option<Editing>,
    /// Per-column pixel widths; a column absent here uses `COL_W`.
    widths: HashMap<String, f32>,
    /// The in-progress column resize, if the user is dragging a header edge.
    resize: Option<Resize>,
}

struct Editing {
    row: usize,
    column: String,
    input: Entity<TextInput>,
}

#[derive(Clone)]
struct Resize {
    column: String,
    start_x: f32,
    start_width: f32,
}

/// How a pending change touches a rendered row.
enum RowMark {
    None,
    Updated,
    Deleted,
}

fn pk_matches(primary_key: &Row, row: &Row) -> bool {
    primary_key.iter().all(|(k, v)| row.get(k) == Some(v))
}

impl DataGrid {
    pub fn new(app: AppState, state: WorkspaceState, cx: &mut Context<Self>) -> Self {
        watch(cx, &state.rows);
        watch(cx, &state.sort);
        watch(cx, &state.selection);
        watch(cx, &state.hidden_columns);
        watch(cx, &state.pending);
        DataGrid { app, state, editing: None, widths: HashMap::new(), resize: None }
    }

    fn col_width(&self, column: &str) -> f32 {
        self.widths.get(column).copied().unwrap_or(COL_W)
    }

    fn visible_columns(&self, cx: &gpui::App) -> Vec<String> {
        let hidden = self.state.hidden_columns.read(cx);
        match self.state.rows.read(cx) {
            Some(response) => response
                .columns
                .iter()
                .filter(|c| !hidden.contains(*c))
                .cloned()
                .collect(),
            None => Vec::new(),
        }
    }

    fn toggle_sort(&self, column: &str, cx: &mut gpui::App) {
        let next = match self.state.sort.get(cx) {
            Some(sort) if sort.column == column => SortSpec {
                column: column.to_string(),
                direction: if sort.direction == "asc" { "desc" } else { "asc" }.to_string(),
            },
            _ => SortSpec { column: column.to_string(), direction: "asc".to_string() },
        };
        self.state.sort.set(cx, Some(next));
        self.state.page.set(cx, 1);
        self.state.bump_rows(cx);
    }

    fn click_row(&self, idx: usize, toggle: bool, range: bool, cx: &mut gpui::App) {
        let current = self.state.selection.get(cx);
        let next = if toggle {
            let mut set = current;
            if !set.remove(&idx) {
                set.insert(idx);
            }
            set
        } else if range && !current.is_empty() {
            let anchor = *current.iter().next().unwrap();
            let (lo, hi) = (anchor.min(idx), anchor.max(idx));
            (lo..=hi).collect()
        } else {
            [idx].into_iter().collect()
        };
        self.state.selection.set(cx, next);
    }

    fn start_edit(
        &mut self,
        row: usize,
        column: String,
        value: Option<&Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let initial = match value {
            None | Some(Value::Null) => String::new(),
            Some(v) => cell_text(Some(v), ""),
        };
        let input = cx.new(|cx| TextInput::new(cx).size(Size::Xs).value(&initial));
        input.read(cx).focus_handle().focus(window);
        cx.subscribe(&input, |this, _input, event: &TextInputEvent, cx| {
            if let TextInputEvent::Submit(text) = event {
                this.commit_edit(text.clone(), cx);
            }
        })
        .detach();
        self.editing = Some(Editing { row, column, input });
        cx.notify();
    }

    fn commit_edit(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(editing) = self.editing.take() {
            // Read the one edited row without cloning the whole page.
            let row = self
                .state
                .rows
                .read(cx)
                .as_ref()
                .and_then(|r| r.rows.get(editing.row).cloned());
            if let Some(row) = row {
                let value = if text.is_empty() { Value::Null } else { Value::String(text) };
                let table = self.state.active_table.get(cx).unwrap_or_default();
                let column = editing.column;
                self.state.pending.update(cx, move |pending| {
                    // Coalesce into an existing update for this row so a
                    // multi-column edit commits as ONE `UPDATE`. Separate
                    // per-column updates each snapshot the whole row into their
                    // WHERE, so the second statement's WHERE would no longer
                    // match after the first ran in the same transaction — the
                    // later edit was silently lost.
                    let merged = pending.iter_mut().any(|change| match change {
                        PendingChange::Update { table: t, primary_key, changes }
                            if *t == table && pk_matches(primary_key, &row) =>
                        {
                            changes.insert(column.clone(), value.clone());
                            true
                        }
                        _ => false,
                    });
                    if !merged {
                        let mut changes = Row::new();
                        changes.insert(column, value);
                        pending.push(PendingChange::Update { table, primary_key: row, changes });
                    }
                });
            }
        }
        cx.notify();
    }

}

/// Classify how the staged changes touch one rendered row. Called once per row
/// per render, but only when there is at least one pending change.
fn row_mark_for(row: &Row, pending: &[PendingChange]) -> (RowMark, Vec<String>) {
    let mut mark = RowMark::None;
    let mut changed = Vec::new();
    for change in pending {
        match change {
            PendingChange::Delete { primary_key, .. } if pk_matches(primary_key, row) => {
                mark = RowMark::Deleted;
            }
            PendingChange::Update { primary_key, changes, .. } if pk_matches(primary_key, row) => {
                if matches!(mark, RowMark::None) {
                    mark = RowMark::Updated;
                }
                changed.extend(changes.keys().cloned());
            }
            _ => {}
        }
    }
    (mark, changed)
}

impl Render for DataGrid {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let null_display = self.app.settings.read(cx).null_display.clone();
        let theme = guise::theme::theme(cx);
        let yellow = theme.color(ColorName::Yellow, 6).hsla();
        let red = theme.color(ColorName::Red, 6).hsla();
        let blue_sel = theme.color(ColorName::Blue, 5).hsla();
        let dimmed = theme.dimmed().hsla();
        let text_color = theme.text().hsla();
        let blue_color = theme.color(ColorName::Blue, 4);
        let columns = self.visible_columns(cx);
        if columns.is_empty() {
            return div()
                .flex()
                .size_full()
                .child(Center::new().child(Text::new("No rows").size(Size::Sm).dimmed()))
                .into_any_element();
        }

        // Borrow the page instead of deep-cloning every row each render.
        let rows_binding = self.state.rows.read(cx);
        let Some(response) = rows_binding.as_ref() else {
            return div()
                .flex()
                .size_full()
                .child(Center::new().child(Text::new("No rows").size(Size::Sm).dimmed()))
                .into_any_element();
        };
        let sort = self.state.sort.get(cx);
        let selection = self.state.selection.get(cx);

        // Precompute per-row change marks once. When nothing is staged (the
        // common case) this is skipped entirely, so the render loop does no
        // pending scan at all.
        let pending = self.state.pending.read(cx);
        let marks: Vec<(RowMark, Vec<String>)> = if pending.is_empty() {
            Vec::new()
        } else {
            response.rows.iter().map(|row| row_mark_for(row, pending)).collect()
        };

        // Column widths and the total content width (drives horizontal scroll).
        let widths: Vec<f32> = columns.iter().map(|c| self.col_width(c)).collect();
        let total_w = ROW_NUM_W + widths.iter().sum::<f32>();

        // Header.
        let mut header = div()
            .flex()
            .flex_none()
            .w(px(total_w))
            .bg(colors.bg_subtle)
            .border_b_1()
            .border_color(colors.border)
            .child(header_cell("#", ROW_NUM_W, colors.text_muted));
        for (column, width) in columns.iter().zip(&widths) {
            let arrow = match &sort {
                Some(s) if &s.column == column => {
                    Some(if s.direction == "asc" { "↑" } else { "↓" })
                }
                _ => None,
            };
            let for_sort = column.clone();
            let for_resize = column.clone();
            header = header.child(
                div()
                    .id(SharedString::from(format!("h-{column}")))
                    .relative()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap_1()
                    .w(px(*width))
                    .px(px(10.0))
                    .py(px(5.0))
                    .cursor_pointer()
                    .text_size(px(11.0))
                    .text_color(colors.text_muted)
                    .child(SharedString::from(column.clone()))
                    .children(arrow.map(|a| Text::new(a).size(Size::Xs).color(blue_color)))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_sort(&for_sort, cx)))
                    .child(
                        div()
                            .id(SharedString::from(format!("rz-{column}")))
                            .absolute()
                            .top(px(0.0))
                            .right(px(0.0))
                            .h_full()
                            .w(px(6.0))
                            .cursor_col_resize()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                                    let start_width = this.col_width(&for_resize);
                                    this.resize = Some(Resize {
                                        column: for_resize.clone(),
                                        start_x: f32::from(ev.position.x),
                                        start_width,
                                    });
                                    cx.stop_propagation();
                                }),
                            ),
                    ),
            );
        }

        // Body.
        let mut body = div().flex().flex_col();
        let default_mark = (RowMark::None, Vec::new());
        for (idx, row) in response.rows.iter().enumerate() {
            let (mark, changed) = marks.get(idx).unwrap_or(&default_mark);
            let selected = selection.contains(&idx);
            let bg = if selected {
                Some(Hsla { a: 0.22, ..blue_sel })
            } else {
                match mark {
                    RowMark::Deleted => Some(Hsla { a: 0.12, ..red }),
                    RowMark::Updated => Some(Hsla { a: 0.10, ..yellow }),
                    RowMark::None if idx % 2 == 1 => Some(colors.grid_stripe),
                    RowMark::None => None,
                }
            };
            let struck = matches!(mark, RowMark::Deleted);

            let mut tr = div()
                .id(SharedString::from(format!("r-{idx}")))
                .flex()
                .flex_none()
                .w(px(total_w))
                .border_b_1()
                .border_color(colors.border_subtle)
                .on_click(cx.listener(move |this, event: &gpui::ClickEvent, _, cx| {
                    let mods = event.modifiers();
                    this.click_row(idx, mods.platform || mods.control, mods.shift, cx);
                }));
            if let Some(bg) = bg {
                tr = tr.bg(bg);
            }

            // Row-number cell.
            tr = tr.child(
                div()
                    .flex_none()
                    .w(px(ROW_NUM_W))
                    .px(px(8.0))
                    .py(px(3.0))
                    .border_r_1()
                    .border_color(colors.border_subtle)
                    .text_size(px(10.0))
                    .text_color(colors.text_muted)
                    .child(SharedString::from(format!("{}", idx + 1))),
            );

            for (column, width) in columns.iter().zip(&widths) {
                let editing_here = self
                    .editing
                    .as_ref()
                    .is_some_and(|e| e.row == idx && &e.column == column);
                let is_changed = changed.iter().any(|c| c == column);

                let mut td = div()
                    .id(SharedString::from(format!("c-{idx}-{column}")))
                    .flex_none()
                    .w(px(*width))
                    .px(px(10.0))
                    .py(px(3.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(12.0))
                    .font_family(crate::theme::MONO_FAMILY);
                if is_changed {
                    td = td.bg(Hsla { a: 0.10, ..yellow }).border_l_2().border_color(yellow);
                }

                if editing_here {
                    td = td.child(self.editing.as_ref().unwrap().input.clone());
                } else {
                    let (display, color) = match row.get(column) {
                        None | Some(Value::Null) => (null_display.clone(), dimmed),
                        Some(v) => {
                            let color = if struck { dimmed } else { text_color };
                            (cell_text(Some(v), &null_display), color)
                        }
                    };
                    let column = column.clone();
                    td = td.text_color(color).child(SharedString::from(display)).on_click(
                        cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                            if event.click_count() == 2 {
                                let value =
                                    this.state.rows.read(cx).as_ref().and_then(|response| {
                                        response.rows.get(idx).and_then(|r| r.get(&column)).cloned()
                                    });
                                this.start_edit(idx, column.clone(), value.as_ref(), window, cx);
                            }
                        }),
                    );
                }
                tr = tr.child(td);
            }
            body = body.child(tr);
        }

        div()
            .id("data-grid-scroll")
            .size_full()
            .overflow_x_scroll()
            .overflow_y_scroll()
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if let Some(resize) = this.resize.clone() {
                    let next = (resize.start_width + f32::from(ev.position.x) - resize.start_x)
                        .max(MIN_COL_W);
                    this.widths.insert(resize.column, next);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    if this.resize.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .child(div().flex().flex_col().w(px(total_w)).child(header).child(body))
            .into_any_element()
    }
}

fn header_cell(label: &str, width: f32, color: Hsla) -> impl IntoElement {
    div()
        .flex_none()
        .w(px(width))
        .px(px(8.0))
        .py(px(5.0))
        .text_size(px(10.0))
        .text_color(color)
        .child(SharedString::from(label.to_string()))
}
