//! Streaming HTTP client with bounded delivery and request deadlines.

use futures::channel::mpsc::Sender;
use futures::SinkExt;
use futures::StreamExt;
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Duration;

use crate::config::{AiConfig, AuthMode, Message, StreamEvent};

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const OAUTH_BETA: &str = "oauth-2025-04-20";
const MAX_TOKENS: u32 = 4096;

/// Stream a chat completion, sending each text delta over `tx`. On failure a
/// single [`StreamEvent::Error`] is sent instead. Returns when the completion
/// ends, the connection drops, or the receiver is gone.
pub async fn stream_chat(
    config: AiConfig,
    credential: String,
    system: Option<String>,
    messages: Vec<Message>,
    mut tx: Sender<StreamEvent>,
) {
    if let Err(error) = run(&config, &credential, system, &messages, &mut tx).await {
        let _ = tx.send(StreamEvent::Error(error)).await;
    }
}

async fn run(
    config: &AiConfig,
    credential: &str,
    system: Option<String>,
    messages: &[Message],
    tx: &mut Sender<StreamEvent>,
) -> Result<(), String> {
    let payload = build_payload(config, system, messages);

    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    let client = CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60))
            .timeout(Duration::from_secs(300))
            .build()
            .expect("build HTTP client")
    });
    let mut request = client
        .post(API_URL)
        .header("anthropic-version", API_VERSION)
        .header("content-type", "application/json");
    // The one real difference between an API key and a subscription: the header
    // it rides on (plus the oauth beta for subscription tokens).
    request = match config.auth {
        AuthMode::ApiKey => request.header("x-api-key", credential),
        AuthMode::Subscription => request
            .header("authorization", format!("Bearer {credential}"))
            .header("anthropic-beta", OAUTH_BETA),
    };

    let response = request
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let mut stream = response.bytes_stream();
        let mut body = Vec::new();
        while let Some(Ok(chunk)) = stream.next().await {
            let remaining = (64 * 1024usize).saturating_sub(body.len());
            body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
            if body.len() == 64 * 1024 {
                break;
            }
        }
        let body = String::from_utf8_lossy(&body);
        return Err(format!("{status}: {}", extract_error(&body)));
    }

    let mut stream = response.bytes_stream();
    let mut decoder = crate::sse::Decoder::default();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| e.to_string())?;
        for event in decoder.push(&bytes)? {
            let failed = matches!(event, StreamEvent::Error(_));
            if tx.send(event).await.is_err() || failed {
                return Ok(());
            }
        }
        if decoder.finished() {
            return Ok(());
        }
    }
    Err("The response ended before completion. Please try again.".into())
}

fn build_payload(config: &AiConfig, system: Option<String>, messages: &[Message]) -> Value {
    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| json!({ "role": m.role.wire(), "content": m.text }))
        .collect();
    let mut payload = json!({
        "model": config.model,
        "max_tokens": MAX_TOKENS,
        "stream": true,
        "messages": msgs,
    });
    if let Some(system) = system {
        payload["system"] = json!(system);
    }
    payload
}

/// The `error.message` from an API error body, else a truncated raw body.
fn extract_error(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.chars().take(200).collect())
}
