//! Export, import, mock-data, schema-compare, and profiling handlers.

use serde_json::{Map, Value};

use db::Dialect;
use db::SharedAdapter;
use model::{
    ColumnInfo, ColumnProfile, ExportFileResult, ImportResult, ImportSqlResult, Row, SchemaDiff,
    TableInfo, TopValue,
};

use crate::facade::{row_i64, Host};
use crate::{compare, csv, mock, values};

impl Host {
    /// A query or a whole table serialized to CSV / JSON / SQL text, in-memory.
    pub async fn export_data(
        &self,
        table: Option<&str>,
        sql: Option<&str>,
        format: &str,
    ) -> Result<String, String> {
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();
        let result = if let Some(sql) = sql.filter(|s| !s.is_empty()) {
            adapter.query(sql).await?
        } else if let Some(table) = table.filter(|t| !t.is_empty()) {
            adapter.query(&format!("SELECT * FROM {}", dialect.quote_ident(table))).await?
        } else {
            return Err("No table or SQL provided for export".into());
        };

        match format {
            "csv" => {
                let mut lines = vec![result.columns.join(",")];
                for row in &result.rows {
                    let cells: Vec<String> = result
                        .columns
                        .iter()
                        .map(|col| match row.get(col) {
                            None | Some(Value::Null) => String::new(),
                            Some(v) => csv_field(&stringify_cell(v), ","),
                        })
                        .collect();
                    lines.push(cells.join(","));
                }
                Ok(lines.join("\n"))
            }
            "json" => serde_json::to_string_pretty(&result.rows).map_err(|e| e.to_string()),
            "sql" => {
                let table = table
                    .filter(|t| !t.is_empty())
                    .ok_or_else(|| "Table name required for SQL export".to_string())?;
                Ok(insert_statements(dialect, table, &result.columns, &result.rows, false))
            }
            other => Err(format!("Unknown format: {other}")),
        }
    }

    /// Like `export_data` but for a single table with CSV options, written to
    /// `path`. No path returns `{ path: null, rows: 0 }`.
    pub async fn export_file(
        &self,
        table: &str,
        format: &str,
        path: Option<&str>,
        options: &Map<String, Value>,
    ) -> Result<ExportFileResult, String> {
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();
        let result = adapter
            .query(&format!("SELECT * FROM {}", dialect.quote_ident(table)))
            .await?;

        let content = match format {
            "csv" => {
                let delim = options
                    .get("delimiter")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or(",");
                let null_as = options.get("nullAs").and_then(Value::as_str).unwrap_or("");
                let include_headers =
                    options.get("includeHeaders").and_then(Value::as_bool) != Some(false);

                let mut lines: Vec<String> = Vec::new();
                if include_headers {
                    lines.push(
                        result
                            .columns
                            .iter()
                            .map(|c| csv_field(c, delim))
                            .collect::<Vec<_>>()
                            .join(delim),
                    );
                }
                for row in &result.rows {
                    let cells: Vec<String> = result
                        .columns
                        .iter()
                        .map(|col| match row.get(col) {
                            None | Some(Value::Null) => null_as.to_string(),
                            Some(v) => csv_field(&stringify_cell(v), delim),
                        })
                        .collect();
                    lines.push(cells.join(delim));
                }
                lines.join("\n")
            }
            "json" => serde_json::to_string_pretty(&result.rows).map_err(|e| e.to_string())?,
            "sql" => insert_statements(dialect, table, &result.columns, &result.rows, true),
            other => return Err(format!("Unknown export format: {other}")),
        };

        let Some(path) = path.filter(|p| !p.is_empty()) else {
            return Ok(ExportFileResult { path: None, rows: 0 });
        };
        std::fs::write(path, content).map_err(|e| e.to_string())?;
        Ok(ExportFileResult { path: Some(path.to_string()), rows: result.rows.len() as u64 })
    }

