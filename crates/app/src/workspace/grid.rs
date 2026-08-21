//! The data grid. A custom grid — not guise's `Table` or `TableView` — so cells
//! can be selected, sorted, and edited inline.
//!
//! `TableView` was the obvious candidate and is the wrong shape for this one:
//! it sorts client-side (Tables sorts in the database, because the page in front
//! of you is one page of many), it has no per-row background hook (a staged
//! update tints its row yellow, a staged delete red), and its cell closures see
//! the row but not its index, which is what an edit, a selection and a pending
//! mark are all keyed by.
//!
//! What it does take from guise is [`VirtualList`], for the part that actually
//! needed it: only the rows in view are built, so a large page costs what a
//! small one does. That wants two things this grid did not previously have — a
//! uniform row height, and a definite viewport height — and both are now real.
//! The height comes off an invisible `canvas` probe measuring the body area, the
//! way guise's own components measure themselves.
//!
//! Editing a cell queues a `PendingChange::Update`; the commit flow lives in the
//! Data panel.

use std::collections::HashMap;

use gpui::prelude::*;
use gpui::{
    canvas, div, px, AnyElement, App, Bounds, Context, Entity, Hsla, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, SharedString, WeakEntity, Window,
};
use guise::prelude::*;
use serde_json::Value;

use crate::state::{AppState, PendingChange, WorkspaceState};
use crate::workspace::cell_text;
use model::{Row, SortSpec};

const ROW_NUM_W: f32 = 48.0;
const COL_W: f32 = 168.0;
const MIN_COL_W: f32 = 60.0;
const HEADER_H: f32 = 26.0;
/// Until the probe has measured the body once. Only the first frame uses it.
const DEFAULT_BODY_H: f32 = 400.0;

/// Row height for each `grid_row_height` setting. `uniform_list` requires every
/// row to agree, so this is the single place a row's height is decided.
fn row_height(setting: &str) -> f32 {
    match setting {
        "normal" => 28.0,
        "comfortable" => 34.0,
        _ => 22.0,
    }
}

pub struct DataGrid {
    app: AppState,
    state: WorkspaceState,
    editing: Option<Editing>,
    /// Per-column pixel widths; a column absent here uses `COL_W`.
    widths: HashMap<String, f32>,
    /// The in-progress column resize, if the user is dragging a header edge.
    resize: Option<Resize>,
    /// The per-cell context menu (copy value / set NULL).
    menu: Option<Entity<ContextMenu>>,
    /// Measured height of the body viewport, from the `canvas` probe.
    body_height: f32,
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
#[derive(Clone, Copy, PartialEq)]
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
        DataGrid {
            app,
            state,
            editing: None,
            widths: HashMap::new(),
            resize: None,
            menu: None,
            body_height: DEFAULT_BODY_H,
        }
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
                self.stage_update(row, editing.column, value, cx);
            }
        }
        cx.notify();
    }

    /// Stage a single-cell change, coalescing into an existing update for the
    /// same row so a multi-column edit commits as ONE `UPDATE`. Separate
    /// per-column updates each snapshot the whole row into their WHERE, so the
    /// second statement's WHERE would no longer match after the first ran in the
    /// same transaction — the later edit would be silently lost.
    fn stage_update(&self, row: Row, column: String, value: Value, cx: &mut gpui::App) {
        let table = self.state.active_table.get(cx).unwrap_or_default();
        self.state.pending.update(cx, move |pending| {
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

    /// Explicitly set a cell to SQL NULL (distinct from clearing to an empty
    /// string in the inline editor).
    fn set_cell_null(&mut self, row_idx: usize, column: String, cx: &mut Context<Self>) {
        let row = self.state.rows.read(cx).as_ref().and_then(|r| r.rows.get(row_idx).cloned());
        if let Some(row) = row {
            self.stage_update(row, column, Value::Null, cx);
        }
        cx.notify();
    }

    /// Open the per-cell context menu (copy value / set NULL) at the cursor.
    fn open_cell_menu(
        &mut self,
        row_idx: usize,
        column: String,
        pos: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = self
            .state
            .rows
            .read(cx)
            .as_ref()
            .and_then(|r| r.rows.get(row_idx))
            .and_then(|r| r.get(&column))
            .cloned();
        let display = cell_text(value.as_ref(), "");
        let this = cx.entity();
        let menu = cx.new(|cx| {
            ContextMenu::new(cx)
                .item("Copy value", {
                    let display = display.clone();
                    move |_w, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(display.clone()));
                    }
                })
                .item("Set NULL", {
                    let (this, column) = (this.clone(), column.clone());
                    move |_w, cx| {
                        this.update(cx, |grid, cx| grid.set_cell_null(row_idx, column.clone(), cx));
                    }
                })
        });
        menu.update(cx, |m, cx| m.show(pos, window, cx));
        self.menu = Some(menu);
        cx.notify();
    }
}

