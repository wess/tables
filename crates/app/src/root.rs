//! The UI tree. `Root` owns routing (home ⇄ workspace), constructs the host
//! facade, installs the app-wide context (`AppState`), and hosts the toast stack
//! that floats above every page.

use std::sync::Arc;

use gpui::prelude::*;
use gpui::{div, Context, Entity, Window};
use guise::prelude::*;

use crate::bridge;
use crate::home::Home;
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
        let state = AppState {
            host,
            route: Signal::new(cx, Route::Home),
            settings: Signal::new(cx, settings),
            toasts: Toasts::new(cx),
        };
        provide(cx, state.clone());
        watch(cx, &state.route);

        let toast_stack = state.toasts.stack();
        let home = cx.new(Home::new);
        Root { state, home, workspace: None, toast_stack }
    }
}

impl Render for Root {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<Theme>();
        let body = t.body().hsla();
        let text = t.text().hsla();
        let font = t.font_family.clone();

        let mut root = div()
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
            }));

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

        root.child(self.toast_stack.clone())
    }
}
