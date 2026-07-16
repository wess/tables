//! The AI assistant client. A small, gpui-free wrapper over the Anthropic
//! Messages API: configuration types plus a streaming chat call. The app drives
//! it through the tokio↔gpui bridge, so this crate never touches the UI.
//!
//! Two auth modes are supported (see [`config::AuthMode`]): a pay-per-use API
//! key sent as `x-api-key`, or a Claude subscription OAuth access token sent as
//! `Authorization: Bearer` with the `oauth-2025-04-20` beta header.

mod anthropic;
mod config;

pub use anthropic::stream_chat;
pub use config::{AiConfig, AuthMode, Message, Role, StreamEvent, DEFAULT_MODEL, MODELS};
