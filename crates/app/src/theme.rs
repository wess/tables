//! App surfaces and Guise semantic colors.

use gpui::Hsla;
use guise::prelude::*;
use guise::theme::{Color, Shades};

/// Graphite shades for components that use the dark ramp directly.
const DARK_RAMP: [&str; 10] = [
    "#EDF0F5", "#D8DDE6", "#B2BAC7", "#8993A3", "#606A7A", "#454D5B", "#39414E", "#2E3037",
    "#2B2D33", "#222933",
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
    theme.palette.set_shades(
        ColorName::Gray,
        Shades(
            [
                "#F7F9FB", "#EDF1F5", "#DFE5ED", "#CCD5E1", "#B6C0CF", "#8A97AA", "#5D6B80",
                "#45546A", "#334258", "#243246",
            ]
            .map(Color::hex),
        ),
    );
    theme.primary_color = ColorName::Blue;
    theme.default_radius = Size::Sm;
    theme.font_family = ".SystemUIFont".into();
    match scheme {
        ColorScheme::Dark => theme
            .with_body(hex("#2B2D33"))
            .with_surface(hex("#30333B"))
            .with_surface_hover(hex("#424650"))
            .with_text(hex("#E3E5EA"))
            .with_dimmed(hex("#A8ADB8"))
            .with_border(hex("#3D414A")),
        ColorScheme::Light => theme
            .with_body(hex("#FFFFFF"))
            .with_surface(hex("#F2F4F7"))
            .with_surface_hover(hex("#E6EAF0"))
            .with_text(hex("#253043"))
            .with_dimmed(hex("#627087"))
            .with_border(hex("#D5DBE4")),
    }
}

pub fn scheme(preference: &str, cx: &gpui::App) -> ColorScheme {
    match preference {
        "light" => ColorScheme::Light,
        "dark" => ColorScheme::Dark,
        _ => match cx.window_appearance() {
            gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark => ColorScheme::Dark,
            _ => ColorScheme::Light,
        },
    }
}

/// The monospace family used for data cells and SQL.
pub const MONO_FAMILY: &str = "Menlo";

/// The resolved surface/border/text palette for the active theme. A few tokens
/// (scrollbar/tab tints) are staged for panels that don't consume them yet.
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub struct Palette {
    pub bg_surface: Hsla,
    pub bg_titlebar: Hsla,
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
            bg_surface: hex("#30333B"),
            bg_titlebar: hex("#383B44"),
            bg_subtle: hex("#2E3037"),
            bg_muted: hex("#383C45"),
            border: hex("#3D414A"),
            border_subtle: hex("#33363E"),
            text_muted: shade(2),
            grid_header: hex("#30333B"),
            grid_stripe: gpui::hsla(0.0, 0.0, 0.0, 0.035),
            scrollbar: shade(4),
            scrollbar_hover: shade(3),
            tab_hover: shade(6),
            tab_text: shade(2),
            tab_text_hover: shade(0),
        },
        ColorScheme::Light => Palette {
            bg_surface: hex("#F2F4F7"),
            bg_titlebar: hex("#E9ECF1"),
            bg_subtle: hex("#f8f9fa"),
            bg_muted: hex("#DFE4ED"),
            border: gray(3),
            border_subtle: gray(2),
            text_muted: hex("#627087"),
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
