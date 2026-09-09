//! Cross-panel state contracts. `AppState` lives for the whole app and is
//! provided as context by `Root`; `WorkspaceState` lives while a connection
//! workspace is open. Panels read the signals they care about and `watch` them.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;

use guise::prelude::*;

use crate::toasts::Toasts;
use host::Host;
use model::{FilterCondition, RowsResponse, Settings, SortSpec, StoredConnection, TableInfo};

#[derive(Clone, Debug, PartialEq)]
pub enum Route {
    Home,
    Workspace(String),
}

/// App-wide state, provided once as context.
#[derive(Clone)]
pub struct AppState {
    pub host: Arc<Host>,
    pub route: Signal<Route>,
    pub settings: Signal<Settings>,
    pub toasts: Toasts,
}

impl AppState {
    pub fn get(cx: &gpui::App) -> AppState {
        use_context::<AppState>(cx).expect("AppState provided by Root")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceTab {
    Data,
    Query,
    Structure,
}

/// Filter panel state: a draft being edited and the applied set actually sent
/// with a rows request.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FilterState {
    pub conditions: Vec<FilterCondition>,
    pub logic: String, // "and" | "or"
}

#[derive(Clone, Debug, PartialEq)]
pub enum PendingChange {
    Update {
        table: String,
        primary_key: model::Row,
        changes: model::Row,
    },
    Insert {
        table: String,
        row: model::Row,
    },
    Delete {
        table: String,
        primary_key: model::Row,
    },
}

/// Per-workspace shared state. Signals so any panel can watch what it needs.
#[derive(Clone)]
pub struct WorkspaceState {
    pub connection_id: String,
    pub connection: Signal<Option<StoredConnection>>,
    pub tables: Signal<Vec<TableInfo>>,
    pub tables_loading: Signal<bool>,
    /// `Some` when the last table load failed — distinct from an empty schema.
    pub tables_error: Signal<Option<String>>,
    pub databases: Signal<Vec<String>>,
    pub active_table: Signal<Option<String>>,
    pub active_tab: Signal<WorkspaceTab>,
    pub open_tables: Signal<Vec<String>>,
    sessions: Rc<RefCell<BTreeMap<String, TableSession>>>,

    // active table state; inactive tabs retain settings and edits, not result sets.
    pub page: Signal<u64>,
    pub sort: Signal<Option<SortSpec>>,
    pub draft_filters: Signal<FilterState>,
    pub applied_filters: Signal<FilterState>,
    pub filter_panel_open: Signal<bool>,
    pub selection: Signal<BTreeSet<usize>>,
    pub restoring_selection: Signal<Vec<model::Row>>,
    pub hidden_columns: Signal<BTreeSet<String>>,
    pub inspector_open: Signal<bool>,
    pub rows: Signal<Option<RowsResponse>>,
    pub rows_loading: Signal<bool>,
    pub pending: Signal<Vec<PendingChange>>,

    /// Bump to ask the workspace to refetch rows / tables.
    pub rows_epoch: Signal<u64>,
    pub tables_epoch: Signal<u64>,

    /// The slide-out AI assistant column.
    pub ai_open: Signal<bool>,
}

impl WorkspaceState {
    pub fn get(cx: &gpui::App) -> WorkspaceState {
        use_context::<WorkspaceState>(cx).expect("WorkspaceState provided by workspace")
    }

    pub fn new(cx: &mut gpui::App, connection_id: String) -> Self {
        WorkspaceState {
            connection_id,
            connection: Signal::new(cx, None),
            tables: Signal::new(cx, Vec::new()),
            tables_loading: Signal::new(cx, true),
            tables_error: Signal::new(cx, None),
            databases: Signal::new(cx, Vec::new()),
            active_table: Signal::new(cx, None),
            active_tab: Signal::new(cx, WorkspaceTab::Query),
            open_tables: Signal::new(cx, Vec::new()),
            sessions: Rc::new(RefCell::new(BTreeMap::new())),
            page: Signal::new(cx, 1),
            sort: Signal::new(cx, None),
            draft_filters: Signal::new(cx, FilterState::default()),
            applied_filters: Signal::new(cx, FilterState::default()),
            filter_panel_open: Signal::new(cx, false),
            selection: Signal::new(cx, BTreeSet::new()),
            restoring_selection: Signal::new(cx, Vec::new()),
            hidden_columns: Signal::new(cx, BTreeSet::new()),
            inspector_open: Signal::new(cx, false),
            rows: Signal::new(cx, None),
            rows_loading: Signal::new(cx, false),
            pending: Signal::new(cx, Vec::new()),
            rows_epoch: Signal::new(cx, 0),
            tables_epoch: Signal::new(cx, 0),
            ai_open: Signal::new(cx, false),
        }
    }

