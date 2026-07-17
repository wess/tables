//! Query-tab data actions: toggling the history/favorites side panel, loading
//! those lists, loading a chosen SQL into the editor, and saving/deleting
//! favorites.

use super::{QueryPanel, Side};
use crate::bridge;
use crate::state::AppState;
use model::Row;

/// The export format implied by a save-path extension (default CSV).
fn format_for_path(path: &str) -> &'static str {
    if path.ends_with(".json") {
        "json"
    } else if path.ends_with(".sql") {
        "sql"
    } else if path.ends_with(".md") || path.ends_with(".markdown") {
        "markdown"
    } else if path.ends_with(".tsv") {
        "tsv"
    } else {
        "csv"
    }
}

impl QueryPanel {
    pub(super) fn toggle_side(&self, side: Side, cx: &mut gpui::App) {
        let current = *self.side.read(cx);
        if current == Some(side) {
            self.side.set(cx, None);
            return;
        }
        self.side.set(cx, Some(side));
        match side {
            Side::History => self.load_history(cx),
            Side::Favorites => self.load_favorites(cx),
        }
    }

    fn load_history(&self, cx: &mut gpui::App) {
        let host = self.app.host.clone();
        let out = self.history.clone();
        let toasts = self.app.toasts.clone();
        bridge::run(
            cx,
            async move { host.query_history() },
            move |result, cx| match result {
                Ok(list) => out.set(cx, list),
                Err(error) => toasts.error(cx, "History failed", &error),
            },
        );
    }

    fn load_favorites(&self, cx: &mut gpui::App) {
        let host = self.app.host.clone();
        let out = self.favorites.clone();
        let toasts = self.app.toasts.clone();
        bridge::run(
            cx,
            async move { host.list_favorites() },
            move |result, cx| match result {
                Ok(list) => out.set(cx, list),
                Err(error) => toasts.error(cx, "Favorites failed", &error),
            },
        );
    }

    pub(super) fn load_sql(&self, sql: &str, cx: &mut gpui::App) {
        let sql = sql.to_string();
        self.editor.update(cx, |editor, cx| editor.set_text(&sql, cx));
    }

    pub(super) fn save_current_favorite(&self, cx: &mut gpui::App) {
        let name = self.fav_name.read(cx).text();
        let sql = self.editor.read(cx).text();
        if name.trim().is_empty() || sql.trim().is_empty() {
            return;
        }
        let host = self.app.host.clone();
        let favorites = self.favorites.clone();
        let toasts = self.app.toasts.clone();
        let fav_name = self.fav_name.clone();
        bridge::run(
            cx,
            async move { host.save_favorite(None, &name, &sql) },
            move |result, cx| match result {
                Ok(_) => {
                    fav_name.update(cx, |i, cx| i.set_text("", cx));
                    toasts.success(cx, "Favorite saved", 1500);
                    let host = AppState::get(cx).host;
                    bridge::run(
                        cx,
                        async move { host.list_favorites().unwrap_or_default() },
                        move |list, cx| favorites.set(cx, list),
                    );
                }
                Err(error) => toasts.error(cx, "Save failed", &error),
            },
        );
    }

    /// The first result set with data — the target of export/copy actions.
    fn first_result(&self, cx: &gpui::App) -> Option<(Vec<String>, Vec<Row>)> {
        self.results
            .read(cx)
            .iter()
            .find(|r| !r.columns.is_empty() && !r.rows.is_empty())
            .map(|r| (r.columns.clone(), r.rows.clone()))
    }

    /// Export the current result set to a file (format inferred from extension).
    pub(super) fn export_results(&self, cx: &mut gpui::App) {
        let Some((columns, rows)) = self.first_result(cx) else {
            return;
        };
        let dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let rx = cx.prompt_for_new_path(&dir, Some("query.csv"));
        let host = self.app.host.clone();
        let toasts = self.app.toasts.clone();
        cx.spawn(async move |cx| {
            let Ok(Ok(Some(path))) = rx.await else {
                return;
            };
            let path = path.to_string_lossy().into_owned();
            let format = format_for_path(&path);
            let _ = cx.update(|cx| {
                bridge::run(
                    cx,
                    async move { host.write_result(&columns, &rows, format, &path, None) },
                    move |result, cx| match result {
                        Ok(n) => toasts.success(cx, &format!("Exported {n} row(s)"), 2000),
                        Err(e) => toasts.error(cx, "Export failed", &e),
                    },
                );
            });
        })
        .detach();
    }

    /// Copy the current result set to the clipboard as a Markdown table.
    pub(super) fn copy_results_markdown(&self, cx: &mut gpui::App) {
        let Some((columns, rows)) = self.first_result(cx) else {
            return;
        };
        match self.app.host.serialize_result(&columns, &rows, "markdown", None) {
            Ok(md) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(md));
                self.app.toasts.success(cx, "Copied as Markdown", 1500);
            }
            Err(e) => self.app.toasts.error(cx, "Copy failed", &e),
        }
    }

    pub(super) fn delete_favorite(&self, id: String, cx: &mut gpui::App) {
        let host = self.app.host.clone();
        let favorites = self.favorites.clone();
        let toasts = self.app.toasts.clone();
        bridge::run(
            cx,
            async move {
                let result = host.delete_favorite(&id);
                (result, host.list_favorites().unwrap_or_default())
            },
            move |(result, list), cx| {
                favorites.set(cx, list);
                match result {
                    Ok(_) => toasts.success(cx, "Favorite deleted", 1500),
                    Err(error) => toasts.error(cx, "Delete failed", &error),
                }
            },
        );
    }
}
