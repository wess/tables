//! The Query tab. A SQL editor plus a result grid; Cmd+Enter or the Run button
//! executes against the active connection. A toggleable right panel shows query
//! history or saved favorites.
//!
//! Split by responsibility: the panel core and layout live here; the history/
//! favorites/chart data actions in `actions`; the side panel and result render
//! helpers in `panels`.

mod actions;
mod complete;
mod panels;

use gpui::prelude::*;
use gpui::{anchored, deferred, div, point, px, Context, Entity, SharedString, Window};
use guise::prelude::*;

use crate::bridge;
use crate::state::{AppState, WorkspaceState};
use crate::workspace::charts::{ChartEvent, ChartModal};
use model::{Favorite, HistoryEntry, QueryResult};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Side {
    History,
    Favorites,
}

/// Cheap heuristic: does any statement start with a schema-changing keyword? A
/// false positive just triggers a harmless table-list refetch.
fn is_ddl(sql: &str) -> bool {
    sql.split(['\n', ';'])
        .map(str::trim_start)
        .filter_map(|s| s.split_whitespace().next())
        .any(|word| {
            matches!(
                word.to_ascii_uppercase().as_str(),
                "CREATE" | "DROP" | "ALTER" | "TRUNCATE" | "RENAME"
            )
        })
}

pub struct QueryPanel {
    app: AppState,
    state: WorkspaceState,
    editor: Entity<Editor>,
    fav_name: Entity<TextInput>,
    results: Signal<Vec<QueryResult>>,
    running: Signal<bool>,
    side: Signal<Option<Side>>,
    history: Signal<Vec<HistoryEntry>>,
    favorites: Signal<Vec<Favorite>>,
    chart_modal: Option<Entity<ChartModal>>,
    /// Table/column identifiers for autocomplete, loaded once in the background.
    schema: Signal<Vec<String>>,
    /// The current completion suggestions (empty = popup hidden).
    completions: Vec<String>,
}

