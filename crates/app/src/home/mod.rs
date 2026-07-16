//! The home page — the connection list. Loads the stored connections through
//! the host, groups them, and drives new / edit / delete / connect.

pub mod card;
pub mod form;

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, Window};
use guise::prelude::*;

use crate::bridge;
use crate::home::form::{ConnectionForm, ConnectionFormEvent};
use crate::state::{AppState, Route};
use host::Host;
use model::StoredConnection;

pub struct Home {
    pub(super) state: AppState,
    pub(super) connections: Signal<Vec<StoredConnection>>,
    pub(super) loading: Signal<bool>,
    pub(super) connecting: Signal<Option<String>>,
    form: Option<Entity<ConnectionForm>>,
    pending_delete: Option<StoredConnection>,
}

impl Home {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let state = AppState::get(cx);
        let connections = Signal::new(cx, Vec::new());
        let loading = Signal::new(cx, true);
        let connecting = Signal::new(cx, None);
        watch(cx, &connections);
        watch(cx, &loading);
        watch(cx, &connecting);

        let home = Home {
            state,
            connections,
            loading,
            connecting,
            form: None,
            pending_delete: None,
        };
        home.reload(cx);
        home
    }

    /// Refetch the stored connections off-thread.
    fn reload(&self, cx: &mut gpui::App) {
        reload_into(
            self.state.host.clone(),
            self.connections.clone(),
            self.loading.clone(),
            cx,
        );
    }

    /// Connect, then navigate to the workspace. A failure clears the spinner and
    /// toasts the error, staying on home.
    pub(super) fn connect(&self, id: String, cx: &mut gpui::App) {
        self.connecting.set(cx, Some(id.clone()));
        let host = self.state.host.clone();
        let route = self.state.route.clone();
        let connecting = self.connecting.clone();
        let toasts = self.state.toasts.clone();
        let target = id.clone();
        bridge::run(
            cx,
            async move { host.connect(&target).await },
            move |result, cx| {
                connecting.set(cx, None);
                match result {
                    Ok(_) => route.set(cx, Route::Workspace(id)),
                    Err(error) => toasts.error(cx, "Connection failed", &error),
                }
            },
        );
    }

    pub(super) fn open_form(&mut self, initial: Option<StoredConnection>, cx: &mut Context<Self>) {
        // Resolve the keychain-stored password so the edit form is prefilled.
        let initial = initial.map(|mut conn| {
            conn.password = self.state.host.resolve_password(&conn);
            conn
        });
        let form = cx.new(|cx| ConnectionForm::new(initial, cx));
        cx.subscribe(&form, |this, _form, event: &ConnectionFormEvent, cx| match event {
            ConnectionFormEvent::Cancel => {
                this.form = None;
                cx.notify();
            }
            ConnectionFormEvent::Save(conn) => {
                this.save((**conn).clone(), cx);
                this.form = None;
                cx.notify();
            }
        })
        .detach();
        self.form = Some(form);
        cx.notify();
    }

    fn save(&self, conn: StoredConnection, cx: &mut gpui::App) {
        let host = self.state.host.clone();
        let host_reload = host.clone();
        let connections = self.connections.clone();
        let loading = self.loading.clone();
        let toasts = self.state.toasts.clone();
        bridge::run(
            cx,
            async move { host.save_connection(&conn) },
            move |result, cx| match result {
                Ok(_) => reload_into(host_reload, connections, loading, cx),
                Err(error) => toasts.error(cx, "Save failed", &error),
            },
        );
    }

    pub(super) fn request_delete(&mut self, conn: StoredConnection, cx: &mut Context<Self>) {
        self.pending_delete = Some(conn);
        cx.notify();
    }

    fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(conn) = self.pending_delete.take() {
            let host = self.state.host.clone();
            let host_reload = host.clone();
            let connections = self.connections.clone();
            let loading = self.loading.clone();
            let id = conn.id.clone();
            bridge::run(
                cx,
                async move { host.delete_connection(&id).await },
                move |_, cx| reload_into(host_reload, connections, loading, cx),
            );
        }
        cx.notify();
    }
}

