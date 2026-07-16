//! Tables — an open-source database client built with gpui and guise.
//!
//! `main` installs the theme, wires the menu bar, and opens the root window.
//! The heavy lifting lives in the domain crates (`db`, `store`, `host`); the
//! async DB layer is reached through the tokio bridge.

mod bridge;
mod home;
mod root;
mod sheet;
mod theme;
mod workspace;

// `state` holds the full cross-panel contract and `toasts` the severity
// helpers; the panels consume them incrementally, so a few are staged ahead of
// their first caller.
#[allow(dead_code)]
mod state;
#[allow(dead_code)]
mod toasts;

use gpui::prelude::*;
use gpui::{
    px, size, App, Application, Bounds, KeyBinding, Menu, MenuItem, OsAction, SharedString,
    TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowOptions,
};
use guise::prelude::*;

#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = tables, no_json)]
pub struct NewConnection;

#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = tables, no_json)]
pub struct RunQuery;

#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = tables, no_json)]
pub struct OpenPalette;

#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = tables, no_json)]
pub struct Quit;

#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = tables, no_json)]
pub struct Hide;

#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = tables, no_json)]
pub struct HideOthers;

#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = tables, no_json)]
pub struct ShowAll;

/// The native menu bar. Custom items are UI-handled; only Quit / Hide act here.
fn menus() -> Vec<Menu> {
    vec![
        Menu {
            name: SharedString::new_static("Tables"),
            items: vec![
                MenuItem::action("Hide Tables", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit Tables", Quit),
            ],
        },
        Menu {
            name: SharedString::new_static("File"),
            items: vec![
                MenuItem::action("New Connection", NewConnection),
                MenuItem::separator(),
                MenuItem::action("Quit", Quit),
            ],
        },
        Menu {
            name: SharedString::new_static("Edit"),
            items: vec![
                MenuItem::os_action("Cut", Quit, OsAction::Cut),
                MenuItem::os_action("Copy", Quit, OsAction::Copy),
                MenuItem::os_action("Paste", Quit, OsAction::Paste),
                MenuItem::os_action("Select All", Quit, OsAction::SelectAll),
            ],
        },
        Menu {
            name: SharedString::new_static("Query"),
            items: vec![MenuItem::action("Execute Query", RunQuery)],
        },
    ]
}

fn main() {
    Application::new().run(|cx: &mut App| {
        theme::build(ColorScheme::Dark).init(cx);

        cx.bind_keys([
            KeyBinding::new("cmd-n", NewConnection, None),
            KeyBinding::new("cmd-p", OpenPalette, None),
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("alt-cmd-h", HideOthers, None),
        ]);
        cx.set_menus(menus());
        cx.on_action::<Quit>(|_, cx| cx.quit());
        cx.on_action::<Hide>(|_, cx| cx.hide());
        cx.on_action::<HideOthers>(|_, cx| cx.hide_other_apps());
        cx.on_action::<ShowAll>(|_, cx| cx.unhide_other_apps());

        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(720.0), px(480.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some(format!("Tables v{}", env!("CARGO_PKG_VERSION")).into()),
                    ..Default::default()
                }),
                window_background: WindowBackgroundAppearance::Blurred,
                ..Default::default()
            },
            |_, cx| cx.new(root::Root::new),
        )
        .unwrap();
        cx.activate(true);
    });
}
