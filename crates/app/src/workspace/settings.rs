//! The settings modal. Edits the app-wide `Settings`, persists them through the
//! host, updates the live `AppState.settings` signal, and re-applies the theme.

use gpui::prelude::*;
use gpui::{Context, Entity, EventEmitter, Window};
use guise::prelude::*;

use crate::sheet::Sheet;
use crate::state::AppState;
use model::Settings;

pub enum SettingsEvent {
    Close,
}

pub struct SettingsModal {
    app: AppState,
    theme: Entity<Select>,
    row_height: Entity<Select>,
    page_size: Entity<NumberInput>,
    font_size: Entity<NumberInput>,
    tab_size: Entity<NumberInput>,
    null_display: Entity<TextInput>,
    date_format: Entity<TextInput>,
    ai_model: Entity<Select>,
    ai_auth: Entity<Select>,
    ai_secret: Entity<TextInput>,
    word_wrap: bool,
    line_numbers: bool,
    show_row_numbers: bool,
    alternate_rows: bool,
    base: Settings,
}

impl EventEmitter<SettingsEvent> for SettingsModal {}

impl SettingsModal {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let app = AppState::get(cx);
        let base = app.settings.get(cx);

        let theme_idx = match base.theme.as_str() {
            "light" => 0,
            "auto" => 2,
            _ => 1,
        };
        let row_idx = match base.grid_row_height.as_str() {
            "normal" => 1,
            "comfortable" => 2,
            _ => 0,
        };

        let theme = cx.new(move |cx| {
            Select::new(cx).label("Theme").data(["Light", "Dark", "Auto"]).selected(theme_idx)
        });
        let row_height = cx.new(move |cx| {
            Select::new(cx)
                .label("Row height")
                .data(["Compact", "Normal", "Comfortable"])
                .selected(row_idx)
        });
        let page_size = cx.new({
            let v = base.grid_page_size as f64;
            move |cx| NumberInput::new(cx).label("Default page size").value(v)
        });
        let font_size = cx.new({
            let v = base.editor_font_size as f64;
            move |cx| NumberInput::new(cx).label("Editor font size").value(v)
        });
        let tab_size = cx.new({
            let v = base.editor_tab_size as f64;
            move |cx| NumberInput::new(cx).label("Editor tab size").value(v)
        });
        let null_display = cx.new({
            let v = base.null_display.clone();
            move |cx| TextInput::new(cx).label("NULL display").value(&v)
        });
        let date_format = cx.new({
            let v = base.date_format.clone();
            move |cx| TextInput::new(cx).label("Date format").value(&v)
        });

        let model_idx = ai::MODELS
            .iter()
            .position(|(id, _)| *id == base.ai_model)
            .unwrap_or(0);
        let ai_model = cx.new(move |cx| {
            let labels: Vec<String> = ai::MODELS.iter().map(|(_, label)| label.to_string()).collect();
            Select::new(cx).label("Model").data(labels).selected(model_idx)
        });
        let auth_idx = if base.ai_auth_mode == "subscription" { 1 } else { 0 };
        let ai_auth = cx.new(move |cx| {
            Select::new(cx)
                .label("Authentication")
                .data(["API Key", "Claude Subscription"])
                .selected(auth_idx)
        });
        let ai_stored = app.host.has_ai_secret(&base.ai_auth_mode);
        let ai_secret = cx.new(move |cx| {
            let placeholder = if ai_stored {
                "•••••••• stored — paste to replace"
            } else {
                "sk-ant-… (API key) or OAuth token"
            };
            TextInput::new(cx).label("Secret").placeholder(placeholder).password(true)
        });

