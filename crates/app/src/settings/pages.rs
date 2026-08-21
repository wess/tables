//! The settings pages: what each one shows.
//!
//! `SettingsView` can also carry a search field, and this deliberately does not
//! use it. The field is a `TextInput`, which guise wraps in a `Field` whose root
//! never sets `w_full`, so in the view's own sidebar it collapses to about the
//! width of three characters. Nothing here can size it — it belongs to the
//! component — so the field stays off until guise fixes it, rather than shipping
//! a search box that looks broken. Re-enabling is `.searchable(true)` plus
//! matching the query against each row's label and description.

use gpui::prelude::*;
use gpui::{div, px, AnyElement, App, IntoElement, WeakEntity, Window};
use guise::prelude::*;
use guise::settings::{SettingsRow, SettingsSection};

use super::{Controls, SettingsModal};
use model::Settings;

/// Puts one setting back to its default.
type ResetFn = Box<dyn Fn(&mut App) + 'static>;

/// One settings row.
struct Row {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    modified: bool,
    reset: Option<ResetFn>,
    control: AnyElement,
}

impl Row {
    fn new(
        id: &'static str,
        label: &'static str,
        description: &'static str,
        control: impl IntoElement,
    ) -> Self {
        Row {
            id,
            label,
            description,
            modified: false,
            reset: None,
            control: control.into_any_element(),
        }
    }

    /// Mark the row as differing from its default, and say how to put it back.
    ///
    /// Both halves together: a reset arrow on a row that already holds its
    /// default is a control that does nothing.
    fn resettable(mut self, modified: bool, reset: impl Fn(&mut App) + 'static) -> Self {
        self.modified = modified;
        self.reset = Some(Box::new(reset));
        self
    }

    fn render(self, last: bool) -> SettingsRow {
        let mut row = SettingsRow::new(self.id, self.label)
            .description(self.description)
            .modified(self.modified)
            .control(self.control)
            .divider(!last);
        if let Some(reset) = self.reset {
            row = row.on_reset(move |_, _, cx| reset(cx));
        }
        row
    }
}

/// A titled group of rows.
fn section(title: &'static str, rows: Vec<Row>) -> SettingsSection {
    let last = rows.len().saturating_sub(1);
    let mut out = SettingsSection::new(title);
    for (index, row) in rows.into_iter().enumerate() {
        out = out.child(row.render(index == last));
    }
    out
}

/// A `Switch` that writes a `bool` field on the modal.
fn toggle(
    weak: &WeakEntity<SettingsModal>,
    id: &'static str,
    on: bool,
    set: fn(&mut SettingsModal) -> &mut bool,
) -> impl IntoElement {
    let weak = weak.clone();
    Switch::new(id).checked(on).on_change(move |_, _, cx| {
        if let Some(modal) = weak.upgrade() {
            modal.update(cx, |modal, cx| {
                let field = set(modal);
                *field = !*field;
                cx.notify();
            });
        }
    })
}

/// Fixed-width box so every control on the right lines up.
fn control(width: f32, inner: impl IntoElement) -> impl IntoElement {
    div().w(px(width)).child(inner)
}

