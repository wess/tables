//! The Anthropic Messages API streaming client (raw HTTP via reqwest — there is
//! no official Rust SDK). Emits each text delta over an unbounded channel so the
//! UI can render tokens as they arrive.

use futures::channel::mpsc::UnboundedSender;
use futures::StreamExt;
use serde_json::{json, Value};

use crate::config::{AiConfig, AuthMode, Message, StreamEvent, Usage};

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
            if let Some(event) = parse_frame(&text) {
                if tx.unbounded_send(event).is_err() {
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

/// Turn one SSE frame into an event, if it carries anything we render.
///
/// Three frame types matter: `content_block_delta` for text, and
/// `message_start` / `message_delta` for the token counts that feed the cost
/// meter. Everything else (pings, block starts and stops) is skipped.
fn parse_frame(frame: &str) -> Option<StreamEvent> {
    let data = frame.lines().find_map(|l| l.strip_prefix("data:"))?.trim();
    if data.is_empty() || data == "[DONE]" {
        return None;
    }
    let value: Value = serde_json::from_str(data).ok()?;
    match value.get("type")?.as_str()? {
        "content_block_delta" => {
            let delta = value.get("delta")?;
            if delta.get("type")?.as_str()? != "text_delta" {
                return None;
            }
            Some(StreamEvent::Delta(delta.get("text")?.as_str()?.to_string()))
        }
        "message_start" => {
            let usage = read_usage(value.get("message")?.get("usage")?);
            Some(StreamEvent::Usage(usage))
        }
        "message_delta" => {
            let usage = read_usage(value.get("usage")?);
            Some(StreamEvent::Usage(usage))
        }
        _ => None,
    }
}

/// Read the token counts a usage object carries. Absent fields read as zero,
/// which [`Usage::merge`] treats as "not reported by this frame".
fn read_usage(usage: &Value) -> Usage {
    let field = |name: &str| usage.get(name).and_then(Value::as_u64).unwrap_or(0);
    Usage {
        input: field("input_tokens"),
        output: field("output_tokens"),
        cache_read: field("cache_read_input_tokens"),
    }
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

    fn text_of(frame: &str) -> Option<String> {
        match parse_frame(frame) {
            Some(StreamEvent::Delta(text)) => Some(text),
            _ => None,
        }
    }

    fn usage_of(frame: &str) -> Option<Usage> {
        match parse_frame(frame) {
            Some(StreamEvent::Usage(usage)) => Some(usage),
            _ => None,
        }
    }

    #[test]
    fn parses_a_text_delta_frame() {
        let frame = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}";
        assert_eq!(text_of(frame), Some("Hello".to_string()));
    }

    #[test]
    fn ignores_non_delta_frames() {
        assert!(parse_frame("data: {\"type\":\"message_stop\"}").is_none());
        assert!(parse_frame("event: ping\ndata: {\"type\":\"ping\"}").is_none());
        assert!(parse_frame("data: [DONE]").is_none());
        assert!(parse_frame(": comment only").is_none());
    }

    #[test]
    fn reads_input_tokens_from_message_start() {
        let frame = "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":120,\"cache_read_input_tokens\":80}}}";
        let usage = usage_of(frame).expect("usage");
        assert_eq!(usage.input, 120);
        assert_eq!(usage.cache_read, 80);
        assert_eq!(usage.output, 0);
    }

    #[test]
    fn reads_output_tokens_from_message_delta() {
        let frame = "data: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":42}}";
        assert_eq!(usage_of(frame).expect("usage").output, 42);
    }

    #[test]
    fn merge_keeps_the_input_count_a_later_frame_omits() {
        // `message_delta` reports only output; the input count from
        // `message_start` has to survive it or the cost halves mid-stream.
        let mut usage = Usage { input: 120, output: 0, cache_read: 80 };
        usage.merge(Usage { input: 0, output: 42, cache_read: 0 });
        assert_eq!(usage, Usage { input: 120, output: 42, cache_read: 80 });
    }

    #[test]
    fn cost_is_per_million_tokens() {
        let usage = Usage { input: 1_000_000, output: 200_000, cache_read: 0 };
        // $5/Mtok in, $25/Mtok out => 5.00 + 5.00
        assert!((usage.cost(5.0, 25.0) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn find_locates_the_frame_boundary() {
        assert_eq!(find(b"abc\n\ndef", b"\n\n"), Some(3));
        assert_eq!(find(b"no boundary", b"\n\n"), None);
    }

    #[test]
    fn build_payload_sets_stream_and_system() {
        let config = AiConfig { model: "claude-opus-5".into(), auth: AuthMode::ApiKey };
        let messages = vec![Message { role: Role::User, text: "hi".into() }];
        let payload = build_payload(&config, Some("be terse".into()), &messages);
        assert_eq!(payload["stream"], json!(true));
        assert_eq!(payload["model"], json!("claude-opus-5"));
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
