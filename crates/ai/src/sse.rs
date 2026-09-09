use serde_json::Value;

use crate::config::{StreamEvent, Usage};

const FRAME_LIMIT: usize = 64 * 1024;

#[derive(Default)]
pub struct Decoder {
  line: Vec<u8>,
  data: String,
  finished: bool,
}

impl Decoder {
  pub fn finished(&self) -> bool {
    self.finished
  }

  pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<StreamEvent>, String> {
    let mut events = Vec::new();
    for byte in bytes {
      if self.finished {
        break;
      }
      if self.line.len() + self.data.len() >= FRAME_LIMIT {
        return Err("Response frame exceeded 64 KiB".into());
      }
      if *byte != b'\n' {
        self.line.push(*byte);
        continue;
      }
      let line = std::str::from_utf8(&self.line)
        .map_err(|_| "Response contained invalid UTF-8")?
        .trim_end_matches('\r');
      if line.is_empty() {
        if !self.data.is_empty() {
          let value: Value = serde_json::from_str(&self.data)
            .map_err(|_| "Response contained invalid event data")?;
          match value["type"].as_str() {
            Some("message_stop") => self.finished = true,
            Some("error") => {
              events.push(StreamEvent::Error(
                value["error"]["message"]
                  .as_str()
                  .unwrap_or("The request failed while streaming")
                  .to_string(),
              ));
              self.finished = true;
            }
            Some("content_block_delta") if value["delta"]["type"] == "text_delta" => {
              if let Some(text) = value["delta"]["text"].as_str() {
                events.push(StreamEvent::Delta(text.to_string()));
              }
            }
            Some("message_start") => {
              events.push(StreamEvent::Usage(usage(&value["message"]["usage"])))
            }
            Some("message_delta") => events.push(StreamEvent::Usage(usage(&value["usage"]))),
            _ => {}
          }
          self.data.clear();
        }
      } else if let Some(data) = line.strip_prefix("data:") {
        if !self.data.is_empty() {
          self.data.push('\n');
        }
        self.data.push_str(data.strip_prefix(' ').unwrap_or(data));
      }
      self.line.clear();
    }
    Ok(events)
  }
}

fn usage(value: &Value) -> Usage {
  let count = |key: &str| value[key].as_u64().unwrap_or(0);
  Usage {
    input: count("input_tokens"),
    output: count("output_tokens"),
    cache_read: count("cache_read_input_tokens"),
  }
}
