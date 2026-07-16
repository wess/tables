//! Shared data shapes for Tables. Serde names mirror the app's on-disk JSON
//! exactly, so these types round-trip the files under `~/.tables/`.

mod analysis;
mod connection;
mod health;
mod history;
mod macros;
mod plugin;
mod query;
mod schema;
mod settings;
mod transfer;
mod util;

pub use analysis::{ColumnProfile, SchemaDiff, TopValue};
pub use connection::{
    ConnectionConfig, ConnectionTestResult, SshConfig, SslConfig, StoredConnection,
};
pub use health::Health;
pub use history::{Favorite, HistoryEntry, SavedTab};
pub use macros::{Macro, MacroStep};
pub use plugin::{InstalledPlugin, PluginManifest};
pub use query::{
    FilterCondition, QueryResult, RawResult, Row, RowWrite, RowsRequest, RowsResponse, SortSpec,
};
pub use schema::{ColumnInfo, ForeignKeyInfo, IndexInfo, TableInfo, TableStructure};
pub use settings::Settings;
pub use transfer::{ExportFileResult, ImportResult, ImportSqlResult};
pub use util::{iso_now, new_uuid};
