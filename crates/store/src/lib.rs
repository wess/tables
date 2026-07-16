//! Local metadata persistence under `~/.tables/` — one module per file, plain
//! JSON, matching the original on-disk shapes and corrupt-file behaviors.

pub mod connections;
pub mod favorites;
pub mod history;
pub mod keychain;
pub mod macros;
pub mod paths;
pub mod plugins;
pub mod settings;
pub mod tabs;
