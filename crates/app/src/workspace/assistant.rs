//! The AI assistant: a slide-out right column that chats with Claude about the
//! connected database. It streams tokens live through `bridge::stream`, seeds a
//! system prompt from the workspace schema, and resolves its credential (API key
//! or subscription token) from the OS keychain via the host.

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, Window};
use guise::prelude::*;

use crate::bridge;
use crate::state::{AppState, WorkspaceState};
use ai::{AiConfig, AuthMode, Message as AiMessage, Role, StreamEvent};

#[derive(Clone)]
struct ChatMsg {
    role: Role,
    text: String,
}

pub struct AssistantPanel {
    app: AppState,
    state: WorkspaceState,
    input: Entity<TextInput>,
    messages: Signal<Vec<ChatMsg>>,
    streaming: Signal<bool>,
}

impl AssistantPanel {
    pub fn new(app: AppState, state: WorkspaceState, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            TextInput::new(cx).placeholder("Ask about your data…  (⏎ to send)")
        });
        let messages = Signal::new(cx, Vec::new());
        let streaming = Signal::new(cx, false);
        watch(cx, &messages);
        watch(cx, &streaming);

        cx.subscribe(&input, |this, _input, event: &TextInputEvent, cx| {
            if let TextInputEvent::Submit(text) = event {
                this.send(text.clone(), cx);
            }
        })
        .detach();

        AssistantPanel { app, state, input, messages, streaming }
    }

    fn send_from_input(&self, cx: &mut gpui::App) {
        let text = self.input.read(cx).text();
        self.send(text, cx);
    }

    fn send(&self, prompt: String, cx: &mut gpui::App) {
        let prompt = prompt.trim().to_string();
        if prompt.is_empty() || *self.streaming.read(cx) {
            return;
        }

        let settings = self.app.settings.get(cx);
        let auth = AuthMode::parse(&settings.ai_auth_mode);
        let Some(credential) = self.app.host.ai_secret(&settings.ai_auth_mode) else {
            self.app.toasts.error(
                cx,
                "No AI credential",
                "Add an API key or subscription token in Settings (⚙) → AI Assistant.",
            );
            return;
        };
        let config = AiConfig { model: settings.ai_model.clone(), auth };

        // The request is the prior turns plus this prompt.
        let mut history: Vec<AiMessage> = self
            .messages
            .get(cx)
            .iter()
            .map(|m| AiMessage { role: m.role, text: m.text.clone() })
            .collect();
        history.push(AiMessage { role: Role::User, text: prompt.clone() });
        let system = self.system_prompt(cx);

        // Show the user turn and an empty assistant turn to stream into.
        self.messages.update(cx, |list| {
            list.push(ChatMsg { role: Role::User, text: prompt });
            list.push(ChatMsg { role: Role::Assistant, text: String::new() });
        });
        self.input.update(cx, |i, cx| i.set_text("", cx));
        self.streaming.set(cx, true);

        let messages = self.messages.clone();
        let streaming = self.streaming.clone();
        bridge::stream(
            cx,
            move |tx| ai::stream_chat(config, credential, Some(system), history, tx),
            move |event, cx| match event {
                StreamEvent::Delta(text) => messages.update(cx, |list| {
                    if let Some(last) = list.last_mut() {
                        last.text.push_str(&text);
                    }
                }),
                StreamEvent::Error(error) => messages.update(cx, |list| {
                    if let Some(last) = list.last_mut() {
                        last.text = format!("⚠ {error}");
                    }
                }),
            },
            move |cx| streaming.set(cx, false),
        );
    }

    /// Seed the model with the connection's dialect and table list so it can
    /// write dialect-correct SQL.
    fn system_prompt(&self, cx: &gpui::App) -> String {
        let dialect = self
            .state
            .connection
            .get(cx)
            .as_ref()
            .map(|c| dialect_label(&c.kind))
            .unwrap_or("SQL");
        let names: Vec<String> =
            self.state.tables.get(cx).iter().map(|t| t.name.clone()).collect();
        let schema = if names.is_empty() { "(no tables loaded)".to_string() } else { names.join(", ") };
        format!(
            "You are the SQL assistant built into Tables, a database client. The user is \
             connected to a {dialect} database. Available tables: {schema}. Help the user \
             write and understand SQL for this dialect. Keep answers concise and put runnable \
             SQL in fenced ```sql code blocks. If you are unsure of a table's exact columns, \
             say so rather than guessing."
        )
    }

    fn clear(&self, cx: &mut gpui::App) {
        self.messages.set(cx, Vec::new());
    }
}

fn dialect_label(kind: &str) -> &'static str {
    match kind {
        "postgres" => "PostgreSQL",
        "mysql" => "MySQL",
        "sqlite" => "SQLite",
        _ => "SQL",
    }
}

impl Render for AssistantPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = crate::theme::palette(cx);
        let streaming = *self.streaming.read(cx);
        let messages = self.messages.get(cx);
        let has_key = self.app.host.has_ai_secret(&self.app.settings.get(cx).ai_auth_mode);

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(colors.border)
            .child(Text::new("AI Assistant").size(Size::Xs).medium())
            .child(
                Button::new("ai-clear", "Clear")
                    .size(Size::Xs)
                    .variant(Variant::Subtle)
                    .disabled(messages.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
            );

        let body = if messages.is_empty() {
            let hint = if has_key {
                "Ask me to write a query, explain a table, or debug SQL."
            } else {
                "Add an AI key in Settings (⚙) → AI Assistant to get started."
            };
            Center::new()
                .child(Text::new(hint).size(Size::Xs).dimmed())
                .into_any_element()
        } else {
            let mut stack = Stack::new().gap(Size::Sm);
            for (i, msg) in messages.iter().enumerate() {
                let is_user = msg.role == Role::User;
                let label = if is_user { "You" } else { "Assistant" };
                let showing = if msg.text.is_empty() && streaming && i + 1 == messages.len() {
                    "▍".to_string()
                } else {
                    msg.text.clone()
                };
                stack = stack.child(
                    div()
                        .p(px(8.0))
                        .rounded(px(6.0))
                        .bg(colors.bg_muted)
                        .child(
                            Text::new(label)
                                .size(Size::Xs)
                                .when(is_user, |t| t.medium())
                                .when(!is_user, |t| t.dimmed()),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .child(gpui::SharedString::from(showing)),
                        ),
                );
            }
            div()
                .id("ai-messages")
                .size_full()
                .overflow_y_scroll()
                .p(px(8.0))
                .child(stack)
                .into_any_element()
        };

        let composer = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(8.0))
            .py(px(6.0))
            .border_t_1()
            .border_color(colors.border)
            .child(div().flex_1().child(self.input.clone()))
            .child(
                Button::new("ai-send", if streaming { "…" } else { "Send" })
                    .size(Size::Xs)
                    .disabled(streaming)
                    .on_click(cx.listener(|this, _, _, cx| this.send_from_input(cx))),
            );

        div()
            .flex()
            .flex_col()
            .w(px(340.0))
            .h_full()
            .flex_none()
            .border_l_1()
            .border_color(colors.border)
            .bg(colors.bg_surface)
            .child(header)
            .child(div().flex_1().min_h(px(0.0)).child(body))
            .child(composer)
    }
}
