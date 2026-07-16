//! Query-tab data actions: toggling the history/favorites side panel, loading
//! those lists, loading a chosen SQL into the editor, and saving/deleting
//! favorites.

use super::{QueryPanel, Side};
use crate::bridge;
use crate::state::AppState;

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

    pub(super) fn delete_favorite(&self, id: String, cx: &mut gpui::App) {
        let host = self.app.host.clone();
        let favorites = self.favorites.clone();
        bridge::run(
            cx,
            async move {
                let _ = host.delete_favorite(&id);
                host.list_favorites().unwrap_or_default()
            },
            move |list, cx| favorites.set(cx, list),
        );
    }
}
