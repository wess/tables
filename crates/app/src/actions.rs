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
pub struct ShowAbout;

#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = tables, no_json)]
pub struct CheckForUpdates;

#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = tables, no_json)]
pub struct Hide;

#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = tables, no_json)]
pub struct HideOthers;

#[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
#[action(namespace = tables, no_json)]
pub struct ShowAll;

// Workspace actions dispatched from the menu bar (handled on the Workspace root
// when a connection is open; no-ops on the home screen).
macro_rules! ws_actions {
    ($($name:ident),* $(,)?) => {
        $(
            #[derive(Clone, PartialEq, Default, Debug, gpui::Action)]
            #[action(namespace = tables, no_json)]
            pub struct $name;
        )*
    };
}
ws_actions!(
  NewTable,
  FormatSql,
  ExplainQuery,
  RunTransaction,
  OpenSessions,
  BackupDatabase,
  RestoreDatabase,
  RefreshTables,
  SchemaCompare,
  ErDiagram,
  OpenExtensions,
  ToggleAi,
  ToggleFilters,
  ToggleInspector,
  OpenSettings,
  ShowDocs,
);
