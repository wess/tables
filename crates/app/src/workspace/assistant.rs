//! The AI assistant: a slide-out right column that chats with Claude about the
//! connected database. It streams tokens live through `bridge::stream`, seeds a
//! system prompt from the workspace schema, and resolves its credential (API key
//! or subscription token) from the OS keychain via the host.
//!
//! The transcript is built from guise's `AIMessage` rather than its `AIChatView`
//! for one reason: a Tables reply is not only prose. A fenced `sql` block gets
//! Run and Insert buttons, and those are children of the message — which
//! `AIMessage` accepts and `AITurn` has nowhere to put. Everything else the
//! chat view would have provided is here explicitly: markdown bodies, a
//! streaming caret, the composer, and a cost meter fed by the API's own counts.

use gpui::prelude::*;
use gpui::{div, px, Context, Entity, EventEmitter, Window};
use guise::ai::format_cost;
use guise::prelude::*;

use crate::bridge;
use crate::state::{AppState, WorkspaceState};
use ai::{AiConfig, AuthMode, Message as AiMessage, Role, StreamEvent, Usage};

/// The assistant asks the workspace to act on a SQL block it produced.
pub enum AssistantEvent {
    /// Load the SQL into the Query editor and run it.
    RunSql(String),
    /// Load the SQL into the Query editor without running it.
    InsertSql(String),
}

#[derive(Clone)]
struct ChatMsg {
    role: Role,
    text: String,
}

/// A parsed piece of an assistant message: prose, or a fenced code block.
enum Segment {
    Text(String),
    Code { runnable: bool, code: String },
}

/// Split assistant text into prose and fenced code blocks. A block tagged `sql`
/// (or untagged) is `runnable`; another language is shown but not offered to
/// run. An unterminated block (still streaming) is not yet runnable.
fn segments(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut prose = String::new();
    let mut code: Option<(bool, String)> = None;
    for line in text.split_inclusive('\n') {
        let bare = line.trim_end_matches(['\n', '\r']);
        if let Some((_, acc)) = code.as_mut() {
            if bare.trim_start().starts_with("```") {
                let (runnable, acc) = code.take().unwrap();
                out.push(Segment::Code { runnable, code: acc.trim_end().to_string() });
            } else {
                acc.push_str(line);
            }
        } else if let Some(rest) = bare.trim_start().strip_prefix("```") {
            let prose = std::mem::take(&mut prose);
            if !prose.trim().is_empty() {
                out.push(Segment::Text(prose.trim().to_string()));
            }
            let lang = rest.trim().to_lowercase();
            code = Some((lang.is_empty() || lang == "sql", String::new()));
        } else {
            prose.push_str(line);
        }
    }
    // A block still open at the end is mid-stream: show it, don't offer to run.
    if let Some((_, acc)) = code {
        out.push(Segment::Code { runnable: false, code: acc.trim_end().to_string() });
    } else if !prose.trim().is_empty() {
        out.push(Segment::Text(prose.trim().to_string()));
    }
    out
}

/// Whether a reply has started a fenced block. While it hasn't, the whole body
/// can go through `AIMessage`'s own streaming renderer, caret and all.
fn has_fence(text: &str) -> bool {
    text.lines().any(|l| l.trim_start().starts_with("```"))
}

pub struct AssistantPanel {
    app: AppState,
    state: WorkspaceState,
    composer: Entity<AIComposer>,
    messages: Signal<Vec<ChatMsg>>,
    streaming: Signal<bool>,
    /// Tokens the conversation has spent, accumulated across turns.
    usage: Signal<Usage>,
    /// Keeps the transcript pinned to the newest tokens while streaming.
    scroll: gpui::ScrollHandle,
}

impl EventEmitter<AssistantEvent> for AssistantPanel {}

impl AssistantPanel {
    pub fn new(app: AppState, state: WorkspaceState, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| {
            AIComposer::new(cx)
                .size(Size::Sm)
                .attachments(false)
                .hint("⏎ to send · ⇧⏎ for a new line")
                .placeholder("Ask about your data…", cx)
        });
        let messages = Signal::new(cx, Vec::new());
        let streaming = Signal::new(cx, false);
        let usage = Signal::new(cx, Usage::default());
        watch(cx, &messages);
        watch(cx, &streaming);
        watch(cx, &usage);

        cx.subscribe(&composer, |this, _composer, event: &AIComposerEvent, cx| match event {
            AIComposerEvent::Submit(text) => this.send(text.clone(), cx),
            // Nothing to interrupt yet: `bridge::stream` owns the task and does
            // not hand back a handle, so the button is only shown as state.
            AIComposerEvent::Stop | AIComposerEvent::Attach | AIComposerEvent::Change(_) => {}
        })
        .detach();

