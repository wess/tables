//! The Data tab: a toolbar, the editable grid, a pending-changes bar with a
//! review/commit modal, an insert modal, and paging. Rows refetch whenever
//! `rows_epoch` bumps.
//!
//! Split by responsibility: the panel core and layout live here; write/transfer
//! handlers in `actions`; the toolbar / review / inspector render helpers in
//! `panels`.

mod actions;
mod panels;

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, Window};
use guise::prelude::*;

use crate::bridge;
use crate::state::{AppState, WorkspaceState};
use crate::workspace::filter::FilterPanel;
use crate::workspace::grid::DataGrid;
use crate::workspace::insert::InsertModal;
use model::RowsRequest;

pub struct DataPanel {
    app: AppState,
    state: WorkspaceState,
    grid: Entity<DataGrid>,
    menu: Entity<ContextMenu>,
    filter: Entity<FilterPanel>,
    insert: Option<Entity<InsertModal>>,
    show_review: bool,
    /// True while the toolbar "Discard" confirmation is up.
    confirm_discard: bool,
    committing: Signal<bool>,
    /// True while an async toolbar action (generate / import / export) runs, so
    /// the toolbar can show a spinner and disable those buttons.
    busy: Signal<bool>,
}

impl DataPanel {
    fn open_copy(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = !self.state.selection.read(cx).is_empty();
        let target = cx.weak_entity();
        let menu = cx.new(move |cx| {
            let mut menu = ContextMenu::new(cx).size(Size::Sm).width(250.0);
            for (label, action) in [("Copy selected rows", 0), ("Copy as INSERT statements", 1)] {
                if !selected {
                    continue;
                }
                let target = target.clone();
                let handler = move |_: &mut Window, cx: &mut gpui::App| {
                    if let Some(target) = target.upgrade() {
                        target.update(cx, |this, cx| {
                            if this.busy.get(cx) || this.committing.get(cx) {
                                return;
                            }
                            match action {
                                0 => this.copy_selection(cx),
                                _ => this.copy_as_insert(cx),
                            }
                        });
                    }
                };
                menu = menu.item(label, handler);
            }
            menu
        });
        menu.update(cx, |menu, cx| menu.show(position, window, cx));
        self.menu = menu;
        cx.notify();
    }

    pub fn new(app: AppState, state: WorkspaceState, cx: &mut Context<Self>) -> Self {
        cx.observe(state.active_table.entity(), |this, _, cx| {
            this.insert = None;
            this.show_review = false;
            this.confirm_discard = false;
            cx.notify();
        })
        .detach();
        watch(cx, &state.rows);
        watch(cx, &state.rows_loading);
        watch(cx, &state.selection);
        watch(cx, &state.pending);
        watch(cx, &state.filter_panel_open);
        watch(cx, &state.inspector_open);
        let committing = Signal::new(cx, false);
        watch(cx, &committing);
        let busy = Signal::new(cx, false);
        watch(cx, &busy);

        let grid = {
            let (app, state) = (app.clone(), state.clone());
            cx.new(move |cx| DataGrid::new(app, state, cx))
        };
        let filter = {
            let (app, state) = (app.clone(), state.clone());
            cx.new(move |cx| FilterPanel::new(app, state, cx))
        };

        let effect_app = app.clone();
        let effect_state = state.clone();
        use_effect(cx, &state.rows_epoch, move |_, cx| {
            fetch_rows(&effect_app, &effect_state, cx);
        });

        let menu = cx.new(ContextMenu::new);
        DataPanel {
            app,
            state,
            grid,
            menu,
            filter,
            insert: None,
            show_review: false,
            confirm_discard: false,
            committing,
            busy,
        }
    }

    fn page_size(&self, cx: &gpui::App) -> u64 {
        self.app.settings.read(cx).grid_page_size.max(1)
    }
}