    /// Serialize an already-fetched result set (columns + rows) to `format`
    /// (`csv` / `json` / `sql` / `markdown` / `tsv`). Sync — the data is in
    /// memory — so it drives both clipboard copy and file export. `table` names
    /// the target for SQL `INSERT`s.
    pub fn serialize_result(
        &self,
        columns: &[String],
        rows: &[Row],
        format: &str,
        table: Option<&str>,
    ) -> Result<String, String> {
        let dialect = self.active_adapter().map(|a| a.dialect()).unwrap_or(Dialect::Sqlite);
        match format {
            "csv" | "tsv" => {
                let delim = if format == "tsv" { "\t" } else { "," };
                let mut lines = vec![columns
                    .iter()
                    .map(|c| csv_field(c, delim))
                    .collect::<Vec<_>>()
                    .join(delim)];
                for row in rows {
                    let cells: Vec<String> = columns
                        .iter()
                        .map(|col| match row.get(col) {
                            None | Some(Value::Null) => String::new(),
                            Some(v) => csv_field(&stringify_cell(v), delim),
                        })
                        .collect();
                    lines.push(cells.join(delim));
                }
                Ok(lines.join("\n"))
            }
            "json" => serde_json::to_string_pretty(rows).map_err(|e| e.to_string()),
            "sql" => Ok(insert_statements(
                dialect,
                table.filter(|t| !t.is_empty()).unwrap_or("query_result"),
                columns,
                rows,
                true,
            )),
            "markdown" => Ok(markdown_table(columns, rows)),
            other => Err(format!("Unknown format: {other}")),
        }
    }

    /// Serialize a result set and write it to `path`; returns the row count.
    pub fn write_result(
        &self,
        columns: &[String],
        rows: &[Row],
        format: &str,
        path: &str,
        table: Option<&str>,
    ) -> Result<u64, String> {
        let content = self.serialize_result(columns, rows, format, table)?;
        std::fs::write(path, content).map_err(|e| e.to_string())?;
        Ok(rows.len() as u64)
    }

    /// Run one statement from inline text or a file path.
    pub async fn import_sql(
        &self,
        sql: Option<&str>,
        path: Option<&str>,
    ) -> Result<ImportSqlResult, String> {
        let adapter = self.active_adapter()?;
        let sql = match path.filter(|p| !p.is_empty()) {
            Some(path) => std::fs::read_to_string(path).map_err(|e| e.to_string())?,
            None => sql.unwrap_or("").to_string(),
        };
        if sql.is_empty() {
            return Ok(ImportSqlResult {
                success: false,
                error: Some("No SQL provided".into()),
                rows_affected: 0,
            });
        }
        Ok(match adapter.query(&sql).await {
            Ok(raw) => ImportSqlResult {
                success: true,
                error: None,
                rows_affected: raw.rows_affected,
            },
            Err(error) => ImportSqlResult { success: false, error: Some(error), rows_affected: 0 },
        })
    }

    /// Inline CSV to parameterized INSERTs, applied in chunked transactions.
    pub async fn import_csv(
        &self,
        table: &str,
        csv: &str,
        delimiter: Option<&str>,
    ) -> Result<ImportResult, String> {
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();
        let delimiter = delimiter.filter(|d| !d.is_empty()).unwrap_or(",");
        let col_types = crate::facade::pg_col_types(&adapter, dialect, table).await;
        let batch = csv::csv_to_insert_params(dialect, table, csv, delimiter, &col_types);
        Ok(run_import_params(&adapter, batch).await)
    }

    /// Read a CSV/TSV file (tab delimiter for `.tsv`).
    pub async fn import_csv_file(&self, table: &str, path: &str) -> Result<ImportResult, String> {
        let csv = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();
        let delimiter = if path.ends_with(".tsv") { "\t" } else { "," };
        let col_types = crate::facade::pg_col_types(&adapter, dialect, table).await;
        let batch = csv::csv_to_insert_params(dialect, table, &csv, delimiter, &col_types);
        Ok(run_import_params(&adapter, batch).await)
    }

    /// Generate and insert fake rows (default 10).
    pub async fn mock_data(&self, table: &str, count: usize) -> Result<ImportResult, String> {
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();
        let columns = adapter.get_columns(table).await?;
        let count = if count == 0 { 10 } else { count };
        let rows = mock::generate_mock_rows(&columns, count);
        let total = rows.len() as u64;
        let quoted_table = dialect.quote_ident(table);

        let statements: Vec<String> = rows
            .iter()
            .filter(|row| !row.is_empty())
            .map(|row| {
                let cols =
                    row.keys().map(|k| dialect.quote_ident(k)).collect::<Vec<_>>().join(", ");
                let vals = row
                    .values()
                    .map(|v| values::literal_bool_kw(dialect, v))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("INSERT INTO {quoted_table} ({cols}) VALUES ({vals})")
            })
            .collect();
        let result = run_import(&adapter, statements).await;
        Ok(ImportResult { inserted: result.inserted, total, error: result.error })
    }