        AssistantPanel {
            app,
            state,
            composer,
            messages,
            streaming,
            usage,
            scroll: gpui::ScrollHandle::new(),
        }
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
                "Add an API key or subscription token in Settings (⌘,) → Assistant.",
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
        self.streaming.set(cx, true);
        self.composer.update(cx, |composer, cx| composer.set_busy(true, cx));

        let messages = self.messages.clone();
        let streaming = self.streaming.clone();
        let usage = self.usage.clone();
        let composer = self.composer.clone();
        let scroll = self.scroll.clone();
        bridge::stream(
            cx,
            move |tx| ai::stream_chat(config, credential, Some(system), history, tx),
            move |event, cx| match event {
                StreamEvent::Delta(text) => {
                    messages.update(cx, |list| {
                        if let Some(last) = list.last_mut() {
                            last.text.push_str(&text);
                        }
                    });
                    // Keep the freshest tokens in view as they arrive.
                    scroll.scroll_to_bottom();
                }
                StreamEvent::Usage(reported) => {
                    usage.update(cx, |total| total.merge(reported));
                }
                StreamEvent::Error(error) => messages.update(cx, |list| {
                    if let Some(last) = list.last_mut() {
                        last.text = format!("⚠ {error}");
                    }
                }),
            },
            move |cx| {
                streaming.set(cx, false);
                composer.update(cx, |composer, cx| composer.set_busy(false, cx));
            },
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
        self.usage.set(cx, Usage::default());
    }

    /// One fenced code block: the SQL, and what can be done with it.
    fn code_block(
        &self,
        msg_idx: usize,
        block: usize,
        runnable: bool,
        code: String,
        cx: &Context<Self>,
    ) -> gpui::AnyElement {
        let colors = crate::theme::palette(cx);
        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(8.0))
            .rounded(px(4.0))
            .bg(colors.bg_surface)
            .border_1()
            .border_color(colors.border)
            .child(
                div()
                    .id(gpui::SharedString::from(format!("ai-code-{msg_idx}-{block}")))
                    .overflow_x_scroll()
                    .font_family(crate::theme::MONO_FAMILY)
                    .text_size(px(11.0))
                    .child(gpui::SharedString::from(code.clone())),
            );

