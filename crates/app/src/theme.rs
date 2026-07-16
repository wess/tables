//! The app's theme, mapped onto guise.
//!
//! The dark scheme re-pins guise's `Dark` ramp to the Mantine dark scale the
//! original configured, so every guise semantic color (`body`, `surface`,
//! `text`, `dimmed`, `border`) resolves to the intended values.

use gpui::Hsla;
use guise::prelude::*;
use guise::theme::{Color, Shades};

/// Mantine dark scale (`dark-0` … `dark-9`).
const DARK_RAMP: [&str; 10] = [
    "#C1C2C5", "#A6A7AB", "#909296", "#5C5F66", "#373A40", "#2C2E33", "#25262B", "#1A1B1E",
    "#141517", "#101113",
];

/// Build the guise theme for a scheme. Text uses the system UI font.
pub fn build(scheme: ColorScheme) -> Theme {
    let mut theme = match scheme {
        ColorScheme::Dark => Theme::dark(),
        ColorScheme::Light => Theme::light(),
    };
    theme
        .palette
        .set_shades(ColorName::Dark, Shades(DARK_RAMP.map(Color::hex)));
    theme.primary_color = ColorName::Blue;
    theme.default_radius = Size::Md;
    theme.font_family = ".SystemUIFont".into();
    theme
}

/// The monospace family used for data cells and SQL.
pub const MONO_FAMILY: &str = "Menlo";

/// The resolved surface/border/text palette for the active theme. A few tokens
/// (scrollbar/tab tints) are staged for panels that don't consume them yet.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct Palette {
    pub bg_surface: Hsla,
    pub bg_subtle: Hsla,
    pub bg_muted: Hsla,
    pub border: Hsla,
    pub border_subtle: Hsla,
    pub text_muted: Hsla,
    pub grid_header: Hsla,
    pub grid_stripe: Hsla,
    pub scrollbar: Hsla,
    pub scrollbar_hover: Hsla,
    pub tab_hover: Hsla,
    pub tab_text: Hsla,
    pub tab_text_hover: Hsla,
}

fn hex(code: &str) -> Hsla {
    Color::hex(code).hsla()
}

pub fn colors(theme: &Theme) -> Palette {
    let shade = |i: usize| theme.color(ColorName::Dark, i).hsla();
    let gray = |i: usize| theme.color(ColorName::Gray, i).hsla();
    match theme.scheme {
        ColorScheme::Dark => Palette {
            bg_surface: shade(8),
            bg_subtle: shade(7),
            bg_muted: shade(6),
            border: shade(5),
            border_subtle: shade(6),
            text_muted: shade(2),
            grid_header: shade(7),
            grid_stripe: gpui::hsla(0.0, 0.0, 0.0, 0.08),
            scrollbar: shade(4),
            scrollbar_hover: shade(3),
            tab_hover: shade(6),
            tab_text: shade(2),
            tab_text_hover: shade(0),
        },
        ColorScheme::Light => Palette {
            bg_surface: hex("#ffffff"),
            bg_subtle: hex("#f8f9fa"),
            bg_muted: hex("#f1f3f5"),
            border: gray(3),
            border_subtle: gray(2),
            text_muted: gray(6),
            grid_header: gray(0),
            grid_stripe: gpui::hsla(0.0, 0.0, 0.0, 0.02),
            scrollbar: gray(4),
            scrollbar_hover: gray(5),
            tab_hover: gray(1),
            tab_text: gray(6),
            tab_text_hover: gray(8),
        },
    }
}

/// The resolved palette for the active global theme.
pub fn palette(cx: &gpui::App) -> Palette {
    colors(guise::theme::theme(cx))
}

/// Per-DB-type accent maps shared by cards, badges, and the status bar.
pub fn type_label(kind: &str) -> &'static str {
    match kind {
        "postgres" => "PostgreSQL",
        "sqlite" => "SQLite",
        "mysql" => "MySQL",
        _ => "unknown",
    }
}

pub fn type_color(kind: &str) -> ColorName {
    match kind {
        "postgres" => ColorName::Blue,
        "sqlite" => ColorName::Teal,
        "mysql" => ColorName::Orange,
        _ => ColorName::Gray,
    }
}
