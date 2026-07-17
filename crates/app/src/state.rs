//! Cross-panel state contracts. `AppState` lives for the whole app and is
//! provided as context by `Root`; `WorkspaceState` lives while a connection
//! workspace is open. Panels read the signals they care about and `watch` them.

use std::collections::BTreeSet;
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

    // Data-tab state, reset when a table is selected.
    pub page: Signal<u64>,
    pub sort: Signal<Option<SortSpec>>,
    pub draft_filters: Signal<FilterState>,
    pub applied_filters: Signal<FilterState>,
    pub filter_panel_open: Signal<bool>,
    pub selection: Signal<BTreeSet<usize>>,
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
            active_tab: Signal::new(cx, WorkspaceTab::Data),
            page: Signal::new(cx, 1),
            sort: Signal::new(cx, None),
            draft_filters: Signal::new(cx, FilterState::default()),
            applied_filters: Signal::new(cx, FilterState::default()),
            filter_panel_open: Signal::new(cx, false),
            selection: Signal::new(cx, BTreeSet::new()),
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

    /// Selecting a table resets the data-tab state and activates Data.
    pub fn select_table(&self, cx: &mut gpui::App, table: &str) {
        self.page.set(cx, 1);
        self.sort.set(cx, None);
        self.selection.set(cx, BTreeSet::new());
        self.hidden_columns.set(cx, BTreeSet::new());
        self.draft_filters.set(cx, FilterState::default());
        self.applied_filters.set(cx, FilterState::default());
        self.active_table.set(cx, Some(table.to_string()));
        self.active_tab.set(cx, WorkspaceTab::Data);
        self.bump_rows(cx);
    }

    /// FK navigation: same resets, but pre-seeded with one `=` filter and the
    /// panel open.
    pub fn navigate_fk(&self, cx: &mut gpui::App, table: &str, column: &str, value: &str) {
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
