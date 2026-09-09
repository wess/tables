use std::io::{BufWriter, Write};
use std::path::Path;

use db::{Dialect, SharedAdapter};
use model::Row;
use serde_json::{Map, Value};
use tempfile::NamedTempFile;
use tokio::sync::mpsc;

use crate::data::{csv_field, insert_statements, stringify_cell};

pub async fn backup(adapter: SharedAdapter, path: &str) -> Result<u64, String> {
  let tables = adapter.get_tables().await?;
  let parent = Path::new(path)
    .parent()
    .filter(|p| !p.as_os_str().is_empty())
    .unwrap_or(Path::new("."));
  let mut staged = NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
  staged
    .write_all(b"-- Tables backup\n\n")
    .map_err(|e| e.to_string())?;
  let mut count = 0;
  for table in tables.into_iter().filter(|t| t.kind == "table") {
    let ddl = adapter.get_ddl(&table.name).await?;
    let ddl = ddl.trim().trim_end_matches(';');
    if ddl.is_empty() {
      return Err(format!("No schema returned for {}", table.name));
    }
    writeln!(staged, "{ddl};\n").map_err(|e| e.to_string())?;
    let rows = NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    write(
      adapter.clone(),
      &table.name,
      "sql",
      rows.path().to_str().ok_or("Invalid export path")?,
      &Map::new(),
    )
    .await?;
    staged = tokio::task::spawn_blocking(move || -> Result<NamedTempFile, String> {
      let mut source = std::fs::File::open(rows.path()).map_err(|e| e.to_string())?;
      std::io::copy(&mut source, &mut staged).map_err(|e| e.to_string())?;
      staged.write_all(b"\n").map_err(|e| e.to_string())?;
      Ok(staged)
    })
    .await
    .map_err(|e| e.to_string())??;
    count += 1;
  }
  let path = path.to_string();
  tokio::task::spawn_blocking(move || {
    staged.as_file().sync_all().map_err(|e| e.to_string())?;
    staged
      .persist(path)
      .map(|_| count)
      .map_err(|e| e.to_string())
  })
  .await
  .map_err(|e| e.to_string())?
}

pub async fn write(
  adapter: SharedAdapter,
  table: &str,
  format: &str,
  path: &str,
  options: &Map<String, Value>,
) -> Result<u64, String> {
  if !matches!(format, "csv" | "json" | "sql") {
    return Err(format!("Unknown export format: {format}"));
  }
  let columns = adapter
    .get_columns(table)
    .await?
    .into_iter()
    .map(|c| c.name)
    .collect();
  let dialect = adapter.dialect();
  let (tx, rx) = mpsc::channel(8);
  let (table_name, format, path, options) = (
    table.to_string(),
    format.to_string(),
    path.to_string(),
    options.clone(),
  );
  let destination = path.clone();
  let writer = tokio::task::spawn_blocking(move || {
    encode(rx, dialect, &table_name, columns, &format, &path, &options)
  });
  let result = adapter
    .stream_rows(&format!("SELECT * FROM {}", dialect.quote_ident(table)), tx)
    .await;
  let (staged, count) = writer.await.map_err(|e| e.to_string())??;
  result?;
  tokio::task::spawn_blocking(move || {
    staged
      .persist(destination)
      .map(|_| count)
      .map_err(|e| e.to_string())
  })
  .await
  .map_err(|e| e.to_string())?
}

fn encode(
  mut rows: mpsc::Receiver<Row>,
  dialect: Dialect,
  table: &str,
  columns: Vec<String>,
  format: &str,
  path: &str,
  options: &Map<String, Value>,
) -> Result<(NamedTempFile, u64), String> {
  let parent = Path::new(path)
    .parent()
    .filter(|p| !p.as_os_str().is_empty())
    .unwrap_or(Path::new("."));
  let mut staged = NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
  let mut out = BufWriter::new(staged.as_file_mut());
  let delimiter = options
    .get("delimiter")
    .and_then(Value::as_str)
    .filter(|s| !s.is_empty())
    .unwrap_or(",");
  let null = options.get("nullAs").and_then(Value::as_str).unwrap_or("");
  let mut count = 0;
  let result = (|| -> std::io::Result<()> {
    if format == "json" {
      out.write_all(b"[\n")?;
    }
    if format == "csv" && options.get("includeHeaders").and_then(Value::as_bool) != Some(false) {
      writeln!(
        out,
        "{}",
        columns
          .iter()
          .map(|c| csv_field(c, delimiter))
          .collect::<Vec<_>>()
          .join(delimiter)
      )?;
    }
    while let Some(row) = rows.blocking_recv() {
      match format {
        "json" => {
          if count > 0 {
            out.write_all(b",\n")?;
          }
          serde_json::to_writer(&mut out, &row)?;
        }
        "csv" => {
          let fields = columns
            .iter()
            .map(|column| match row.get(column) {
              None | Some(Value::Null) => csv_field(null, delimiter),
              Some(value) => csv_field(&stringify_cell(value), delimiter),
            })
            .collect::<Vec<_>>();
          writeln!(out, "{}", fields.join(delimiter))?;
        }
        "sql" => writeln!(
          out,
          "{}",
          insert_statements(dialect, table, &columns, &[row], true)
        )?,
        _ => unreachable!(),
      }
      count += 1;
    }
    if format == "json" {
      out.write_all(b"\n]\n")?;
    }
    out.flush()
  })();
  result.map_err(|e| e.to_string())?;
  drop(out);
  staged.as_file().sync_all().map_err(|e| e.to_string())?;
  Ok((staged, count))
}
