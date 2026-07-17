//! The session/process manager modal: lists live server sessions and kills one.

use gpui::prelude::*;
use gpui::{div, px, Context, EventEmitter, SharedString, Window};
use guise::prelude::*;

use crate::bridge;
use crate::state::AppState;
use model::Session;

pub enum SessionsEvent {
    Close,
}

pub struct SessionsModal {
    app: AppState,
    sessions: Signal<Vec<Session>>,
    loading: Signal<bool>,
}

impl EventEmitter<SessionsEvent> for SessionsModal {}

impl SessionsModal {
    pub fn new(app: AppState, cx: &mut Context<Self>) -> Self {
        let sessions = Signal::new(cx, Vec::new());
        let loading = Signal::new(cx, true);
        watch(cx, &sessions);
        watch(cx, &loading);
        let modal = SessionsModal { app, sessions, loading };
        modal.refresh(cx);
        modal
    }

    fn refresh(&self, cx: &mut gpui::App) {
        self.loading.set(cx, true);
        let host = self.app.host.clone();
        let out = self.sessions.clone();
        let loading = self.loading.clone();
        let toasts = self.app.toasts.clone();
        bridge::run(
            cx,
            async move { host.sessions().await },
            move |result, cx| {
                loading.set(cx, false);
                match result {
                    Ok(list) => out.set(cx, list),
                    Err(error) => toasts.error(cx, "Sessions failed", &error),
                }
            },
        );
    }

    fn kill(&self, id: String, cx: &mut gpui::App) {
        let host = self.app.host.clone();
        let toasts = self.app.toasts.clone();
        let sessions = self.sessions.clone();
        let loading = self.loading.clone();
        loading.set(cx, true);
        bridge::run(
            cx,
            async move {
                let killed = host.kill_session(&id).await;
                let list = host.sessions().await.unwrap_or_default();
                (killed, list)
            },
            move |(killed, list), cx| {
                loading.set(cx, false);
                sessions.set(cx, list);
                match killed {
                    Ok(_) => toasts.success(cx, "Session killed", 1500),
                    Err(e) => toasts.error(cx, "Kill failed", &e),
                }
            },
        );
    }
}

fn cell(text: String, width: f32) -> impl IntoElement {
    div()
        .flex_none()
        .w(px(width))
        .px(px(4.0))
        .text_size(px(11.0))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(SharedString::from(text))
}

impl Render for SessionsModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let loading = *self.loading.read(cx);
        let sessions = self.sessions.read(cx);

        let header = div()
            .flex()
            .items_center()
            .px(px(6.0))
            .py(px(4.0))
            .border_b_1()
            .border_color(colors.border)
            .child(cell("PID".into(), 60.0))
            .child(cell("User".into(), 90.0))
            .child(cell("DB".into(), 90.0))
            .child(cell("State".into(), 110.0))
            .child(cell("Time".into(), 60.0))
            .child(cell("Query".into(), 260.0))
            .child(div().w(px(48.0)));

        let body: gpui::AnyElement = if loading {
            Center::new().child(Loader::new().size(Size::Sm)).into_any_element()
        } else if sessions.is_empty() {
            div()
                .p(px(12.0))
                .child(Text::new("No active sessions").size(Size::Sm).dimmed())
                .into_any_element()
        } else {
            let mut list = div().flex().flex_col();
            for s in sessions.iter() {
                let id = s.id.clone();
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .px(px(6.0))
                        .py(px(2.0))
                        .border_b_1()
                        .border_color(colors.border_subtle)
                        .child(cell(s.id.clone(), 60.0))
                        .child(cell(s.user.clone(), 90.0))
                        .child(cell(s.database.clone(), 90.0))
                        .child(cell(s.state.clone(), 110.0))
                        .child(cell(s.duration.clone(), 60.0))
                        .child(cell(s.query.clone(), 260.0))
                        .child(
                            div().w(px(48.0)).flex().justify_end().child(
                                Button::new(SharedString::from(format!("kill-{}", s.id)), "Kill")
                                    .size(Size::Xs)
                                    .variant(Variant::Subtle)
                                    .color(ColorName::Red)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.kill(id.clone(), cx)
                                    })),
                            ),
                        ),
                );
            }
            list.into_any_element()
        };

        Modal::new()
            .title("Sessions")
            .width(760.0)
            .on_close(cx.listener(|_, _, _, cx| cx.emit(SessionsEvent::Close)))
            .child(
                Group::new().justify(Justify::End).child(
                    Button::new("sessions-refresh", if loading { "Refreshing…" } else { "Refresh" })
                        .size(Size::Xs)
                        .variant(Variant::Subtle)
                        .disabled(loading)
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                ),
            )
            .child(Divider::new())
            .child(header)
            .child(
                div()
                    .id("sessions-scroll")
                    .max_h(px(420.0))
                    .overflow_y_scroll()
                    .child(body),
            )
    }
}