        if runnable {
            let run_code = code.clone();
            let ins_code = code;
            card = card.child(
                Group::new()
                    .gap(Size::Xs)
                    .align(Align::Center)
                    .child(
                        Button::new(
                            gpui::SharedString::from(format!("ai-run-{msg_idx}-{block}")),
                            "Run",
                        )
                        .size(Size::Xs)
                        .on_click(cx.listener(move |_this, _, _, cx| {
                            cx.emit(AssistantEvent::RunSql(run_code.clone()));
                        })),
                    )
                    .child(
                        Button::new(
                            gpui::SharedString::from(format!("ai-ins-{msg_idx}-{block}")),
                            "Insert",
                        )
                        .size(Size::Xs)
                        .variant(Variant::Subtle)
                        .on_click(cx.listener(move |_this, _, _, cx| {
                            cx.emit(AssistantEvent::InsertSql(ins_code.clone()));
                        })),
                    ),
            );
        }
        card.into_any_element()
    }

    /// One turn. A reply that has not opened a fence yet streams through
    /// `AIMessage` itself; once it has, the body is built from its segments so
    /// the SQL can carry its actions.
    fn turn(&self, index: usize, msg: &ChatMsg, streaming_now: bool, cx: &Context<Self>) -> gpui::AnyElement {
        if msg.role == Role::User {
            return AIMessage::new(AIRole::User, msg.text.clone())
                .size(Size::Sm)
                .into_any_element();
        }

        if !has_fence(&msg.text) {
            return AIMessage::new(AIRole::Assistant, msg.text.clone())
                .size(Size::Sm)
                .streaming(streaming_now)
                .into_any_element();
        }

        let mut message = AIMessage::new(AIRole::Assistant, "").size(Size::Sm);
        for (block, seg) in segments(&msg.text).into_iter().enumerate() {
            message = match seg {
                Segment::Text(text) => {
                    message.child(Markdown::new(text).size(Size::Sm).into_any_element())
                }
                Segment::Code { runnable, code } => {
                    message.child(self.code_block(index, block, runnable, code, cx))
                }
            };
        }
        message.into_any_element()
    }

    /// The running token count and what it has cost, from the API's own
    /// numbers and the selected model's published rates.
    fn cost_line(&self, cx: &Context<Self>) -> Option<gpui::AnyElement> {
        let usage = *self.usage.read(cx);
        if usage.input == 0 && usage.output == 0 {
            return None;
        }
        let model = self.app.settings.read(cx).ai_model.clone();
        let info = ai::model_info(&model)?;
        let spent = usage.cost(info.input_per_million, info.output_per_million);
        Some(
            Group::new()
                .gap(Size::Xs)
                .align(Align::Center)
                .child(
                    AITokenMeter::new(usage.input + usage.output, info.context)
                        .size(Size::Xs)
                        .bar(false),
                )
                .child(Text::new(format_cost(spent)).size(Size::Xs).dimmed())
                .into_any_element(),
        )
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
        let messages = self.messages.read(cx).clone();
        let has_key = self.app.host.has_ai_secret(&self.app.settings.read(cx).ai_auth_mode);

        let header = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .px(px(8.0))
            .py(px(6.0))
            .border_b_1()
            .border_color(colors.border)
            .child(
                Group::new()
                    .gap(Size::Xs)
                    .align(Align::Center)
                    .child(Icon::new(IconName::Sparkles).size(Size::Xs))
                    .child(Text::new("Assistant").size(Size::Xs).medium()),
            )
            .child(
                Group::new()
                    .gap(Size::Xs)
                    .align(Align::Center)
                    .children(self.cost_line(cx))
                    .child(
                        Button::new("ai-clear", "Clear")
                            .size(Size::Xs)
                            .variant(Variant::Subtle)
                            .disabled(messages.is_empty())
                            .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
                    )
                    .child(
                        ActionIcon::new("ai-close", "✕")
                            .variant(Variant::Subtle)
                            .size(Size::Sm)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.state.ai_open.set(cx, false)
                            })),
                    ),
            );

        let body = if messages.is_empty() {
            let hint = if has_key {
                "Ask me to write a query, explain a table, or debug SQL."
            } else {
                "Add an AI key in Settings (⌘,) → Assistant to get started."
            };
            Center::new()
                .child(Text::new(hint).size(Size::Xs).dimmed())
                .into_any_element()
        } else {
            let last = messages.len() - 1;
            let mut stack = Stack::new().gap(Size::Sm);
            for (i, msg) in messages.iter().enumerate() {
                stack = stack.child(self.turn(i, msg, streaming && i == last, cx));
            }
            div()
                .id("ai-messages")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .p(px(8.0))
                .child(stack)
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .w(px(360.0))
            .h_full()
            .flex_none()
            .border_l_1()
            .border_color(colors.border)
            .bg(colors.bg_surface)
            .child(header)
            .child(div().flex_1().min_h(px(0.0)).child(body))
            .child(
                div()
                    .px(px(8.0))
                    .py(px(6.0))
                    .border_t_1()
                    .border_color(colors.border)
                    .child(self.composer.clone()),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{has_fence, segments, Segment};

    fn one_code(text: &str) -> (bool, String) {
        segments(text)
            .into_iter()
            .find_map(|s| match s {
                Segment::Code { runnable, code } => Some((runnable, code)),
                _ => None,
            })
            .expect("a code block")
    }

    #[test]
    fn extracts_a_runnable_sql_block() {
        let text = "Here you go:\n```sql\nSELECT * FROM users;\n```\nDone.";
        let segs = segments(text);
        assert_eq!(segs.len(), 3); // prose, code, prose
        let (runnable, code) = one_code(text);
        assert!(runnable);
        assert_eq!(code, "SELECT * FROM users;");
    }

    #[test]
    fn untagged_fence_is_runnable_but_other_languages_are_not() {
        assert!(one_code("```\nSELECT 1;\n```").0);
        assert!(!one_code("```python\nprint(1)\n```").0);
    }

    #[test]
    fn unterminated_block_is_not_runnable() {
        // Mid-stream: the closing fence has not arrived yet.
        let (runnable, code) = one_code("```sql\nSELECT * FROM ");
        assert!(!runnable);
        assert_eq!(code, "SELECT * FROM");
    }

    #[test]
    fn plain_text_has_no_code_segment() {
        let segs = segments("just a sentence with no code");
        assert_eq!(segs.len(), 1);
        assert!(matches!(segs[0], Segment::Text(_)));
    }

    #[test]
    fn fence_detection_decides_which_renderer_a_reply_gets() {
        // No fence: the whole body streams through AIMessage, caret and all.
        assert!(!has_fence("still writing prose"));
        // An opening fence is enough — the block is rendered before it closes.
        assert!(has_fence("here:\n```sql\nSELECT"));
        assert!(has_fence("```\nplain\n```"));
    }
}
