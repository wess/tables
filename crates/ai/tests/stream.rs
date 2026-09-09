use ai as config;
use ai::{StreamEvent, Usage};

#[path = "../src/sse.rs"]
mod sse;

#[test]
fn every_byte_boundary_preserves_unicode_and_crlf() {
  let wire = "event: content_block_delta\r\ndata: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"héllo 世界\"}}\r\n\r\ndata: {\"type\":\"message_stop\"}\r\n\r\n";
  for split in 0..wire.len() {
    let mut decoder = sse::Decoder::default();
    let mut events = decoder.push(&wire.as_bytes()[..split]).unwrap();
    events.extend(decoder.push(&wire.as_bytes()[split..]).unwrap());
    assert!(decoder.finished());
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], StreamEvent::Delta(text) if text == "héllo 世界"));
  }
}

#[test]
fn server_error_is_terminal_and_visible() {
  let mut decoder = sse::Decoder::default();
  let events = decoder
    .push(b"data: {\"type\":\"error\",\"error\":{\"message\":\"overloaded\"}}\n\n")
    .unwrap();
  assert!(matches!(&events[0], StreamEvent::Error(text) if text == "overloaded"));
  assert!(decoder.finished());
  assert!(decoder.push(b"data: nonsense\n\n").unwrap().is_empty());
}

#[test]
fn usage_frames_merge_without_erasing_input() {
  let mut decoder = sse::Decoder::default();
  let events = decoder.push(b"data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":120,\"cache_read_input_tokens\":80}}}\n\ndata: {\"type\":\"message_delta\",\"usage\":{\"output_tokens\":42}}\n\n").unwrap();
  let mut total = Usage::default();
  for event in events {
    if let StreamEvent::Usage(usage) = event {
      total.merge(usage);
    }
  }
  assert_eq!(
    total,
    Usage {
      input: 120,
      output: 42,
      cache_read: 80
    }
  );
  assert!(!decoder.finished());
}

#[test]
fn rejects_malformed_and_unbounded_frames() {
  assert!(sse::Decoder::default().push(b"data: nope\n\n").is_err());
  assert!(sse::Decoder::default()
    .push(&vec![b'a'; 65 * 1024])
    .is_err());
}

#[test]
fn handles_multiline_data_comments_and_unknown_events() {
  let mut decoder = sse::Decoder::default();
  assert!(decoder
    .push(b": keepalive\n\nevent: ping\ndata: {\"type\":\n data: ignored\ndata: \"ping\"}\n\n")
    .unwrap()
    .is_empty());
  assert!(!decoder.finished());
}

#[test]
fn a_partial_frame_is_not_a_completed_reply() {
  let mut decoder = sse::Decoder::default();
  assert!(decoder
    .push(b"data: {\"type\":\"message_stop\"}")
    .unwrap()
    .is_empty());
  assert!(!decoder.finished());
}

#[test]
fn cost_uses_per_million_rates() {
  let usage = Usage {
    input: 1_000_000,
    output: 200_000,
    cache_read: 0,
  };
  assert!((usage.cost(5.0, 25.0) - 10.0).abs() < 1e-9);
}
