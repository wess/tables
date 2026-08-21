//! The UI tree. `Root` owns routing (home ⇄ workspace), constructs the host
//! facade, installs the app-wide context (`AppState`), and hosts the toast stack
//! that floats above every page.

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{div, Context, Entity, FocusHandle, Focusable, Window};
use guise::prelude::*;

use crate::about::AboutSheet;
use crate::bridge;
use crate::home::Home;
use crate::settings::{SettingsEvent, SettingsModal};
use crate::state::{AppState, Route};
use crate::toasts::Toasts;
use crate::workspace::Workspace;
use host::Host;

pub struct Root {
    state: AppState,
    home: Entity<Home>,
    /// The live workspace, kept keyed by connection id so switching back to an
    /// already-open connection reuses its view instead of rebuilding it.
    workspace: Option<(String, Entity<Workspace>)>,
    toast_stack: Entity<ToastStack>,
    about_open: bool,
    /// Settings are app-wide, so they open from Home as readily as from a
    /// workspace — which is why the modal lives here and not in one route.
    settings_modal: Option<Entity<SettingsModal>>,
    /// The window's fallback focus.
    ///
    /// gpui dispatches an action along the focus path, so an action registered
    /// on an element is only reachable when something is focused. With nothing
    /// ever focused the whole menu bar greys out and its shortcuts do nothing —
    /// which is what happened to Settings… and About Tables. The root holds
    /// focus whenever it would otherwise go nowhere.
    focus: FocusHandle,
}

impl Focusable for Root {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Root {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let host = Arc::new(Host::new());
        let settings = host.settings();
        // Materialize settings.json on first run so there is a real file to edit
        // (the loader otherwise merges defaults in memory without writing).
        if host.settings_raw().is_none() {
            if let Ok(value) = serde_json::to_value(&settings) {
                host.save_settings(&value);
            }
        }
        let settings_auto_update = settings.auto_update;
        let state = AppState {
            host,
            route: Signal::new(cx, Route::Home),
            settings: Signal::new(cx, settings),
            toasts: Toasts::new(cx),
        };
        provide(cx, state.clone());
        watch(cx, &state.route);

        // Only the check is automatic; installing is always an explicit click.
        if settings_auto_update {
            crate::update::start(cx);
        }

        let toast_stack = state.toasts.stack();
        let home = cx.new(Home::new);
        Root {
            state,
            home,
            workspace: None,
            toast_stack,
            about_open: false,
            settings_modal: None,
            focus: cx.focus_handle(),
        }
    }
}

impl Root {
    fn open_settings(&mut self, cx: &mut Context<Self>) {
        let modal = cx.new(SettingsModal::new);
        cx.subscribe(&modal, |this, _modal, _event: &SettingsEvent, cx| {
            this.settings_modal = None;
            cx.notify();
        })
        .detach();
        self.settings_modal = Some(modal);
        cx.notify();
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Claim focus when nothing else holds it — on the first frame, and
        // again whenever a focused view goes away.
        if window.focused(cx).is_none() {
            window.focus(&self.focus);
        }

        let t = cx.global::<Theme>();
        let body = t.body().hsla();
        let text = t.text().hsla();
        let font = t.font_family.clone();

        let mut root = div()
            .track_focus(&self.focus)
            .key_context("Tables")
            .relative()
            .size_full()
            .bg(body)
            .text_color(text)
            .font_family(font)
            // File → New Connection / cmd-n: go home and open the form.
            .on_action(cx.listener(|this, _: &crate::NewConnection, _, cx| {
                this.state.route.set(cx, Route::Home);
                this.home.update(cx, |home, cx| home.open_form(None, cx));
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &crate::ShowAbout, _, cx| {
                this.about_open = true;
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &crate::OpenSettings, _, cx| this.open_settings(cx)));

        match self.state.route.get(cx) {
            Route::Home => {
                root = root.child(self.home.clone());
            }
            Route::Workspace(id) => {
                let stale = self.workspace.as_ref().map(|(wid, _)| wid != &id).unwrap_or(true);
                if stale {
                    // Replacing the cached workspace: disconnect the one we're
                    // dropping so its health monitor and pooled connection don't
                    // linger after its view is gone.
                    if let Some((old_id, _)) = self.workspace.take() {
                        if old_id != id {
                            let host = self.state.host.clone();
                            bridge::run(cx, async move { host.disconnect(&old_id).await }, |_, _| {});
                        }
                    }
                    let for_view = id.clone();
                    let view = cx.new(|cx| Workspace::new(for_view, cx));
                    self.workspace = Some((id.clone(), view));
                }
                root = root.child(self.workspace.as_ref().unwrap().1.clone());
            }
        }

        if let Some(modal) = &self.settings_modal {
            root = root.child(modal.clone());
        }

        if self.about_open {
            root = root.child(AboutSheet::new().on_close(cx.listener(|this, _, _, cx| {
                this.about_open = false;
                cx.notify();
            })));
        }

        root.child(self.toast_stack.clone())
    }
}
