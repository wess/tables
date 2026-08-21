//! The About card — what this build is, and where it came from.
//!
//! guise ships the card; this wraps it in the app's `Sheet` so it closes the
//! way every other overlay does. The build kind comes from `build.rs`, which is
//! the only place that can tell a release apart from a checkout that happens to
//! carry the same version number.

use gpui::prelude::*;
use gpui::{div, px, App, ClickEvent, IntoElement, Window};
use guise::prelude::*;
use guise::{About, BuildKind};

use crate::sheet::Sheet;

const REPO: &str = "https://github.com/wess/tables";

type CloseFn = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// `Released` only when `build.rs` saw `TABLES_RELEASE=1`.
fn kind() -> BuildKind {
    match env!("BUILD_KIND") {
        "released" => BuildKind::Released,
        _ => BuildKind::Development,
    }
}

/// The About card in a sheet. `on_close` gets the scrim and the ×.
#[derive(IntoElement)]
pub struct AboutSheet {
    on_close: Option<CloseFn>,
}

impl AboutSheet {
    pub fn new() -> Self {
        AboutSheet { on_close: None }
    }

    pub fn on_close(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Some(Box::new(handler));
        self
    }
}

impl Default for AboutSheet {
    fn default() -> Self {
        AboutSheet::new()
    }
}

impl RenderOnce for AboutSheet {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let card = About::new("Tables")
            .version(env!("CARGO_PKG_VERSION"))
            .build(kind(), env!("BUILD_DATE"))
            .icon(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(56.0))
                    .child(Icon::new(IconName::Database).size(Size::Xl)),
            )
            .tagline("A database client for PostgreSQL, MySQL, and SQLite.")
            .credits("MIT licensed. Built with gpui and guise.")
            .link(Anchor::new("about-repo", "Source").on_click(|_, _, cx| cx.open_url(REPO)))
            .link(
                Anchor::new("about-issues", "Report an issue")
                    .on_click(|_, _, cx| cx.open_url(&format!("{REPO}/issues"))),
            )
            .link(
                Anchor::new("about-sponsor", "Sponsor")
                    .on_click(|_, _, cx| cx.open_url("https://github.com/sponsors/wess")),
            );

        let mut sheet = Sheet::new().title("About Tables").width(420.0);
        if let Some(handler) = self.on_close {
            sheet = sheet.on_close(move |ev, window, cx| handler(ev, window, cx));
        }
        sheet.child(card)
    }
}
