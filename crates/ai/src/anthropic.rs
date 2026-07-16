//! The Anthropic Messages API streaming client (raw HTTP via reqwest — there is
//! no official Rust SDK). Emits each text delta over an unbounded channel so the
//! UI can render tokens as they arrive.

use futures::channel::mpsc::UnboundedSender;
use futures::StreamExt;
use serde_json::{json, Value};

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
    tx: UnboundedSender<StreamEvent>,
) {
    if let Err(error) = run(&config, &credential, system, &messages, &tx).await {
        let _ = tx.unbounded_send(StreamEvent::Error(error));
    }
}

async fn run(
    config: &AiConfig,
    credential: &str,
    system: Option<String>,
    messages: &[Message],
    tx: &UnboundedSender<StreamEvent>,
) -> Result<(), String> {
    let payload = build_payload(config, system, messages);

    let client = reqwest::Client::new();
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

    let response = request.json(&payload).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("{status}: {}", extract_error(&body)));
    }

    // Anthropic streams Server-Sent Events. Frames are separated by a blank
    // line; buffer bytes and only decode complete frames so a multibyte UTF-8
    // character split across chunks is never corrupted.
    let mut stream = response.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| e.to_string())?;
        buf.extend_from_slice(&bytes);
        while let Some(pos) = find(&buf, b"\n\n") {
            let frame: Vec<u8> = buf.drain(..pos + 2).collect();
            let text = String::from_utf8_lossy(&frame[..pos]);
            if let Some(delta) = parse_frame(&text) {
                if tx.unbounded_send(StreamEvent::Delta(delta)).is_err() {
                    return Ok(()); // panel closed — stop quietly
                }
            }
        }
    }
    Ok(())
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

/// Pull the text out of one SSE frame's `data:` line, if it is a text delta.
fn parse_frame(frame: &str) -> Option<String> {
    let data = frame.lines().find_map(|l| l.strip_prefix("data:"))?.trim();
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    let value: Value = serde_json::from_str(data).ok()?;
    if value.get("type")?.as_str()? != "content_block_delta" {
        return None;
    }
    let delta = value.get("delta")?;
    if delta.get("type")?.as_str()? != "text_delta" {
        return None;
    }
    Some(delta.get("text")?.as_str()?.to_string())
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

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Role;

    #[test]
    fn parses_a_text_delta_frame() {
        let frame = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}";
        assert_eq!(parse_frame(frame), Some("Hello".to_string()));
    }

    #[test]
    fn ignores_non_delta_frames() {
        assert_eq!(parse_frame("data: {\"type\":\"message_stop\"}"), None);
        assert_eq!(parse_frame("event: ping\ndata: {\"type\":\"ping\"}"), None);
        assert_eq!(parse_frame("data: [DONE]"), None);
        assert_eq!(parse_frame(": comment only"), None);
    }

    #[test]
    fn find_locates_the_frame_boundary() {
        assert_eq!(find(b"abc\n\ndef", b"\n\n"), Some(3));
        assert_eq!(find(b"no boundary", b"\n\n"), None);
    }

    #[test]
    fn build_payload_sets_stream_and_system() {
        let config = AiConfig { model: "claude-opus-4-8".into(), auth: AuthMode::ApiKey };
        let messages = vec![Message { role: Role::User, text: "hi".into() }];
        let payload = build_payload(&config, Some("be terse".into()), &messages);
        assert_eq!(payload["stream"], json!(true));
        assert_eq!(payload["model"], json!("claude-opus-4-8"));
        assert_eq!(payload["system"], json!("be terse"));
        assert_eq!(payload["messages"][0]["role"], json!("user"));
        assert_eq!(payload["messages"][0]["content"], json!("hi"));
    }

    #[test]
    fn extract_error_reads_the_message() {
        let body = r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#;
        assert_eq!(extract_error(body), "invalid x-api-key");
    }
}