/// Classify how the staged changes touch one rendered row.
fn row_mark_for(row: &Row, pending: &[PendingChange]) -> (RowMark, Vec<String>) {
    let mut mark = RowMark::None;
    let mut changed = Vec::new();
    for change in pending {
        match change {
            PendingChange::Delete { primary_key, .. } if pk_matches(primary_key, row) => {
                mark = RowMark::Deleted;
            }
            PendingChange::Update { primary_key, changes, .. } if pk_matches(primary_key, row) => {
                if mark == RowMark::None {
                    mark = RowMark::Updated;
                }
                changed.extend(changes.keys().cloned());
            }
            _ => {}
        }
    }
    (mark, changed)
}

/// What one row needs to draw itself, read out of the grid in one borrow so the
/// item closure can build elements without holding onto the entity.
struct RowSnapshot {
    row: Row,
    mark: RowMark,
    changed: Vec<String>,
    selected: bool,
    editing: Option<(String, Entity<TextInput>)>,
}

/// Resolved colors the row builder needs. Read once per frame rather than per
/// row — the theme cannot change between two rows of the same frame.
#[derive(Clone, Copy)]
struct RowColors {
    yellow: Hsla,
    red: Hsla,
    selected: Hsla,
    stripe: Hsla,
    dimmed: Hsla,
    text: Hsla,
    muted: Hsla,
    border: Hsla,
}

impl DataGrid {
    /// Everything row `idx` needs, or `None` if the page no longer has it.
    fn snapshot(&self, idx: usize, pending: &[PendingChange], cx: &gpui::App) -> Option<RowSnapshot> {
        let row = self.state.rows.read(cx).as_ref()?.rows.get(idx)?.clone();
        let (mark, changed) = if pending.is_empty() {
            (RowMark::None, Vec::new())
        } else {
            row_mark_for(&row, pending)
        };
        let editing = self
            .editing
            .as_ref()
            .filter(|e| e.row == idx)
            .map(|e| (e.column.clone(), e.input.clone()));
        Some(RowSnapshot {
            row,
            mark,
            changed,
            selected: self.state.selection.read(cx).contains(&idx),
            editing,
        })
    }
}