    /// Diff two live connections' table schemas.
    pub async fn compare_schemas(
        &self,
        source_id: &str,
        target_id: &str,
    ) -> Result<Vec<SchemaDiff>, String> {
        let source = self.registry.adapter(source_id)?;
        let target = self.registry.adapter(target_id)?;
        let source_tables = source.get_tables().await?;
        let target_tables = target.get_tables().await?;
        let source_schemas = table_columns(&source, &source_tables).await?;
        let target_schemas = table_columns(&target, &target_tables).await?;
        Ok(compare::compare_schemas(&source_schemas, &target_schemas))
    }

    /// Per-column stats for the active connection's table.
    pub async fn profile_table(&self, table: &str) -> Result<Vec<ColumnProfile>, String> {
        let adapter = self.active_adapter()?;
        let dialect = adapter.dialect();
        let columns = adapter.get_columns(table).await?;
        let quoted_table = dialect.quote_ident(table);
        let count = adapter
            .query(&format!("SELECT COUNT(*) as total FROM {quoted_table}"))
            .await?;
        let total_rows = count.rows.first().map(|row| row_i64(row, "total")).unwrap_or(0);
        let mut profiles = Vec::with_capacity(columns.len());
        for col in &columns {
            profiles.push(profile_column(&adapter, dialect, &quoted_table, col, total_rows).await);
        }
        Ok(profiles)
    }
}

/// Wrap a value as an unescaped CSV/text cell.
fn stringify_cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// A GitHub-flavored Markdown table for copy-to-clipboard. Pipes and newlines
/// in cells are escaped so the table stays on its grid.
fn markdown_table(columns: &[String], rows: &[Row]) -> String {
    let esc = |s: &str| s.replace('|', "\\|").replace('\n', " ");
    let mut out = vec![
        format!("| {} |", columns.iter().map(|c| esc(c)).collect::<Vec<_>>().join(" | ")),
        format!("| {} |", columns.iter().map(|_| "---").collect::<Vec<_>>().join(" | ")),
    ];
    for row in rows {
        let cells: Vec<String> = columns
            .iter()
            .map(|col| match row.get(col) {
                None | Some(Value::Null) => String::new(),
                Some(v) => esc(&stringify_cell(v)),
            })
            .collect();
        out.push(format!("| {} |", cells.join(" | ")));
    }
    out.join("\n")
}