/// Build the active page.
pub(super) fn content(
    weak: &WeakEntity<SettingsModal>,
    c: &Controls,
    page: &str,
    _window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let Some(modal) = weak.upgrade() else {
        return div().into_any_element();
    };
    let state = modal.read(cx);
    let defaults = Settings::default();
    let mut sections: Vec<SettingsSection> = Vec::new();
    let want = |id: &str| page == id;

    if want("appearance") {
        let theme_idx = c.theme_idx.clone();
        let null_display = c.null_display.clone();
        let date_format = c.date_format.clone();
        let default_theme = super::theme_index(&defaults.theme);
        let default_null = defaults.null_display.clone();
        let default_date = defaults.date_format.clone();

        sections.push(section(
            "Appearance",
            vec![
                Row::new(
                    "set-theme",
                    "Theme",
                    "Auto follows the system appearance.",
                    control(150.0, c.theme.clone()),
                )
                .resettable(c.theme_idx.get(cx) != default_theme, move |cx| {
                    theme_idx.set(cx, default_theme)
                }),
                Row::new(
                    "set-null",
                    "NULL display",
                    "What an empty database value reads as in the grid.",
                    control(150.0, c.null_display.clone()),
                )
                .resettable(
                    c.null_display.read(cx).text() != default_null,
                    move |cx| {
                        let v = default_null.clone();
                        null_display.update(cx, |input, cx| input.set_text(&v, cx));
                    },
                ),
                Row::new(
                    "set-date",
                    "Date format",
                    "How timestamps are rendered.",
                    control(150.0, c.date_format.clone()),
                )
                .resettable(
                    c.date_format.read(cx).text() != default_date,
                    move |cx| {
                        let v = default_date.clone();
                        date_format.update(cx, |input, cx| input.set_text(&v, cx));
                    },
                ),
            ],
        ));
    }

    if want("grid") {
        let row_height_idx = c.row_height_idx.clone();
        let page_size = c.page_size.clone();
        let default_height = super::row_height_index(&defaults.grid_row_height);
        let default_page = defaults.grid_page_size as f64;

        sections.push(section(
            "Data grid",
            vec![
                Row::new(
                    "set-rowheight",
                    "Row height",
                    "How much vertical room each row takes.",
                    control(150.0, c.row_height.clone()),
                )
                .resettable(c.row_height_idx.get(cx) != default_height, move |cx| {
                    row_height_idx.set(cx, default_height)
                }),
                Row::new(
                    "set-pagesize",
                    "Default page size",
                    "Rows fetched per page when browsing a table.",
                    control(110.0, c.page_size.clone()),
                )
                .resettable(
                    c.page_size.read(cx).value_f64().unwrap_or(default_page) != default_page,
                    move |cx| {
                        page_size.update(cx, |input, cx| input.set_value(default_page, cx));
                    },
                ),
                Row::new(
                    "set-rownum",
                    "Show row numbers",
                    "A gutter numbering each row on the page.",
                    toggle(weak, "sw-rownum", state.show_row_numbers, |m| {
                        &mut m.show_row_numbers
                    }),
                ),
                Row::new(
                    "set-altrows",
                    "Alternate row shading",
                    "Tint every other row to help the eye track across.",
                    toggle(weak, "sw-altrows", state.alternate_rows, |m| {
                        &mut m.alternate_rows
                    }),
                ),
            ],
        ));
    }

    if want("editor") {
        let font_size = c.font_size.clone();
        let tab_size = c.tab_size.clone();
        let default_font = defaults.editor_font_size as f64;
        let default_tab = defaults.editor_tab_size as f64;

        sections.push(section(
            "SQL editor",
            vec![
                Row::new(
                    "set-fontsize",
                    "Font size",
                    "Point size of the query text.",
                    control(110.0, c.font_size.clone()),
                )
                .resettable(
                    c.font_size.read(cx).value_f64().unwrap_or(default_font) != default_font,
                    move |cx| {
                        font_size.update(cx, |input, cx| input.set_value(default_font, cx));
                    },
                ),
                Row::new(
                    "set-tabsize",
                    "Tab size",
                    "Spaces a tab indents by.",
                    control(110.0, c.tab_size.clone()),
                )
                .resettable(
                    c.tab_size.read(cx).value_f64().unwrap_or(default_tab) != default_tab,
                    move |cx| {
                        tab_size.update(cx, |input, cx| input.set_value(default_tab, cx));
                    },
                ),
                Row::new(
                    "set-wrap",
                    "Word wrap",
                    "Wrap long statements instead of scrolling sideways.",
                    toggle(weak, "sw-wrap", state.word_wrap, |m| &mut m.word_wrap),
                ),
                Row::new(
                    "set-linenums",
                    "Line numbers",
                    "A numbered gutter down the left of the editor.",
                    toggle(weak, "sw-linenums", state.line_numbers, |m| {
                        &mut m.line_numbers
                    }),
                ),
            ],
        ));
    }

    if want("assistant") {
        let info = ai::MODELS.get(c.ai_model_idx.get(cx));
        let pricing = info
            .map(|m| {
                format!(
                    "{} · {} context · ${:.2}/${:.2} per Mtok",
                    m.description,
                    compact_tokens(m.context),
                    m.input_per_million,
                    m.output_per_million,
                )
            })
            .unwrap_or_else(|| "Anthropic (Claude).".to_string());

        sections.push(section(
            "Assistant",
            vec![
                Row::new(
                    "set-aimodel",
                    "Model",
                    "Which Claude model answers.",
                    control(190.0, c.ai_model.clone()),
                ),
                Row::new(
                    "set-aiauth",
                    "Authentication",
                    "A pay-per-use API key, or a Claude subscription token.",
                    control(190.0, c.ai_auth.clone()),
                ),
                Row::new(
                    "set-aisecret",
                    "Secret",
                    "Stored in your OS keychain, never on disk.",
                    control(230.0, c.ai_secret.clone()),
                ),
            ],
        ));

        sections.push(
            SettingsSection::new("About this model")
                .rule(false)
                .child(Text::new(pricing).size(Size::Xs).dimmed()),
        );
    }

    if want("updates") {
        sections.push(section(
            "Updates",
            vec![Row::new(
                "set-autoupdate",
                "Check automatically",
                "Look for a new release at launch and hourly. Installing is always your click.",
                toggle(weak, "sw-autoupdate", state.auto_update, |m| {
                    &mut m.auto_update
                }),
            )],
        ));

        sections.push(
            SettingsSection::new("This build").rule(false).child(
                Text::new(format!(
                    "Tables {} · {}",
                    env!("CARGO_PKG_VERSION"),
                    env!("BUILD_DATE"),
                ))
                .size(Size::Xs)
                .dimmed(),
            ),
        );
    }

    let mut out = div().flex().flex_col().w_full();
    for s in sections {
        out = out.child(s);
    }
    out.into_any_element()
}

/// `1000000` → `1M`, `200000` → `200K`. Context windows only, so the rounding
/// never has to be cleverer than this.
fn compact_tokens(tokens: u64) -> String {
    match tokens {
        n if n >= 1_000_000 => format!("{}M", n / 1_000_000),
        n if n >= 1_000 => format!("{}K", n / 1_000),
        n => n.to_string(),
    }
}