/// Build one row. Free function rather than a method because the `VirtualList`
/// item closure is `'static` and only has `&mut App` — there is no
/// `Context<Self>` here, so every handler goes through `grid`.
#[allow(clippy::too_many_arguments)]
fn render_row(
    grid: &WeakEntity<DataGrid>,
    idx: usize,
    snap: RowSnapshot,
    columns: &[String],
    widths: &[f32],
    total_w: f32,
    row_h: f32,
    show_row_numbers: bool,
    stripe_rows: bool,
    null_display: &str,
    c: RowColors,
) -> AnyElement {
    let bg = if snap.selected {
        Some(Hsla { a: 0.22, ..c.selected })
    } else {
        match snap.mark {
            RowMark::Deleted => Some(Hsla { a: 0.12, ..c.red }),
            RowMark::Updated => Some(Hsla { a: 0.10, ..c.yellow }),
            RowMark::None if stripe_rows && idx % 2 == 1 => Some(c.stripe),
            RowMark::None => None,
        }
    };
    let struck = snap.mark == RowMark::Deleted;

    let click_grid = grid.clone();
    let mut tr = div()
        .id(SharedString::from(format!("r-{idx}")))
        .flex()
        .flex_none()
        .w(px(total_w))
        .h(px(row_h))
        .items_center()
        .border_b_1()
        .border_color(c.border)
        .on_click(move |event: &gpui::ClickEvent, _, cx| {
            let mods = event.modifiers();
            if let Some(grid) = click_grid.upgrade() {
                grid.update(cx, |grid, cx| {
                    grid.click_row(idx, mods.platform || mods.control, mods.shift, cx)
                });
            }
        });
    if let Some(bg) = bg {
        tr = tr.bg(bg);
    }

    if show_row_numbers {
        tr = tr.child(
            div()
                .flex_none()
                .w(px(ROW_NUM_W))
                .h_full()
                .flex()
                .items_center()
                .px(px(8.0))
                .border_r_1()
                .border_color(c.border)
                .text_size(px(10.0))
                .text_color(c.muted)
                .child(SharedString::from(format!("{}", idx + 1))),
        );
    }

    for (column, width) in columns.iter().zip(widths) {
        let editing_here = snap.editing.as_ref().is_some_and(|(col, _)| col == column);
        let is_changed = snap.changed.iter().any(|ch| ch == column);

        let mut td = div()
            .id(SharedString::from(format!("c-{idx}-{column}")))
            .flex_none()
            .w(px(*width))
            .h_full()
            .flex()
            .items_center()
            .px(px(10.0))
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(px(12.0))
            .font_family(crate::theme::MONO_FAMILY);
        if is_changed {
            td = td.bg(Hsla { a: 0.10, ..c.yellow }).border_l_2().border_color(c.yellow);
        }

        if editing_here {
            let (_, input) = snap.editing.as_ref().unwrap();
            td = td.child(input.clone());
        } else {
            let (display, color) = match snap.row.get(column) {
                None | Some(Value::Null) => (null_display.to_string(), c.dimmed),
                Some(v) => {
                    let color = if struck { c.dimmed } else { c.text };
                    (cell_text(Some(v), null_display), color)
                }
            };
            let edit_grid = grid.clone();
            let edit_column = column.clone();
            let menu_grid = grid.clone();
            let menu_column = column.clone();
            td = td
                .text_color(color)
                .child(SharedString::from(display))
                .on_click(move |event: &gpui::ClickEvent, window, cx| {
                    if event.click_count() != 2 {
                        return;
                    }
                    let Some(grid) = edit_grid.upgrade() else { return };
                    grid.update(cx, |grid, cx| {
                        let value = grid
                            .state
                            .rows
                            .read(cx)
                            .as_ref()
                            .and_then(|r| r.rows.get(idx).and_then(|r| r.get(&edit_column)))
                            .cloned();
                        grid.start_edit(idx, edit_column.clone(), value.as_ref(), window, cx);
                    });
                })
                .on_mouse_down(MouseButton::Right, move |ev: &MouseDownEvent, window, cx| {
                    if let Some(grid) = menu_grid.upgrade() {
                        grid.update(cx, |grid, cx| {
                            grid.open_cell_menu(idx, menu_column.clone(), ev.position, window, cx)
                        });
                    }
                });
        }
        tr = tr.child(td);
    }
    tr.into_any_element()
}

impl Render for DataGrid {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let settings = self.app.settings.read(cx);
        let null_display = settings.null_display.clone();
        let row_h = row_height(&settings.grid_row_height);
        let show_row_numbers = settings.grid_show_row_numbers;
        let stripe_rows = settings.grid_alternate_rows;

        let theme = guise::theme::theme(cx);
        let row_colors = RowColors {
            yellow: theme.color(ColorName::Yellow, 6).hsla(),
            red: theme.color(ColorName::Red, 6).hsla(),
            selected: theme.color(ColorName::Blue, 5).hsla(),
            stripe: colors.grid_stripe,
            dimmed: theme.dimmed().hsla(),
            text: theme.text().hsla(),
            muted: colors.text_muted,
            border: colors.border_subtle,
        };
        let blue_color = theme.color(ColorName::Blue, 4);

        let columns = self.visible_columns(cx);
        let row_count = self.state.rows.read(cx).as_ref().map_or(0, |r| r.rows.len());
        if columns.is_empty() || row_count == 0 {
            return div()
                .flex()
                .size_full()
                .child(Center::new().child(Text::new("No rows").size(Size::Sm).dimmed()))
                .into_any_element();
        }

        let sort = self.state.sort.get(cx);

        // Column widths and the total content width (drives horizontal scroll).
        let widths: Vec<f32> = columns.iter().map(|c| self.col_width(c)).collect();
        let row_num_w = if show_row_numbers { ROW_NUM_W } else { 0.0 };
        let total_w = row_num_w + widths.iter().sum::<f32>();

        // Header. Outside the virtual list, so it is sticky for free — and it
        // still has a `Context`, so its handlers stay ordinary listeners.
        let mut header = div()
            .flex()
            .flex_none()
            .w(px(total_w))
            .h(px(HEADER_H))
            .items_center()
            .bg(colors.bg_subtle)
            .border_b_1()
            .border_color(colors.border);
        if show_row_numbers {
            header = header.child(header_cell("#", ROW_NUM_W, colors.text_muted));
        }
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
                    .h_full()
                    .px(px(10.0))
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

