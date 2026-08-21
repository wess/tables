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

/// One model the picker offers.
///
/// Carries what the picker shows beside the name — the context window and the
/// per-million token prices — so the cost meter has real numbers rather than a
/// second table that drifts from this one.
#[derive(Clone, Copy, Debug)]
pub struct ModelInfo {
    /// What the API is called with.
    pub id: &'static str,
    /// What the user reads.
    pub label: &'static str,
    /// One line on what it is for.
    pub description: &'static str,
    /// Context window in tokens.
    pub context: u64,
    /// USD per million input tokens.
    pub input_per_million: f64,
    /// USD per million output tokens.
    pub output_per_million: f64,
}

/// The models the picker offers, most-capable first.
///
/// Opus 4.8 stays on the list below its successor so a settings file pinned to
/// it keeps the model it asked for instead of being silently moved.
pub const MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "claude-opus-5",
        label: "Claude Opus 5",
        description: "Most capable. Best for schema design and tricky SQL.",
        context: 1_000_000,
        input_per_million: 5.0,
        output_per_million: 25.0,
    },
    ModelInfo {
        id: "claude-opus-4-8",
        label: "Claude Opus 4.8",
        description: "The previous Opus.",
        context: 1_000_000,
        input_per_million: 5.0,
        output_per_million: 25.0,
    },
    ModelInfo {
        id: "claude-sonnet-5",
        label: "Claude Sonnet 5",
        description: "Balanced. A good default for everyday queries.",
        context: 1_000_000,
        input_per_million: 3.0,
        output_per_million: 15.0,
    },
    ModelInfo {
        id: "claude-haiku-4-5",
        label: "Claude Haiku 4.5",
        description: "Fastest and cheapest. Good for quick lookups.",
        context: 200_000,
        input_per_million: 1.0,
        output_per_million: 5.0,
    },
];

/// The default model when nothing is configured.
pub const DEFAULT_MODEL: &str = "claude-opus-5";

/// The entry for a model id, or `None` when the id is not one we offer.
pub fn model_info(id: &str) -> Option<&'static ModelInfo> {
    MODELS.iter().find(|m| m.id == id)
}

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

/// What a completion cost, as the API reports it.
///
/// Arrives in two halves: `message_start` carries the input counts (the whole
/// prompt is known before a token is generated) and `message_delta` carries the
/// running output count, so the last one wins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
}

impl Usage {
    /// Merge a later frame's counts. Zero means "this frame didn't say",
    /// not "zero tokens" — only `message_start` reports input.
    pub fn merge(&mut self, other: Usage) {
        if other.input > 0 {
            self.input = other.input;
        }
        if other.output > 0 {
            self.output = other.output;
        }
        if other.cache_read > 0 {
            self.cache_read = other.cache_read;
        }
    }

    /// What this cost in USD at a model's per-million rates.
    pub fn cost(&self, input_per_million: f64, output_per_million: f64) -> f64 {
        (self.input as f64 / 1_000_000.0) * input_per_million
            + (self.output as f64 / 1_000_000.0) * output_per_million
    }
}

/// One event from a streamed completion.
#[derive(Clone, Debug)]
pub enum StreamEvent {
    /// A chunk of assistant text to append.
    Delta(String),
    /// Updated token counts for the turn in flight.
    Usage(Usage),
    /// The request failed; carries a human-readable message.
    Error(String),
}
