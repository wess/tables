use rmcp::model::{Tool, ToolAnnotations};
use serde_json::{json, Value};

pub fn tools() -> Vec<Tool> {
  let connection = json!({"type":"string", "minLength":1, "maxLength":256});
  let table = json!({"type":"string", "minLength":1, "maxLength":1024});
  vec![
    tool(
      "list_connections",
      "List saved connection IDs, names and database types.",
      json!({}),
      &[],
    ),
    tool(
      "list_tables",
      "List tables and views in a saved connection.",
      json!({"connection":connection}),
      &["connection"],
    ),
    tool(
      "table_schema",
      "Inspect columns, indexes and foreign keys for one table.",
      json!({"connection":connection, "table":table}),
      &["connection", "table"],
    ),
    tool(
      "table_rows",
      "Read a bounded page of a table. Values may be truncated; inspect truncatedCells.",
      json!({"connection":connection, "table":table,
        "limit":{"type":"integer", "minimum":1, "maximum":200, "default":50},
        "offset":{"type":"integer", "minimum":0, "maximum":1000000, "default":0}}),
      &["connection", "table"],
    ),
  ]
}

fn tool(
  name: &'static str,
  description: &'static str,
  properties: Value,
  required: &[&str],
) -> Tool {
  let schema = json!({"type":"object", "properties":properties,
    "required":required, "additionalProperties":false});
  let mut tool = Tool::new(name, description, schema.as_object().unwrap().clone());
  tool.annotations = Some(
    ToolAnnotations::new()
      .read_only(true)
      .destructive(false)
      .idempotent(true)
      .open_world(false),
  );
  tool
}
