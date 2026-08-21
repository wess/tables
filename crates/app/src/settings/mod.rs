//! The settings modal. Edits the app-wide `Settings`, persists them through the
//! host, updates the live `AppState.settings` signal, and re-applies the theme.
//!
//! The shell is guise's `SettingsView` — a page list, a search field, and a
//! content closure re-invoked every frame. That closure is `'static` and gets
//! only `&mut App`, so it cannot hold `cx.listener`; it captures a weak handle
//! to this entity and upgrades it when a control fires. Every control the user
//! can touch therefore lives in [`Controls`], cloned into the closure.
//!
//! Selects bind to a `Signal<usize>` rather than being set directly: `Select`
//! has no runtime setter, and the signal is what lets a row's reset arrow put
//! the choice back to its default.

mod pages;

use gpui::prelude::*;
use gpui::{px, Context, Entity, EventEmitter, Window};
use guise::prelude::*;
use guise::settings::SettingsView;

use crate::sheet::Sheet;
use crate::state::AppState;
use model::Settings;

pub enum SettingsEvent {
    Close,
}

/// Every control the content closure needs. Cloned in wholesale, because the
/// closure is rebuilt each frame and threading ten handles through by hand is
/// ten chances to forget one.
#[derive(Clone)]
pub(super) struct Controls {
    pub theme: Entity<Select>,
    pub theme_idx: Signal<usize>,
    pub row_height: Entity<Select>,
    pub row_height_idx: Signal<usize>,
    pub page_size: Entity<NumberInput>,
    pub font_size: Entity<NumberInput>,
    pub tab_size: Entity<NumberInput>,
    pub null_display: Entity<TextInput>,
    pub date_format: Entity<TextInput>,
    pub ai_model: Entity<Select>,
    pub ai_model_idx: Signal<usize>,
    pub ai_auth: Entity<Select>,
    pub ai_auth_idx: Signal<usize>,
    pub ai_secret: Entity<TextInput>,
}

pub struct SettingsModal {
    app: AppState,
    view: Entity<SettingsView>,
    pub(super) controls: Controls,
    /// The toggles, owned here because a `Switch` is stateless.
    pub(super) word_wrap: bool,
    pub(super) line_numbers: bool,
    pub(super) show_row_numbers: bool,
    pub(super) alternate_rows: bool,
    pub(super) auto_update: bool,
    base: Settings,
}

impl EventEmitter<SettingsEvent> for SettingsModal {}

/// Index of a theme name in the Theme select.
fn theme_index(name: &str) -> usize {
    match name {
        "light" => 0,
        "auto" => 2,
        _ => 1,
    }
}

/// Index of a row-height name in the Row height select.
fn row_height_index(name: &str) -> usize {
    match name {
        "normal" => 1,
        "comfortable" => 2,
        _ => 0,
    }
}

impl SettingsModal {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let app = AppState::get(cx);
        let base = app.settings.get(cx);

        let theme_idx = Signal::new(cx, theme_index(&base.theme));
        let theme = cx.new(|cx| Select::new(cx).data(["Light", "Dark", "Auto"]).size(Size::Sm));
        Select::bind(&theme, &theme_idx, cx);

        let row_height_idx = Signal::new(cx, row_height_index(&base.grid_row_height));
        let row_height = cx.new(|cx| {
            Select::new(cx)
                .data(["Compact", "Normal", "Comfortable"])
                .size(Size::Sm)
        });
        Select::bind(&row_height, &row_height_idx, cx);

        let page_size = cx.new({
            let v = base.grid_page_size as f64;
            move |cx| NumberInput::new(cx).value(v).size(Size::Sm)
        });
        let font_size = cx.new({
            let v = base.editor_font_size as f64;
            move |cx| NumberInput::new(cx).value(v).size(Size::Sm)
        });
        let tab_size = cx.new({
            let v = base.editor_tab_size as f64;
            move |cx| NumberInput::new(cx).value(v).size(Size::Sm)
        });
        let null_display = cx.new({
            let v = base.null_display.clone();
            move |cx| TextInput::new(cx).value(&v).size(Size::Sm)
        });
        let date_format = cx.new({
            let v = base.date_format.clone();
            move |cx| TextInput::new(cx).value(&v).size(Size::Sm)
        });

        let model_index = ai::MODELS.iter().position(|m| m.id == base.ai_model).unwrap_or(0);
        let ai_model_idx = Signal::new(cx, model_index);
        let ai_model = cx.new(|cx| {
            let labels: Vec<String> = ai::MODELS.iter().map(|m| m.label.to_string()).collect();
            Select::new(cx).data(labels).size(Size::Sm)
        });
        Select::bind(&ai_model, &ai_model_idx, cx);