/// Quote a CSV field only when it contains the delimiter, a quote, or a newline.
fn csv_field(s: &str, delim: &str) -> String {
    if s.contains(delim) || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// The `INSERT INTO … VALUES …;` block for SQL export. `bool_kw` switches
/// booleans between `TRUE/FALSE` and quoted strings.
fn insert_statements(
    dialect: Dialect,
    table: &str,
    columns: &[String],
    rows: &[Row],
    bool_kw: bool,
) -> String {
    let quoted_table = dialect.quote_ident(table);
    let cols = columns.iter().map(|c| dialect.quote_ident(c)).collect::<Vec<_>>().join(", ");
    rows.iter()
        .map(|row| {
            let vals = columns
                .iter()
                .map(|c| {
                    let v = row.get(c).unwrap_or(&Value::Null);
                    if bool_kw {
                        values::literal_bool_kw(dialect, v)
                    } else {
                        values::literal(dialect, v)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("INSERT INTO {quoted_table} ({cols}) VALUES ({vals});")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Apply INSERT statements in bounded transactional chunks. Each chunk commits
/// atomically; a chunk that fails rolls back and stops the import, leaving
/// earlier chunks committed (chunked mode) and reporting the failing row.
const IMPORT_CHUNK: usize = 500;

async fn run_import(adapter: &SharedAdapter, statements: Vec<String>) -> ImportResult {
    let total = statements.len() as u64;
    let mut inserted = 0u64;
    let mut error = None;
    for chunk in statements.chunks(IMPORT_CHUNK) {
        match adapter.exec_batch(chunk).await {
            Ok(_) => inserted += chunk.len() as u64,
            Err(e) => {
                error = Some(format!("Row {}: {e}", inserted + 1));
                break;
            }
        }
    }
    ImportResult { inserted, total, error }
}

/// Chunked-transactional import of parameterized INSERTs (see `run_import`).
async fn run_import_params(
    adapter: &SharedAdapter,
    batch: Vec<(String, Vec<Value>)>,
) -> ImportResult {
    let total = batch.len() as u64;
    let mut inserted = 0u64;
    let mut error = None;
    for chunk in batch.chunks(IMPORT_CHUNK) {
        match adapter.exec_batch_params(chunk).await {
            Ok(_) => inserted += chunk.len() as u64,
            Err(e) => {
                error = Some(format!("Row {}: {e}", inserted + 1));
                break;
            }
        }
    }
    ImportResult { inserted, total, error }
}


/// `(name, columns)` pairs for the real tables (not views) of a connection.
async fn table_columns(
    adapter: &SharedAdapter,
    tables: &[TableInfo],
) -> Result<Vec<(String, Vec<ColumnInfo>)>, String> {
    let mut out = Vec::new();
    for table in tables.iter().filter(|t| t.kind == "table") {
        let columns = adapter.get_columns(&table.name).await?;
        out.push((table.name.clone(), columns));
    }
    Ok(out)
}

fn is_numeric_type(data_type: &str) -> bool {
    let data_type = data_type.to_lowercase();
    ["int", "float", "double", "decimal", "numeric", "real", "money", "serial"]
        .iter()
        .any(|kind| data_type.contains(kind))
}

/// null/missing becomes `None`.
fn cell_opt(row: &Row, key: &str) -> Option<String> {
    match row.get(key) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(v) => Some(stringify_cell(v)),
    }
}

async fn profile_column(
    adapter: &SharedAdapter,
    dialect: Dialect,
    quoted_table: &str,
    col: &ColumnInfo,
    total_rows: i64,
) -> ColumnProfile {
    let column = dialect.quote_ident(&col.name);
    let stats_sql = format!(
        "SELECT COUNT(*) - COUNT({column}) as null_count, \
         COUNT(DISTINCT {column}) as distinct_count, \
         MIN({column}::text) as min_val, MAX({column}::text) as max_val FROM {quoted_table}"
    );

    // The one query whose failure zeroes the whole column.
    let stats = match adapter.query(&stats_sql).await {
        Ok(raw) => raw,
        Err(_) => return zero_profile(col, total_rows),
    };
    let stats_row = stats.rows.first();
    let null_count = stats_row.map(|row| row_i64(row, "null_count")).unwrap_or(0);
    let distinct_count = stats_row.map(|row| row_i64(row, "distinct_count")).unwrap_or(0);
    let min_value = stats_row.and_then(|row| cell_opt(row, "min_val"));
    let max_value = stats_row.and_then(|row| cell_opt(row, "max_val"));

    let avg_value = if is_numeric_type(&col.data_type) {
        adapter
            .query(&format!("SELECT AVG({column}::numeric)::text as avg_val FROM {quoted_table}"))
            .await
            .ok()
            .and_then(|raw| raw.rows.into_iter().next())
            .and_then(|row| cell_opt(&row, "avg_val"))
            .and_then(|s| s.parse::<f64>().ok())
            .map(|n| format!("{n:.2}"))
    } else {
        None
    };

    let top_values = adapter
        .query(&format!(
            "SELECT {column}::text as val, COUNT(*) as cnt FROM {quoted_table} \
             WHERE {column} IS NOT NULL GROUP BY {column} ORDER BY cnt DESC LIMIT 5"
        ))
        .await
        .ok()
        .map(|raw| {
            raw.rows
                .iter()
                .map(|row| TopValue {
                    value: cell_opt(row, "val").unwrap_or_default(),
                    count: row_i64(row, "cnt"),
                })
                .collect()
        })
        .unwrap_or_default();

    let null_percent = if total_rows > 0 {
        ((null_count as f64 / total_rows as f64) * 10000.0).round() / 100.0
    } else {
        0.0
    };

    ColumnProfile {
        column: col.name.clone(),
        data_type: col.data_type.clone(),
        total_rows,
        null_count,
        null_percent,
        distinct_count,
        min_value,
        max_value,
        avg_value,
        top_values,
    }
}

fn zero_profile(col: &ColumnInfo, total_rows: i64) -> ColumnProfile {
    ColumnProfile {
        column: col.name.clone(),
        data_type: col.data_type.clone(),
        total_rows,
        null_count: 0,
        null_percent: 0.0,
        distinct_count: 0,
        min_value: None,
        max_value: None,
        avg_value: None,
        top_values: Vec::new(),
    }
}