        // Body. The item closure runs per visible row per frame, so it reads
        // live state rather than a snapshot of the whole page taken up here.
        let weak = cx.entity().downgrade();
        let list_columns = columns.clone();
        let list_widths = widths.clone();
        let list_null = null_display.clone();
        let body = VirtualList::new(
            "data-grid-body",
            row_count,
            move |idx, _window, cx: &mut App| {
                let Some(grid) = weak.upgrade() else {
                    return div().into_any_element();
                };
                let snap = {
                    let g = grid.read(cx);
                    let pending = g.state.pending.read(cx).clone();
                    g.snapshot(idx, &pending, cx)
                };
                match snap {
                    Some(snap) => render_row(
                        &weak,
                        idx,
                        snap,
                        &list_columns,
                        &list_widths,
                        total_w,
                        row_h,
                        show_row_numbers,
                        stripe_rows,
                        &list_null,
                        row_colors,
                    ),
                    None => div().h(px(row_h)).into_any_element(),
                }
            },
        )
        .height(self.body_height);

        // Invisible probe: the body's height is whatever the layout leaves it,
        // and `uniform_list` needs that as a number.
        //
        // The write is deferred rather than done in the callback. This runs
        // inside the grid's own prepaint, where the entity is already borrowed
        // and a `notify` would be swallowed by the frame producing it — so the
        // height would be measured correctly and never used. `defer` puts both
        // after the frame, where the notify schedules a real new one.
        let probe_target = cx.entity();
        let probe = canvas(
            move |bounds: Bounds<Pixels>, _window, cx| {
                let measured = f32::from(bounds.size.height);
                let target = probe_target.clone();
                cx.defer(move |cx| {
                    target.update(cx, |grid, cx| {
                        if (grid.body_height - measured).abs() > 0.5 {
                            grid.body_height = measured;
                            cx.notify();
                        }
                    });
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        let mut scroll = div()
            .id("data-grid-scroll")
            .size_full()
            .overflow_x_scroll()
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
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(total_w))
                    .h_full()
                    .child(header)
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_h(px(0.0))
                            .w_full()
                            .child(probe)
                            .child(body),
                    ),
            );
        if let Some(menu) = &self.menu {
            scroll = scroll.child(menu.clone());
        }
        scroll.into_any_element()
    }
}

fn header_cell(label: &str, width: f32, color: Hsla) -> impl IntoElement {
    div()
        .flex_none()
        .w(px(width))
        .h_full()
        .flex()
        .items_center()
        .px(px(8.0))
        .text_size(px(10.0))
        .text_color(color)
        .child(SharedString::from(label.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(id: i64) -> Row {
        let mut r = Row::new();
        r.insert("id".into(), json!(id));
        r
    }

    #[test]
    fn row_height_follows_the_setting_and_falls_back_to_compact() {
        assert_eq!(row_height("compact"), 22.0);
        assert_eq!(row_height("normal"), 28.0);
        assert_eq!(row_height("comfortable"), 34.0);
        // An unknown value is compact rather than a panic or a zero-height row.
        assert_eq!(row_height("nonsense"), 22.0);
    }

    #[test]
    fn a_staged_delete_outranks_a_staged_update_on_the_same_row() {
        let target = row(1);
        let pending = vec![
            PendingChange::Update {
                table: "t".into(),
                primary_key: target.clone(),
                changes: row(1),
            },
            PendingChange::Delete { table: "t".into(), primary_key: target.clone() },
        ];
        let (mark, _) = row_mark_for(&target, &pending);
        assert!(mark == RowMark::Deleted);
    }

    #[test]
    fn an_untouched_row_is_unmarked() {
        let pending =
            vec![PendingChange::Delete { table: "t".into(), primary_key: row(1) }];
        let (mark, changed) = row_mark_for(&row(2), &pending);
        assert!(mark == RowMark::None);
        assert!(changed.is_empty());
    }

    #[test]
    fn an_update_reports_the_columns_it_touched() {
        let target = row(7);
        let mut changes = Row::new();
        changes.insert("name".into(), json!("ada"));
        let pending = vec![PendingChange::Update {
            table: "t".into(),
            primary_key: target.clone(),
            changes,
        }];
        let (mark, changed) = row_mark_for(&target, &pending);
        assert!(mark == RowMark::Updated);
        assert_eq!(changed, vec!["name".to_string()]);
    }
}