        let ai_auth_idx =
            Signal::new(cx, usize::from(base.ai_auth_mode == "subscription"));
        let ai_auth = cx.new(|cx| {
            Select::new(cx)
                .data(["API Key", "Claude Subscription"])
                .size(Size::Sm)
        });
        Select::bind(&ai_auth, &ai_auth_idx, cx);

        let ai_stored = app.host.has_ai_secret(&base.ai_auth_mode);
        let ai_secret = cx.new(move |cx| {
            let placeholder = if ai_stored {
                "•••••••• stored — paste to replace"
            } else {
                "sk-ant-… (API key) or OAuth token"
            };
            TextInput::new(cx)
                .placeholder(placeholder)
                .password(true)
                .size(Size::Sm)
        });

        let controls = Controls {
            theme,
            theme_idx,
            row_height,
            row_height_idx,
            page_size,
            font_size,
            tab_size,
            null_display,
            date_format,
            ai_model,
            ai_model_idx,
            ai_auth,
            ai_auth_idx,
            ai_secret,
        };

        let weak = cx.weak_entity();
        let for_content = controls.clone();
        let view = cx.new(|cx| {
            SettingsView::new(cx)
                .page_icon("appearance", "Appearance", IconName::Palette)
                .page_icon("grid", "Data grid", IconName::Table2)
                .page_icon("editor", "SQL editor", IconName::Code)
                .page_icon("assistant", "Assistant", IconName::Sparkles)
                .page_icon("updates", "Updates", IconName::Download)
                .sidebar_width(168.0)
                .content(move |page, _query, window, cx| {
                    pages::content(&weak, &for_content, page, window, cx)
                })
        });

        SettingsModal {
            word_wrap: base.editor_word_wrap,
            line_numbers: base.editor_line_numbers,
            show_row_numbers: base.grid_show_row_numbers,
            alternate_rows: base.grid_alternate_rows,
            auto_update: base.auto_update,
            base,
            app,
            view,
            controls,
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let theme = match self.controls.theme_idx.get(cx) {
            0 => "light",
            2 => "auto",
            _ => "dark",
        }
        .to_string();
        let grid_row_height = match self.controls.row_height_idx.get(cx) {
            1 => "normal",
            2 => "comfortable",
            _ => "compact",
        }
        .to_string();

        let ai_model = ai::MODELS
            .get(self.controls.ai_model_idx.get(cx))
            .map(|m| m.id.to_string())
            .unwrap_or_else(|| ai::DEFAULT_MODEL.to_string());
        let ai_auth_mode = match self.controls.ai_auth_idx.get(cx) {
            1 => "subscription",
            _ => "apiKey",
        }
        .to_string();

        // A freshly entered secret goes to the keychain (keyed by auth mode);
        // an empty field leaves any existing secret untouched.
        let secret = self.controls.ai_secret.read(cx).text();
        if !secret.trim().is_empty() {
            if let Err(error) = self.app.host.save_ai_secret(&ai_auth_mode, secret.trim()) {
                self.app.toasts.error(cx, "Keychain unavailable", &error);
            }
        }

        let new = Settings {
            theme: theme.clone(),
            editor_font_size: self.controls.font_size.read(cx).value_f64().unwrap_or(13.0) as f32,
            editor_tab_size: self.controls.tab_size.read(cx).value_f64().unwrap_or(2.0) as usize,
            editor_word_wrap: self.word_wrap,
            editor_line_numbers: self.line_numbers,
            grid_row_height,
            grid_page_size: (self.controls.page_size.read(cx).value_f64().unwrap_or(100.0) as u64)
                .max(1),
            grid_show_row_numbers: self.show_row_numbers,
            grid_alternate_rows: self.alternate_rows,
            date_format: self.controls.date_format.read(cx).text(),
            null_display: self.controls.null_display.read(cx).text(),
            ai_model,
            ai_auth_mode,
            auto_update: self.auto_update,
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
                Button::new("settings-save", "Save")
                    .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
            );

        // `SettingsView` sizes itself to its parent, so it gets a definite box
        // rather than the sheet's scrolling body.
        Sheet::new()
            .title("Settings")
            .width(720.0)
            .on_close(cx.listener(|_, _, _, cx| cx.emit(SettingsEvent::Close)))
            .child(gpui::div().h(px(420.0)).w_full().child(self.view.clone()))
            .child(Divider::new())
            .child(actions)
    }
}