impl QueryPanel {
    pub fn new(app: AppState, state: WorkspaceState, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            Editor::new(cx)
                .language(Language::Sql)
                .rows(10)
                .placeholder("SELECT * FROM …   (⌘⏎ to run)")
        });
        let fav_name = cx.new(|cx| TextInput::new(cx).placeholder("Favorite name").size(Size::Xs));
        let results = Signal::new(cx, Vec::new());
        let running = Signal::new(cx, false);
        let side = Signal::new(cx, None);
        let history = Signal::new(cx, Vec::new());
        let favorites = Signal::new(cx, Vec::new());
        let schema = Signal::new(cx, Vec::new());
        watch(cx, &results);
        watch(cx, &running);
        watch(cx, &side);
        watch(cx, &history);
        watch(cx, &favorites);

        // Load autocomplete identifiers once in the background.
        {
            let host = app.host.clone();
            let out = schema.clone();
            bridge::run(
                cx,
                async move { host.schema_identifiers().await.unwrap_or_default() },
                move |ids, cx| out.set(cx, ids),
            );
        }

        cx.subscribe(&editor, |this, editor, event: &EditorEvent, cx| match event {
            EditorEvent::Run(_) => {
                let sql = editor.read(cx).text();
                this.completions.clear();
                this.run(sql, cx);
            }
            EditorEvent::Change(_) => this.update_completions(cx),
        })
        .detach();

        QueryPanel {
            app,
            state,
            editor,
            fav_name,
            results,
            running,
            side,
            history,
            favorites,
            chart_modal: None,
            schema,
            completions: Vec::new(),
        }
    }

    /// Recompute the completion list from the identifier under the cursor.
    fn update_completions(&mut self, cx: &mut Context<Self>) {
        let word = {
            let model = self.editor.read(cx).model();
            let cursor = model.cursor();
            model.line(cursor.line).and_then(|line| {
                let chars: Vec<char> = line.chars().collect();
                let col = cursor.col.min(chars.len());
                let mut start = col;
                while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
                    start -= 1;
                }
                (start < col).then(|| chars[start..col].iter().collect::<String>())
            })
        };
        self.completions = match word {
            Some(w) => complete::suggestions(&w, self.schema.read(cx)),
            None => Vec::new(),
        };
        cx.notify();
    }

    /// Replace the word under the cursor with `word`.
    fn accept_completion(&mut self, word: &str, window: &mut Window, cx: &mut Context<Self>) {
        let word = word.to_string();
        self.editor.update(cx, |ed, cx| {
            ed.edit(window, cx, |model| {
                model.select_word();
                model.delete_selection();
                model.insert(&word);
            });
        });
        self.completions.clear();
        cx.notify();
    }

    /// Load SQL into the editor without running it (assistant "Insert").
    pub fn set_sql(&self, sql: &str, cx: &mut gpui::App) {
        self.editor.update(cx, |editor, cx| editor.set_text(sql, cx));
    }

    /// Load SQL into the editor and run it immediately (assistant "Run").
    pub fn run_sql(&self, sql: String, cx: &mut gpui::App) {
        self.editor.update(cx, |editor, cx| editor.set_text(&sql, cx));
        self.run(sql, cx);
    }

    /// Run whatever is currently in the editor (the Query → Execute action).
    pub fn run_current(&self, cx: &mut gpui::App) {
        let sql = self.editor.read(cx).text();
        self.run(sql, cx);
    }

    fn run(&self, sql: String, cx: &mut gpui::App) {
        if sql.trim().is_empty() {
            return;
        }
        self.running.set(cx, true);
        let host = self.app.host.clone();
        let results = self.results.clone();
        let running = self.running.clone();
        let toasts = self.app.toasts.clone();
        let state = self.state.clone();
        // Decide once, before `sql` moves into the future, whether to refresh the
        // sidebar's table list when this succeeds.
        let ddl = is_ddl(&sql);
        bridge::run(
            cx,
            async move { host.execute_multi(&sql).await },
            move |outcome, cx| {
                running.set(cx, false);
                match outcome {
                    Ok(list) => {
                        results.set(cx, list);
                        if ddl {
                            state.bump_tables(cx);
                        }
                    }
                    Err(error) => {
                        results.set(cx, Vec::new());
                        toasts.error(cx, "Query failed", &error);
                    }
                }
            },
        );
    }

    /// The autocomplete list, floated at the caret via a deferred anchor.
    fn completion_popup(
        &self,
        caret: gpui::Point<gpui::Pixels>,
        line_h: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let muted = colors.bg_muted;
        let mut list = div()
            .flex()
            .flex_col()
            .min_w(px(160.0))
            .py(px(4.0))
            .bg(colors.bg_surface)
            .border_1()
            .border_color(colors.border)
            .rounded(px(6.0))
            .shadow_lg();
        for (i, s) in self.completions.iter().enumerate() {
            let word = s.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("cmpl-{i}")))
                    .px(px(8.0))
                    .py(px(3.0))
                    .text_size(px(12.0))
                    .font_family(crate::theme::MONO_FAMILY)
                    .cursor_pointer()
                    .hover(move |d| d.bg(muted))
                    .child(SharedString::from(s.clone()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.accept_completion(&word, window, cx)
                    })),
            );
        }
        deferred(anchored().position(point(caret.x, caret.y + px(line_h))).child(list))
    }

    fn open_chart(&mut self, cx: &mut Context<Self>) {
        // Borrow to pick the chartable result, cloning only its columns/rows.
        let picked = {
            let results = self.results.read(cx);
            results
                .iter()
                .find(|r| !r.columns.is_empty() && !r.rows.is_empty())
                .map(|r| (r.columns.clone(), r.rows.clone()))
        };
        let Some((columns, rows)) = picked else {
            return;
        };
        let modal = cx.new(move |cx| ChartModal::new(columns, rows, cx));
        cx.subscribe(&modal, |this, _m, _e: &ChartEvent, cx| {
            this.chart_modal = None;
            cx.notify();
        })
        .detach();
        self.chart_modal = Some(modal);
        cx.notify();
    }
}

