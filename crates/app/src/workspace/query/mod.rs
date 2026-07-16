//! The Query tab. A SQL editor plus a result grid; Cmd+Enter or the Run button
//! executes against the active connection. A toggleable right panel shows query
//! history or saved favorites.
//!
//! Split by responsibility: the panel core and layout live here; the history/
//! favorites/chart data actions in `actions`; the side panel and result render
//! helpers in `panels`.

mod actions;
mod panels;

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, Window};
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

pub struct QueryPanel {
    app: AppState,
    #[allow(dead_code)]
    state: WorkspaceState,
    editor: Entity<Editor>,
    fav_name: Entity<TextInput>,
    results: Signal<Vec<QueryResult>>,
    running: Signal<bool>,
    side: Signal<Option<Side>>,
    history: Signal<Vec<HistoryEntry>>,
    favorites: Signal<Vec<Favorite>>,
    chart_modal: Option<Entity<ChartModal>>,
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
        watch(cx, &results);
        watch(cx, &running);
        watch(cx, &side);
        watch(cx, &history);
        watch(cx, &favorites);

        cx.subscribe(&editor, |this, editor, event: &EditorEvent, cx| {
            if let EditorEvent::Run(_) = event {
                let sql = editor.read(cx).text();
                this.run(sql, cx);
            }
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
        }
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
        bridge::run(
            cx,
            async move { host.execute_multi(&sql).await },
            move |outcome, cx| {
                running.set(cx, false);
                match outcome {
                    Ok(list) => results.set(cx, list),
                    Err(error) => {
                        results.set(cx, Vec::new());
                        toasts.error(cx, "Query failed", &error);
                    }
                }
            },
        );
    }

    fn open_chart(&mut self, cx: &mut Context<Self>) {
        let results = self.results.get(cx);
        let Some(result) = results.iter().find(|r| !r.columns.is_empty() && !r.rows.is_empty())
        else {
            return;
        };
        let columns = result.columns.clone();
        let rows = result.rows.clone();
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                            .disabled(running)
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

        let results = self.results.get(cx);
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
        root
    }
}
