//! Configuration and message shapes for the assistant.

/// How the assistant authenticates to Anthropic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthMode {
    /// A pay-per-use API key, sent as the `x-api-key` header.
    ApiKey,
    /// A Claude subscription OAuth access token, sent as `Authorization:
    /// Bearer` with the `oauth-2025-04-20` beta header. Paste a token minted by
    /// `ant auth print-credentials --access-token` (or a Claude login) here.
    Subscription,
}

impl AuthMode {
    /// Parse the persisted string form; anything unknown falls back to API key.
    pub fn parse(value: &str) -> Self {
        match value {
            "subscription" => AuthMode::Subscription,
            _ => AuthMode::ApiKey,
        }
    }

    /// The persisted string form (settings.json).
    pub fn as_str(self) -> &'static str {
        match self {
            AuthMode::Subscription => "subscription",
            AuthMode::ApiKey => "apiKey",
        }
    }
}

/// The models the picker offers, most-capable first: `(id, label)`.
pub const MODELS: &[(&str, &str)] = &[
    ("claude-opus-4-8", "Claude Opus 4.8"),
    ("claude-sonnet-5", "Claude Sonnet 5"),
    ("claude-haiku-4-5", "Claude Haiku 4.5"),
];

/// The default model when nothing is configured.
pub const DEFAULT_MODEL: &str = "claude-opus-4-8";

/// A resolved assistant configuration.
#[derive(Clone, Debug)]
pub struct AiConfig {
    pub model: String,
    pub auth: AuthMode,
}

/// A chat turn's author.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

impl Role {
    pub(crate) fn wire(self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Assistant => "assistant",
        }
    }
}

/// One chat message sent to the API.
#[derive(Clone, Debug)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

/// One event from a streamed completion.
#[derive(Clone, Debug)]
pub enum StreamEvent {
    /// A chunk of assistant text to append.
    Delta(String),
    /// The request failed; carries a human-readable message.
    Error(String),
}