impl Render for QueryPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let running = *self.running.read(cx);
        let side = *self.side.read(cx);
        let has_chartable = self
            .results
            .read(cx)
            .iter()
            .any(|r| !r.columns.is_empty() && !r.rows.is_empty());

        let toolbar = div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(colors.border)
            .child(
                Group::new()
                    .gap(Size::Xs)
                    .align(Align::Center)
                    .child(
                        Button::new("query-run", if running { "Running…" } else { "Run" })
                            .size(Size::Xs)
                            .disabled(running || self.editor.read(cx).text().trim().is_empty())
                            .on_click(cx.listener(|this, _, _, cx| {
                                let sql = this.editor.read(cx).text();
                                this.run(sql, cx);
                            })),
                    )
                    .child(Text::new("⌘⏎").size(Size::Xs).dimmed()),
            )
            .child(
                Group::new()
                    .gap(Size::Xs)
                    .child(
                        Button::new("query-chart", "Chart")
                            .size(Size::Xs)
                            .variant(Variant::Subtle)
                            .disabled(!has_chartable)
                            .on_click(cx.listener(|this, _, _, cx| this.open_chart(cx))),
                    )
                    .child(
                        Button::new("query-export", "Export")
                            .size(Size::Xs)
                            .variant(Variant::Subtle)
                            .disabled(!has_chartable)
                            .on_click(cx.listener(|this, _, _, cx| this.export_results(cx))),
                    )
                    .child(
                        Button::new("query-copy-md", "Copy MD")
                            .size(Size::Xs)
                            .variant(Variant::Subtle)
                            .disabled(!has_chartable)
                            .on_click(cx.listener(|this, _, _, cx| this.copy_results_markdown(cx))),
                    )
                    .child(
                        Button::new("query-history", "History")
                            .size(Size::Xs)
                            .variant(if side == Some(Side::History) { Variant::Light } else { Variant::Subtle })
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_side(Side::History, cx))),
                    )
                    .child(
                        Button::new("query-favorites", "Favorites")
                            .size(Size::Xs)
                            .variant(if side == Some(Side::Favorites) { Variant::Light } else { Variant::Subtle })
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_side(Side::Favorites, cx))),
                    ),
            );

        let editor_pane = div()
            .p(px(8.0))
            .border_b_1()
            .border_color(colors.border)
            .child(self.editor.clone());

        let results = self.results.read(cx);
        let result_pane = if results.is_empty() {
            Center::new()
                .child(Text::new("Run a query to see results").size(Size::Sm).dimmed())
                .into_any_element()
        } else {
            let multi = results.len() > 1;
            let mut stack = Stack::new().gap(Size::Md);
            for (i, result) in results.iter().enumerate() {
                stack = stack.child(self.result_block(i, result, multi, cx));
            }
            div()
                .id("query-results")
                .size_full()
                .overflow_x_scroll()
                .overflow_y_scroll()
                .p(px(8.0))
                .child(stack)
                .into_any_element()
        };

        let editor_col = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .child(toolbar)
            .child(editor_pane)
            .child(div().flex_1().min_h(px(0.0)).child(result_pane));

        let mut root = div().relative().flex().size_full().child(editor_col);
        if let Some(side) = side {
            root = root.child(self.side_panel(side, cx));
        }
        if let Some(modal) = &self.chart_modal {
            root = root.child(modal.clone());
        }
        if !self.completions.is_empty() {
            let caret = self.editor.read(cx).caret_origin(window);
            let line_h = self.editor.read(cx).line_height();
            root = root.child(self.completion_popup(caret, line_h, cx));
        }
        root
    }
}