        SettingsModal {
            word_wrap: base.editor_word_wrap,
            line_numbers: base.editor_line_numbers,
            show_row_numbers: base.grid_show_row_numbers,
            alternate_rows: base.grid_alternate_rows,
            base,
            app,
            theme,
            row_height,
            page_size,
            font_size,
            tab_size,
            null_display,
            date_format,
            ai_model,
            ai_auth,
            ai_secret,
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let theme = match self.theme.read(cx).selected_index().unwrap_or(1) {
            0 => "light",
            2 => "auto",
            _ => "dark",
        }
        .to_string();
        let grid_row_height = match self.row_height.read(cx).selected_index().unwrap_or(0) {
            1 => "normal",
            2 => "comfortable",
            _ => "compact",
        }
        .to_string();

        let ai_model = ai::MODELS
            .get(self.ai_model.read(cx).selected_index().unwrap_or(0))
            .map(|(id, _)| id.to_string())
            .unwrap_or_else(|| ai::DEFAULT_MODEL.to_string());
        let ai_auth_mode = match self.ai_auth.read(cx).selected_index().unwrap_or(0) {
            1 => "subscription",
            _ => "apiKey",
        }
        .to_string();

        // A freshly entered secret goes to the keychain (keyed by auth mode);
        // an empty field leaves any existing secret untouched.
        let secret = self.ai_secret.read(cx).text();
        if !secret.trim().is_empty() {
            if let Err(error) = self.app.host.save_ai_secret(&ai_auth_mode, secret.trim()) {
                self.app.toasts.error(cx, "Keychain unavailable", &error);
            }
        }

        let new = Settings {
            theme: theme.clone(),
            editor_font_size: self.font_size.read(cx).value_f64().unwrap_or(13.0) as f32,
            editor_tab_size: self.tab_size.read(cx).value_f64().unwrap_or(2.0) as usize,
            editor_word_wrap: self.word_wrap,
            editor_line_numbers: self.line_numbers,
            grid_row_height,
            grid_page_size: (self.page_size.read(cx).value_f64().unwrap_or(100.0) as u64).max(1),
            grid_show_row_numbers: self.show_row_numbers,
            grid_alternate_rows: self.alternate_rows,
            date_format: self.date_format.read(cx).text(),
            null_display: self.null_display.read(cx).text(),
            ai_model,
            ai_auth_mode,
            extra: self.base.extra.clone(),
        };

        if let Ok(value) = serde_json::to_value(&new) {
            self.app.host.save_settings(&value);
        }
        self.app.settings.set(cx, new);

        // Re-apply the theme (auto falls back to dark for now).
        let scheme = if theme == "light" { ColorScheme::Light } else { ColorScheme::Dark };
        crate::theme::build(scheme).init(cx);
        cx.refresh_windows();

        cx.emit(SettingsEvent::Close);
    }
}

impl Render for SettingsModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let switch = |id: &'static str, label: &'static str, on: bool| {
            Switch::new(id).label(label).checked(on)
        };

        let appearance = Stack::new()
            .gap(Size::Sm)
            .child(Title::new("Appearance").order(6))
            .child(self.theme.clone())
            .child(self.null_display.clone())
            .child(self.date_format.clone());

        let editor = Stack::new()
            .gap(Size::Sm)
            .child(Title::new("Editor").order(6))
            .child(Group::new().grow(true).gap(Size::Sm).child(self.font_size.clone()).child(self.tab_size.clone()))
            .child(switch("s-wrap", "Word wrap", self.word_wrap).on_change(cx.listener(
                |this, _, _, cx| {
                    this.word_wrap = !this.word_wrap;
                    cx.notify();
                },
            )))
            .child(switch("s-lines", "Line numbers", self.line_numbers).on_change(cx.listener(
                |this, _, _, cx| {
                    this.line_numbers = !this.line_numbers;
                    cx.notify();
                },
            )));

        let grid = Stack::new()
            .gap(Size::Sm)
            .child(Title::new("Data grid").order(6))
            .child(Group::new().grow(true).gap(Size::Sm).child(self.row_height.clone()).child(self.page_size.clone()))
            .child(switch("s-rownum", "Show row numbers", self.show_row_numbers).on_change(
                cx.listener(|this, _, _, cx| {
                    this.show_row_numbers = !this.show_row_numbers;
                    cx.notify();
                }),
            ))
            .child(switch("s-alt", "Alternate row shading", self.alternate_rows).on_change(
                cx.listener(|this, _, _, cx| {
                    this.alternate_rows = !this.alternate_rows;
                    cx.notify();
                }),
            ));

        let ai = Stack::new()
            .gap(Size::Sm)
            .child(Title::new("AI Assistant").order(6))
            .child(Group::new().grow(true).gap(Size::Sm).child(self.ai_model.clone()).child(self.ai_auth.clone()))
            .child(self.ai_secret.clone())
            .child(
                Text::new(
                    "Anthropic (Claude). Use a pay-per-use API key, or paste a Claude \
                     subscription OAuth token. Stored in your OS keychain, never on disk.",
                )
                .size(Size::Xs)
                .dimmed(),
            );

        let actions = Group::new()
            .justify(Justify::End)
            .gap(Size::Xs)
            .child(
                Button::new("settings-cancel", "Cancel")
                    .variant(Variant::Subtle)
                    .color(ColorName::Gray)
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(SettingsEvent::Close))),
            )
            .child(
                Button::new("settings-save", "Save").on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            );

        Sheet::new()
            .title("Settings")
            .width(520.0)
            .on_close(cx.listener(|_, _, _, cx| cx.emit(SettingsEvent::Close)))
            .child(Stack::new().gap(Size::Lg).child(appearance).child(editor).child(grid).child(ai))
            .child(Divider::new())
            .child(actions)
    }
}
