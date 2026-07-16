//! Data-tab write and transfer handlers: staged deletes, insert, mock-data,
//! commit/discard of the reviewed batch, clipboard copy, and CSV import/export.

use std::collections::BTreeSet;

use gpui::prelude::*;
use gpui::Context;

use super::DataPanel;
use crate::bridge;
use crate::state::PendingChange;
use crate::workspace::insert::{InsertEvent, InsertModal};

impl DataPanel {
    pub(super) fn delete_selected(&self, cx: &mut gpui::App) {
        let selection = self.state.selection.get(cx);
        if selection.is_empty() {
            return;
        }
        let Some(response) = self.state.rows.get(cx) else {
            return;
        };
        let table = self.state.active_table.get(cx).unwrap_or_default();
        let deletes: Vec<PendingChange> = selection
            .iter()
            .filter_map(|idx| response.rows.get(*idx))
            .map(|row| PendingChange::Delete { table: table.clone(), primary_key: row.clone() })
            .collect();
        self.state.pending.update(cx, move |pending| pending.extend(deletes));
        self.state.selection.set(cx, BTreeSet::new());
    }

    pub(super) fn open_insert(&mut self, cx: &mut Context<Self>) {
        let Some(response) = self.state.rows.get(cx) else {
            return;
        };
        let columns = response.columns.clone();
        let table = self.state.active_table.get(cx).unwrap_or_default();
        let modal = cx.new(|cx| InsertModal::new(table, columns, cx));
        cx.subscribe(&modal, |this, _modal, event: &InsertEvent, cx| match event {
            InsertEvent::Cancel => {
                this.insert = None;
                cx.notify();
            }
            InsertEvent::Submit(row) => {
                let row = row.clone();
                let table = this.state.active_table.get(cx).unwrap_or_default();
                let host = this.app.host.clone();
                let toasts = this.app.toasts.clone();
                let state = this.state.clone();
                bridge::run(
                    cx,
                    async move { host.row_insert(&table, &row).await },
                    move |result, cx| match result {
                        Ok(_) => {
                            state.bump_rows(cx);
                            toasts.success(cx, "Row inserted", 1500);
                        }
                        Err(error) => toasts.error(cx, "Insert failed", &error),
                    },
                );
                this.insert = None;
                cx.notify();
            }
        })
        .detach();
        self.insert = Some(modal);
        cx.notify();
    }

    pub(super) fn generate_data(&self, cx: &mut gpui::App) {
        let Some(table) = self.state.active_table.get(cx) else {
            return;
        };
        let host = self.app.host.clone();
        let toasts = self.app.toasts.clone();
        let state = self.state.clone();
        bridge::run(
            cx,
            async move { host.mock_data(&table, 50).await },
            move |result, cx| match result {
                Ok(outcome) => {
                    state.bump_rows(cx);
                    match outcome.error {
                        Some(error) => toasts.warn(
                            cx,
                            "Partial generation",
                            &format!("{}/{} rows. {error}", outcome.inserted, outcome.total),
                        ),
                        None => toasts.success(
                            cx,
                            &format!("{} mock rows generated", outcome.inserted),
                            2000,
                        ),
                    }
                }
                Err(error) => toasts.error(cx, "Generation failed", &error),
            },
        );
    }

    pub(super) fn commit(&mut self, cx: &mut Context<Self>) {
        let changes = self.state.pending.get(cx);
        if changes.is_empty() {
            return;
        }
        let count = changes.len();
        self.committing.set(cx, true);
        let host = self.app.host.clone();
        let pending = self.state.pending.clone();
        let committing = self.committing.clone();
        let toasts = self.app.toasts.clone();
        let state = self.state.clone();
        // Map the pending changes to a batch applied atomically by the host.
        let writes: Vec<model::RowWrite> = changes
            .iter()
            .map(|change| match change {
                PendingChange::Update { table, primary_key, changes } => model::RowWrite::Update {
                    table: table.clone(),
                    primary_key: primary_key.clone(),
                    changes: changes.clone(),
                },
                PendingChange::Insert { table, row } => {
                    model::RowWrite::Insert { table: table.clone(), row: row.clone() }
                }
                PendingChange::Delete { table, primary_key } => model::RowWrite::Delete {
                    table: table.clone(),
                    primary_key: primary_key.clone(),
                },
            })
            .collect();
        bridge::run(
            cx,
            async move { host.apply_row_writes(&writes).await.map(|_| ()) },
            move |result, cx| {
                committing.set(cx, false);
                match result {
                    Ok(_) => {
                        pending.set(cx, Vec::new());
                        state.bump_rows(cx);
                        toasts.success(cx, &format!("{count} change(s) committed"), 2000);
                    }
                    Err(error) => toasts.error(cx, "Commit failed", &error),
                }
            },
        );
        self.show_review = false;
        cx.notify();
    }

