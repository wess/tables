use std::sync::Arc;

use db::{Registry, SharedAdapter};
use serde_json::{json, Map, Value};

#[derive(Default)]
pub struct Session {
  registry: Arc<Registry>,
  connection: Option<String>,
}

impl Session {
  pub async fn close(&mut self) {
    if let Some(id) = self.connection.take() {
      self.registry.disconnect(&id).await;
    }
  }

  pub async fn call(&mut self, name: &str, args: Map<String, Value>) -> Result<Value, String> {
    let definition = crate::catalog::tools()
      .into_iter()
      .find(|tool| tool.name == name)
      .ok_or("Unknown tool")?;
    let properties = definition.input_schema["properties"].as_object().unwrap();
    if args.keys().any(|key| !properties.contains_key(key)) {
      return Err("Unknown argument".into());
    }
    if name == "list_connections" {
      let connections: Vec<Value> = store::connections::load()
        .into_iter()
        .map(|c| json!({"id":c.id, "name":c.name, "type":c.kind}))
        .collect();
      return Ok(json!({"connections":connections}));
    }
    let id = text(&args, "connection", 256)?;
    let table = if name == "list_tables" {
      None
    } else {
      Some(text(&args, "table", 1024)?)
    };
    let limit = number(&args, "limit", 50, 1, 200)?;
    let offset = number(&args, "offset", 0, 0, 1_000_000)?;
    let adapter = self.connect(id).await?;
    if name == "list_tables" {
      return Ok(json!({"tables":adapter.get_tables().await?}));
    }
    let table = table.unwrap();
    // resolve names against metadata; quoted input alone must not expose hidden relations.
    if !adapter.get_tables().await?.iter().any(|t| t.name == table) {
      return Err("Table not found in this connection".into());
    }
    let columns = adapter.get_columns(table).await?;
    if name == "table_schema" {
      return Ok(
        json!({"columns":columns, "indexes":adapter.get_indexes(table).await?,
        "foreignKeys":adapter.get_foreign_keys(table).await?}),
      );
    }
    let dialect = adapter.dialect();
    let keys: Vec<String> = columns
      .iter()
      .filter(|c| c.is_primary_key)
      .map(|c| dialect.quote_ident(&c.name))
      .collect();
    let order = if keys.is_empty() {
      String::new()
    } else {
      format!("ORDER BY {}", keys.join(", "))
    };
    let sql = format!(
      "SELECT * FROM {} {order} LIMIT {} OFFSET {offset}",
      dialect.quote_ident(table),
      limit + 1
    );
    let mut result = adapter.query_bounded(&sql, 201, 512 * 1024).await?;
    if result.columns.is_empty() {
      result.columns = columns.iter().map(|c| c.name.clone()).collect();
    }
    let more = result.rows.len() > limit as usize;
    result.rows.truncate(limit as usize);
    let mut truncated = 0;
    for row in &mut result.rows {
      for value in row.values_mut() {
        if let Value::String(text) = value {
          if text.len() > 4096 {
            let mut end = 4096;
            while !text.is_char_boundary(end) {
              end -= 1;
            }
            text.truncate(end);
            truncated += 1;
          }
        }
      }
    }
    Ok(json!({"columns":result.columns, "rows":result.rows,
      "nextOffset":if more { Some(offset + limit) } else { None }, "truncatedCells":truncated}))
  }

  async fn connect(&mut self, id: &str) -> Result<SharedAdapter, String> {
    if self.connection.as_deref() != Some(id) {
      self.close().await;
    }
    if !self.registry.is_connected(id) {
      let conn = store::connections::find(id).ok_or("Saved connection not found")?;
      let mut config = conn.config();
      if config.password.is_empty() && config.kind != "sqlite" {
        config.password = store::keychain::get_secret(id).unwrap_or_default();
      }
      // keep the id before awaiting so a timeout can close a half-open tunnel.
      self.connection = Some(id.to_string());
      self.registry.connect_readonly(&config).await?;
    }
    self.registry.adapter(id)
  }
}

fn text<'a>(args: &'a Map<String, Value>, key: &str, max: usize) -> Result<&'a str, String> {
  args
    .get(key)
    .and_then(Value::as_str)
    .filter(|v| !v.is_empty() && v.len() <= max)
    .ok_or_else(|| format!("{key} must be a nonempty string of at most {max} bytes"))
}

fn number(
  args: &Map<String, Value>,
  key: &str,
  default: u64,
  min: u64,
  max: u64,
) -> Result<u64, String> {
  match args.get(key) {
    None => Ok(default),
    Some(value) => value
      .as_u64()
      .filter(|v| (min..=max).contains(v))
      .ok_or_else(|| format!("{key} must be an integer between {min} and {max}")),
  }
}