/// Build the rows request from the current state and load it off-thread.
fn fetch_rows(app: &AppState, state: &WorkspaceState, cx: &mut gpui::App) {
    let Some(table) = state.active_table.get(cx) else {
        return;
    };
    let page_size = app.settings.read(cx).grid_page_size.max(1);
    let filters = state.applied_filters.get(cx);
    let has_filters = !filters.conditions.is_empty();
    let request = RowsRequest {
        table,
        page: state.page.get(cx),
        page_size,
        sort: state.sort.get(cx),
        filters: has_filters.then(|| filters.conditions.clone()),
        filter_logic: has_filters.then(|| filters.logic.clone()),
    };

    state.rows_loading.set(cx, true);
    let host = app.host.clone();
    let rows = state.rows.clone();
    let loading = state.rows_loading.clone();
    let toasts = app.toasts.clone();
    // Request-ownership token: each fetch is triggered by a distinct
    // `rows_epoch`. A completion whose epoch is stale (a newer table/page/sort/
    // filter change has since fired) is dropped so it cannot replace current
    // state with an older result.
    let generation = state.rows_epoch.get(cx);
    let epoch = state.rows_epoch.clone();
    let selection = state.selection.clone();
    let restoring = state.restoring_selection.clone();
    bridge::run(
        cx,
        async move { host.table_rows(&request).await },
        move |result, cx| {
            if epoch.get(cx) != generation {
                return;
            }
            loading.set(cx, false);
            match result {
                Ok(response) => {
                    let wanted = restoring.get(cx);
                    if !wanted.is_empty() {
                        selection.set(
                            cx,
                            response
                                .rows
                                .iter()
                                .enumerate()
                                .filter_map(|(i, row)| wanted.contains(row).then_some(i))
                                .collect(),
                        );
                        restoring.set(cx, Vec::new());
                    }
                    rows.set(cx, Some(response));
                }
                Err(error) => {
                    rows.set(cx, None);
                    toasts.error(cx, "Load failed", &error);
                }
            }
        },
    );
}

impl Render for DataPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);

        if self.state.active_table.read(cx).is_none() {
            return div()
                .flex()
                .size_full()
                .child(
                    Center::new().child(
                        Stack::new()
                            .align(Align::Center)
                            .gap(Size::Xs)
                            .child(ThemeIcon::new("▤").color(ColorName::Gray).size(Size::Xl))
                            .child(Text::new("Select a table").size(Size::Sm).dimmed()),
                    ),
                )
                .into_any_element();
        }

        let page = self.state.page.get(cx);
        let page_size = self.page_size(cx);
        let total = self
            .state
            .rows
            .read(cx)
            .as_ref()
            .map(|r| r.total)
            .unwrap_or(0);
        let last_page = (total.max(0) as u64).div_ceil(page_size).max(1);

        let pagination = div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.0))
            .py(px(6.0))
            .border_t_1()
            .border_color(colors.border)
            .child(
                Group::new()
                    .gap(Size::Xs)
                    .align(Align::Center)
                    .child(
                        Button::new("page-prev", "‹")
                            .size(Size::Xs)
                            .variant(Variant::Default)
                            .disabled(page <= 1)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let page = this.state.page.get(cx);
                                if page > 1 {
                                    this.state.page.set(cx, page - 1);
                                    this.state.bump_rows(cx);
                                }
                            })),
                    )
                    .child(
                        Text::new(format!("Page {page} of {last_page}"))
                            .size(Size::Xs)
                            .dimmed(),
                    )
                    .child(
                        Button::new("page-next", "›")
                            .size(Size::Xs)
                            .variant(Variant::Default)
                            .disabled(page >= last_page)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let page = this.state.page.get(cx);
                                if page < last_page {
                                    this.state.page.set(cx, page + 1);
                                    this.state.bump_rows(cx);
                                }
                            })),
                    ),
            )
            .child(Text::new(format!("{total} rows")).size(Size::Xs).dimmed());

        let toolbar = self.toolbar(cx, colors.border);
        let filter_open = *self.state.filter_panel_open.read(cx);

        let inspector_open = *self.state.inspector_open.read(cx);
        let mut mid = div()
            .flex()
            .flex_1()
            .min_h(px(0.0))
            .child(div().flex_1().min_w(px(0.0)).child(self.grid.clone()));
        if inspector_open {
            mid = mid.child(self.inspector_panel(cx));
        }

        let mut root = div().flex().flex_col().size_full().child(toolbar);
        if filter_open {
            root = root.child(self.filter.clone());
        }
        let pending = self.state.pending.read(cx).len();
        let mut root = root.child(mid);
        if pending > 0 {
            root = root.child(self.pending_bar(cx, pending));
        }
        let mut root = root.child(pagination);

        if self.show_review {
            root = root.child(self.review_modal(cx));
        }
        if let Some(modal) = &self.insert {
            root = root.child(modal.clone());
        }
        if self.confirm_discard {
            let count = self.state.pending.read(cx).len();
            root = root.child(
                ConfirmModal::new()
                    .title("Discard changes")
                    .message(format!(
                        "Discard {count} staged change(s)? This cannot be undone."
                    ))
                    .confirm_label("Discard")
                    .cancel_label("Keep")
                    .danger()
                    .on_confirm(cx.listener(|this, _, _, cx| {
                        this.confirm_discard = false;
                        this.discard(cx);
                    }))
                    .on_cancel(cx.listener(|this, _, _, cx| {
                        this.confirm_discard = false;
                        cx.notify();
                    })),
            );
        }

        root.child(self.menu.clone()).into_any_element()
    }
}