    pub fn select_table(&self, cx: &mut gpui::App, table: &str) {
        if self.active_table.get(cx).as_deref() == Some(table) {
            if self.active_tab.get(cx) == WorkspaceTab::Query {
                self.active_tab.set(cx, WorkspaceTab::Data);
            }
            return;
        }
        self.save_table(cx);
        let session = self.sessions.borrow_mut().remove(table).unwrap_or_default();
        self.page.set(cx, session.page.max(1));
        self.sort.set(cx, session.sort);
        self.selection.set(cx, BTreeSet::new());
        self.restoring_selection.set(cx, session.selection);
        self.hidden_columns.set(cx, session.hidden_columns);
        self.draft_filters.set(cx, session.draft_filters);
        self.applied_filters.set(cx, session.applied_filters);
        self.filter_panel_open.set(cx, session.filter_open);
        self.inspector_open.set(cx, session.inspector_open);
        self.pending.set(cx, session.pending);
        self.rows.set(cx, None);
        self.active_table.set(cx, Some(table.to_string()));
        self.active_tab.set(
            cx,
            if session.structure {
                WorkspaceTab::Structure
            } else {
                WorkspaceTab::Data
            },
        );
        self.open_tables.update(cx, |tables| {
            if !tables.iter().any(|t| t == table) {
                tables.push(table.to_string());
            }
        });
        self.bump_rows(cx);
    }

    fn save_table(&self, cx: &gpui::App) {
        if let Some(table) = self.active_table.get(cx) {
            self.sessions.borrow_mut().insert(
                table,
                TableSession {
                    page: self.page.get(cx),
                    sort: self.sort.get(cx),
                    selection: self
                        .rows
                        .read(cx)
                        .as_ref()
                        .map(|r| {
                            self.selection
                                .read(cx)
                                .iter()
                                .filter_map(|i| r.rows.get(*i).cloned())
                                .collect()
                        })
                        .unwrap_or_default(),
                    hidden_columns: self.hidden_columns.get(cx),
                    draft_filters: self.draft_filters.get(cx),
                    applied_filters: self.applied_filters.get(cx),
                    filter_open: self.filter_panel_open.get(cx),
                    inspector_open: self.inspector_open.get(cx),
                    pending: self.pending.get(cx),
                    structure: self.active_tab.get(cx) == WorkspaceTab::Structure,
                },
            );
        }
    }

    pub fn table_dirty(&self, table: &str, cx: &gpui::App) -> bool {
        if self.active_table.get(cx).as_deref() == Some(table) {
            !self.pending.read(cx).is_empty()
        } else {
            self.sessions
                .borrow()
                .get(table)
                .is_some_and(|s| !s.pending.is_empty())
        }
    }

    pub fn close_table(&self, table: &str, cx: &mut gpui::App) {
        if self.table_dirty(table, cx) {
            return;
        }
        self.open_tables
            .update(cx, |tables| tables.retain(|t| t != table));
        self.sessions.borrow_mut().remove(table);
        if self.active_table.get(cx).as_deref() == Some(table) {
            self.active_table.set(cx, None);
            self.rows.set(cx, None);
            self.pending.set(cx, Vec::new());
            let next = self.open_tables.read(cx).last().cloned();
            if let Some(next) = next {
                self.select_table(cx, &next);
            } else {
                self.active_tab.set(cx, WorkspaceTab::Query);
                self.bump_rows(cx);
            }
        }
    }

    pub fn finish_commit(&self, table: &str, changes: &[PendingChange], cx: &mut gpui::App) {
        if self.active_table.get(cx).as_deref() == Some(table) {
            self.pending
                .update(cx, |pending| pending.retain(|p| !changes.contains(p)));
            self.bump_rows(cx);
        } else if let Some(session) = self.sessions.borrow_mut().get_mut(table) {
            session.pending.retain(|p| !changes.contains(p));
        }
    }

    pub fn reset_tables(&self, cx: &mut gpui::App) {
        self.sessions.borrow_mut().clear();
        self.open_tables.set(cx, Vec::new());
        self.active_table.set(cx, None);
        self.rows.set(cx, None);
        self.pending.set(cx, Vec::new());
        self.bump_rows(cx);
    }

    /// FK navigation: same resets, but pre-seeded with one `=` filter and the
    /// panel open.
    pub fn navigate_fk(&self, cx: &mut gpui::App, table: &str, column: &str, value: &str) {
        self.select_table(cx, table);
        let filter = FilterState {
            conditions: vec![FilterCondition {
                id: model::new_uuid(),
                column: column.to_string(),
                operator: "=".into(),
                value: value.to_string(),
                value2: None,
            }],
            logic: "and".into(),
        };
        self.page.set(cx, 1);
        self.sort.set(cx, None);
        self.selection.set(cx, BTreeSet::new());
        self.hidden_columns.set(cx, BTreeSet::new());
        self.draft_filters.set(cx, filter.clone());
        self.applied_filters.set(cx, filter);
        self.filter_panel_open.set(cx, true);
        self.active_table.set(cx, Some(table.to_string()));
        self.active_tab.set(cx, WorkspaceTab::Data);
        self.bump_rows(cx);
    }

    pub fn bump_rows(&self, cx: &mut gpui::App) {
        self.rows_epoch.update(cx, |n| *n += 1);
    }

    pub fn bump_tables(&self, cx: &mut gpui::App) {
        self.tables_epoch.update(cx, |n| *n += 1);
    }
}

#[derive(Default)]
struct TableSession {
    page: u64,
    sort: Option<SortSpec>,
    selection: Vec<model::Row>,
    hidden_columns: BTreeSet<String>,
    draft_filters: FilterState,
    applied_filters: FilterState,
    filter_open: bool,
    inspector_open: bool,
    pending: Vec<PendingChange>,
    structure: bool,
}
