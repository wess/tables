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
            if let Some(response) = self.state.rows.get(cx) {
                if let Some(row) = response.rows.get(editing.row) {
                    let value = if text.is_empty() { Value::Null } else { Value::String(text) };
                    let mut changes = Row::new();
                    changes.insert(editing.column, value);
                    let table = self.state.active_table.get(cx).unwrap_or_default();
                    self.state.pending.update(cx, move |pending| {
                        pending.push(PendingChange::Update {
                            table,
                            primary_key: row.clone(),
                            changes,
                        });
                    });
                }
            }
        }
        cx.notify();
    }

    fn row_mark(&self, row: &Row, cx: &gpui::App) -> (RowMark, Vec<String>) {
        let mut mark = RowMark::None;
        let mut changed = Vec::new();
        for change in self.state.pending.read(cx).iter() {
            match change {
                PendingChange::Delete { primary_key, .. } if pk_matches(primary_key, row) => {
                    mark = RowMark::Deleted;
                }
                PendingChange::Update { primary_key, changes, .. }
                    if pk_matches(primary_key, row) =>
                {
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
}

impl Render for DataGrid {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let null_display = self.app.settings.read(cx).null_display.clone();
        let theme = guise::theme::theme(cx);
        let yellow = theme.color(ColorName::Yellow, 6).hsla();
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

        let response = self.state.rows.get(cx).unwrap_or_default();
        let sort = self.state.sort.get(cx);
        let selection = self.state.selection.get(cx);

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
        for (idx, row) in response.rows.iter().enumerate() {
            let (mark, changed) = self.row_mark(row, cx);
            let selected = selection.contains(&idx);
            let bg = if selected {
                Some(Hsla { h: 0.62, s: 0.6, l: 0.5, a: 0.22 })
            } else {
                match mark {
                    RowMark::Deleted => Some(Hsla { h: 0.0, s: 0.65, l: 0.5, a: 0.12 }),
                    RowMark::Updated => Some(Hsla { h: 0.13, s: 0.75, l: 0.5, a: 0.10 }),
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