/// Load connections into `connections`, toggling `loading` around the fetch.
fn reload_into(
    host: Arc<Host>,
    connections: Signal<Vec<StoredConnection>>,
    loading: Signal<bool>,
    cx: &mut gpui::App,
) {
    loading.set(cx, true);
    bridge::run(
        cx,
        async move { host.list_connections() },
        move |list, cx| {
            connections.set(cx, list);
            loading.set(cx, false);
        },
    );
}

/// Group by `group` (falling back to "Connections"), preserving first-seen order.
fn grouped(conns: &[StoredConnection]) -> Vec<(String, Vec<StoredConnection>)> {
    let mut groups: Vec<(String, Vec<StoredConnection>)> = Vec::new();
    for conn in conns {
        let name = conn
            .group
            .clone()
            .filter(|g| !g.is_empty())
            .unwrap_or_else(|| "Connections".to_string());
        match groups.iter_mut().find(|(existing, _)| existing == &name) {
            Some((_, list)) => list.push(conn.clone()),
            None => groups.push((name, vec![conn.clone()])),
        }
    }
    groups
}

impl Render for Home {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div().id("home").relative().size_full().overflow_y_scroll();

        if *self.loading.read(cx) {
            return root.child(
                Center::new().child(
                    Stack::new()
                        .align(Align::Center)
                        .gap(Size::Sm)
                        .child(Loader::new().size(Size::Sm))
                        .child(Text::new("Loading connections...").size(Size::Sm).dimmed()),
                ),
            );
        }

        let conns = self.connections.get(cx);

        let header = Stack::new()
            .align(Align::Center)
            .gap(Size::Xs)
            .child(ThemeIcon::new("▦").color(ColorName::Blue).size(Size::Xl))
            .child(Text::new("Tables").size(Size::Lg).bold())
            .child(Text::new("Open-source database client").size(Size::Xs).dimmed());

        let new_button = Center::new().child(
            Button::new("new-connection", "New Connection")
                .variant(Variant::Light)
                .on_click(cx.listener(|this, _, _, cx| this.open_form(None, cx))),
        );

        let mut page = Stack::new().gap(Size::Lg).child(header).child(new_button);

        if conns.is_empty() {
            page = page.child(
                Center::new().child(
                    Stack::new()
                        .align(Align::Center)
                        .gap(Size::Xs)
                        .child(Text::new("No connections yet").size(Size::Sm).dimmed())
                        .child(
                            Group::new()
                                .align(Align::Center)
                                .gap(Size::Xs)
                                .child(Text::new("Press").size(Size::Xs).dimmed())
                                .child(Kbd::new("N"))
                                .child(
                                    Text::new("or click above to get started")
                                        .size(Size::Xs)
                                        .dimmed(),
                                ),
                        ),
                ),
            );
        } else {
            let groups = grouped(&conns);
            let multi = groups.len() > 1;
            let mut list = Stack::new().gap(Size::Md);
            for (name, group) in groups {
                let mut section = Stack::new().gap(Size::Xs);
                if multi {
                    section = section.child(Text::new(name.to_uppercase()).size(Size::Xs).dimmed());
                }
                let mut grid = SimpleGrid::new(3).spacing(Size::Sm);
                for conn in &group {
                    grid = grid.child(self.card(conn, cx));
                }
                list = list.child(section.child(grid));
            }
            page = page.child(Center::new().child(div().w_full().max_w(px(900.0)).child(list)));
        }

        root = root.child(div().w_full().px(px(24.0)).py(px(40.0)).child(page));

        if let Some(form) = &self.form {
            root = root.child(form.clone());
        }

        if let Some(conn) = &self.pending_delete {
            let name = if conn.name.is_empty() {
                "this connection".to_string()
            } else {
                conn.name.clone()
            };
            root = root.child(
                ConfirmModal::new()
                    .title("Delete Connection")
                    .message(format!(
                        "Are you sure you want to delete \"{name}\"? This cannot be undone."
                    ))
                    .confirm_label("Delete")
                    .cancel_label("Cancel")
                    .danger()
                    .on_confirm(cx.listener(|this, _, _, cx| this.confirm_delete(cx)))
                    .on_cancel(cx.listener(|this, _, _, cx| {
                        this.pending_delete = None;
                        cx.notify();
                    })),
            );
        }

        root
    }
}