    pub(super) fn discard(&mut self, cx: &mut Context<Self>) {
        self.state.pending.set(cx, Vec::new());
        self.show_review = false;
        cx.notify();
    }

    /// Copy the selected rows to the clipboard as TSV (visible column order).
    pub(super) fn copy_selection(&self, cx: &mut gpui::App) {
        let selection = self.state.selection.get(cx);
        let Some(response) = self.state.rows.get(cx) else {
            return;
        };
        if selection.is_empty() {
            return;
        }
        let mut lines = Vec::new();
        for idx in &selection {
            if let Some(row) = response.rows.get(*idx) {
                let cells: Vec<String> = response
                    .columns
                    .iter()
                    .map(|c| crate::workspace::cell_text(row.get(c), ""))
                    .collect();
                lines.push(cells.join("\t"));
            }
        }
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(lines.join("\n")));
        self.app
            .toasts
            .success(cx, &format!("{} row(s) copied", selection.len()), 1500);
    }

    /// Import a CSV/TSV file into the active table (native file picker).
    pub(super) fn import_csv(&self, cx: &mut gpui::App) {
        let Some(table) = self.state.active_table.get(cx) else {
            return;
        };
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        let host = self.app.host.clone();
        let state = self.state.clone();
        let toasts = self.app.toasts.clone();
        cx.spawn(async move |cx| {
            let Ok(Ok(Some(paths))) = rx.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let path = path.to_string_lossy().into_owned();
            let _ = cx.update(|cx| {
                bridge::run(
                    cx,
                    async move { host.import_csv_file(&table, &path).await },
                    move |result, cx| match result {
                        Ok(r) => {
                            state.bump_rows(cx);
                            match r.error {
                                Some(e) => toasts.warn(
                                    cx,
                                    "Partial import",
                                    &format!("{}/{} rows. {e}", r.inserted, r.total),
                                ),
                                None => toasts.success(
                                    cx,
                                    &format!("{} row(s) imported", r.inserted),
                                    2000,
                                ),
                            }
                        }
                        Err(e) => toasts.error(cx, "Import failed", &e),
                    },
                );
            });
        })
        .detach();
    }

    /// Export the active table to a file (format inferred from extension).
    pub(super) fn export_table(&self, cx: &mut gpui::App) {
        let Some(table) = self.state.active_table.get(cx) else {
            return;
        };
        let dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let rx = cx.prompt_for_new_path(&dir, Some(&format!("{table}.csv")));
        let host = self.app.host.clone();
        let toasts = self.app.toasts.clone();
        cx.spawn(async move |cx| {
            let Ok(Ok(Some(path))) = rx.await else {
                return;
            };
            let path = path.to_string_lossy().into_owned();
            let format = if path.ends_with(".json") {
                "json"
            } else if path.ends_with(".sql") {
                "sql"
            } else {
                "csv"
            }
            .to_string();
            let _ = cx.update(|cx| {
                bridge::run(
                    cx,
                    async move {
                        host.export_file(&table, &format, Some(&path), &serde_json::Map::new()).await
                    },
                    move |result, cx| match result {
                        Ok(r) => toasts.success(cx, &format!("Exported {} row(s)", r.rows), 2000),
                        Err(e) => toasts.error(cx, "Export failed", &e),
                    },
                );
            });
        })
        .detach();
    }
}
