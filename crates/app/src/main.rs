//! Tables — an open-source database client built with gpui and guise.
//!
//! `main` installs the theme, wires the menu bar, and opens the root window.
//! The heavy lifting lives in the domain crates (`db`, `store`, `host`); the
//! async DB layer is reached through the tokio bridge.

mod about;
mod actions;
mod bridge;
mod home;
mod root;
mod settings;
mod sheet;
mod shortcuts;
mod theme;
mod titlebar;
mod update;
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
    px, size, App, Application, Bounds, Menu, MenuItem, SharedString, TitlebarOptions,
    WindowBackgroundAppearance, WindowBounds, WindowOptions,
};
use guise::prelude::*;

pub use actions::*;

fn menu(name: &'static str, items: Vec<MenuItem>) -> Menu {
    Menu {
        name: SharedString::new_static(name),
        items,
    }
}

/// The native menu bar, grouped like a full database client. Application-level
/// items (Quit / Hide / Show Docs) act here; the rest dispatch actions the
/// Workspace handles when a connection is open.
fn menus() -> Vec<Menu> {
    vec![
        menu(
            "Tables",
            vec![
                MenuItem::action("About Tables", ShowAbout),
                MenuItem::action("Check for Updates…", CheckForUpdates),
                MenuItem::separator(),
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::separator(),
                MenuItem::action("Hide Tables", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit Tables", Quit),
            ],
        ),
        menu(
            "File",
            vec![
                MenuItem::action("New Connection", NewConnection),
                MenuItem::action("New Table…", NewTable),
                MenuItem::separator(),
                MenuItem::action("Backup Database…", BackupDatabase),
                MenuItem::action("Restore Database…", RestoreDatabase),
            ],
        ),
        menu(
            "Edit",
            vec![
                MenuItem::action("Undo", guise::actions::Undo),
                MenuItem::action("Redo", guise::actions::Redo),
                MenuItem::separator(),
                MenuItem::action("Cut", guise::actions::Cut),
                MenuItem::action("Copy", guise::actions::Copy),
                MenuItem::action("Paste", guise::actions::Paste),
                MenuItem::action("Select All", guise::actions::SelectAll),
            ],
        ),
        menu(
            "Query",
            vec![
                MenuItem::action("Execute Query", RunQuery),
                MenuItem::action("Run in Transaction", RunTransaction),
                MenuItem::separator(),
                MenuItem::action("Explain", ExplainQuery),
                MenuItem::action("Format SQL", FormatSql),
            ],
        ),
        menu(
            "Database",
            vec![
                MenuItem::action("Refresh Tables", RefreshTables),
                MenuItem::separator(),
                MenuItem::action("Schema Compare…", SchemaCompare),
                MenuItem::action("ER Diagram…", ErDiagram),
                MenuItem::action("Sessions…", OpenSessions),
                MenuItem::separator(),
                MenuItem::action("Extensions…", OpenExtensions),
            ],
        ),
        menu(
            "View",
            vec![
                MenuItem::action("Command Palette…", OpenPalette),
                MenuItem::separator(),
                MenuItem::action("Toggle AI Assistant", ToggleAi),
                MenuItem::action("Toggle Filters", ToggleFilters),
                MenuItem::action("Toggle Inspector", ToggleInspector),
            ],
        ),
        menu("Help", vec![MenuItem::action("Documentation", ShowDocs)]),
    ]
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let settings = host::Host::new().settings();
        theme::build(theme::scheme(&settings.theme, cx)).init(cx);

        cx.bind_keys(shortcuts::bindings());
        cx.set_menus(menus());
        cx.on_action::<Quit>(|_, cx| cx.quit());
        cx.on_action::<Hide>(|_, cx| cx.hide());
        cx.on_action::<HideOthers>(|_, cx| cx.hide_other_apps());
        cx.on_action::<ShowAll>(|_, cx| cx.unhide_other_apps());
        cx.on_action::<ShowDocs>(|_, cx| cx.open_url("https://github.com/wess/tables"));
        cx.on_action::<CheckForUpdates>(|_, cx| update::check_now(cx));

        let bounds = Bounds::centered(None, size(px(1200.0), px(800.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(720.0), px(480.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some(format!("Tables v{}", env!("CARGO_PKG_VERSION")).into()),
                    appears_transparent: cfg!(target_os = "macos"),
                    traffic_light_position: Some(gpui::point(px(12.0), px(11.0))),
                }),
                window_background: WindowBackgroundAppearance::Opaque,
                ..Default::default()
            },
            |window, cx| {
                let root = cx.new(root::Root::new);
                window
                    .observe_window_appearance(|_, cx| {
                        let state = state::AppState::get(cx);
                        if state.settings.read(cx).theme == "auto" {
                            theme::build(theme::scheme("auto", cx)).init(cx);
                            cx.refresh_windows();
                        }
                    })
                    .detach();
                root
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
