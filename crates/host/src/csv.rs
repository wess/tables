//! CSV parsing and INSERT generation.

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

use db::Dialect;

/// Per-line character parser: `"` toggles quote mode, `""` inside quotes is a
/// literal quote, `\r` outside quotes is skipped, blank data lines are
/// dropped. Quoted fields cannot span lines.
pub fn parse_csv(text: &str, delimiter: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let delim = if delimiter.chars().count() == 1 {
        delimiter.chars().next()
    } else {
        None
    };

    let parse_line = |line: &str| -> Vec<String> {
        let mut fields = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut chars = line.chars().peekable();
        while let Some(ch) = chars.next() {
            if in_quotes {
                if ch == '"' && chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else if ch == '"' {
                    in_quotes = false;
                } else {
                    current.push(ch);
                }
            } else if ch == '"' {
                in_quotes = true;
            } else if Some(ch) == delim {
                fields.push(std::mem::take(&mut current));
            } else if ch != '\r' {
                current.push(ch);
            }
        }
        fields.push(current);
        fields
    };

    let mut lines = text.split('\n');
    let headers = parse_line(lines.next().unwrap_or(""));
    let rows = lines
        .filter(|l| !l.trim().is_empty())
        .map(parse_line)
        .collect();
    (headers, rows)
}

/// One `INSERT INTO "table" (cols) VALUES (vals)` per row, no trailing
/// semicolon. Values: empty / "null" → NULL, numeric-looking → bare,
/// everything else quoted with `'` doubled.
pub fn csv_to_insert_sql(dialect: Dialect, table: &str, csv: &str, delimiter: &str) -> Vec<String> {
    static NUMERIC: OnceLock<Regex> = OnceLock::new();
    let numeric = NUMERIC.get_or_init(|| Regex::new(r"^-?[0-9]+(\.[0-9]+)?$").unwrap());

    let (headers, rows) = parse_csv(csv, delimiter);
    if headers.is_empty() {
        return Vec::new();
    }

    let width = headers.len();
    let cols = headers
        .iter()
        .map(|h| dialect.quote_ident(h.trim()))
        .collect::<Vec<_>>()
        .join(", ");
    rows.iter()
        .map(|row| {
            // Normalize to the header width: a short row pads with NULLs, a long
            // row is truncated, so the column/value counts always match.
            let vals = (0..width)
                .map(|i| {
                    let trimmed = row.get(i).map(|s| s.trim()).unwrap_or("");
                    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
                        "NULL".to_string()
                    } else if numeric.is_match(trimmed) {
                        trimmed.to_string()
                    } else {
                        dialect.quote_string(trimmed)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("INSERT INTO {} ({cols}) VALUES ({vals})", dialect.quote_ident(table))
        })
        .collect()
}

/// One parameterized `INSERT` per row: each non-null field becomes a bound
/// placeholder (never interpolated); empty / `null` maps to the SQL `NULL`
/// keyword. Values bind as text; on Postgres each is cast to the target column's
/// type (from `col_types`, keyed by header name) so a text bind still lands in a
/// numeric/temporal column. MySQL and SQLite coerce implicitly, so `col_types`
/// may be empty for them.
pub fn csv_to_insert_params(
    dialect: Dialect,
    table: &str,
    csv: &str,
    delimiter: &str,
    col_types: &HashMap<String, String>,
) -> Vec<(String, Vec<Value>)> {
    let (headers, rows) = parse_csv(csv, delimiter);
    if headers.is_empty() {
        return Vec::new();
    }
    let names: Vec<String> = headers.iter().map(|h| h.trim().to_string()).collect();
    let width = names.len();
    let cols = names.iter().map(|h| dialect.quote_ident(h)).collect::<Vec<_>>().join(", ");
    let quoted_table = dialect.quote_ident(table);

    rows.iter()
        .map(|row| {
            let mut params: Vec<Value> = Vec::new();
            // Normalize to the header width: a short row pads with NULLs, a long
            // row is truncated, so the column/placeholder counts always match.
            let vals: Vec<String> = (0..width)
                .map(|i| {
                    let trimmed = row.get(i).map(|s| s.trim()).unwrap_or("");
                    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
                        return "NULL".to_string();
                    }
                    params.push(Value::String(trimmed.to_string()));
                    let ph = dialect.placeholder(params.len());
                    let cast = names.get(i).and_then(|h| col_types.get(h));
                    match (dialect, cast) {
                        (Dialect::Postgres, Some(ty)) => format!("CAST({ph} AS {ty})"),
                        _ => ph,
                    }
                })
                .collect();
            (format!("INSERT INTO {quoted_table} ({cols}) VALUES ({})", vals.join(", ")), params)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PG: Dialect = Dialect::Postgres;

    #[test]
    fn parses_headers_and_rows() {
        let (headers, rows) = parse_csv("id,name\n1,Alice\n2,Bob", ",");
        assert_eq!(headers, vec!["id", "name"]);
        assert_eq!(rows, vec![vec!["1", "Alice"], vec!["2", "Bob"]]);
    }

    #[test]
    fn honors_a_custom_delimiter() {
        let (headers, rows) = parse_csv("id\tname\n1\tAlice", "\t");
        assert_eq!(headers, vec!["id", "name"]);
        assert_eq!(rows, vec![vec!["1", "Alice"]]);
    }

    #[test]
    fn handles_quoted_fields_containing_the_delimiter() {
        let (_, rows) = parse_csv("id,note\n1,\"a, b, c\"", ",");
        assert_eq!(rows, vec![vec!["1", "a, b, c"]]);
    }

    #[test]
    fn handles_escaped_double_quotes_inside_quoted_fields() {
        let (_, rows) = parse_csv("id,note\n1,\"she said \"\"hi\"\"\"", ",");
        assert_eq!(rows, vec![vec!["1", "she said \"hi\""]]);
    }

    #[test]
    fn skips_blank_trailing_lines() {
        let (_, rows) = parse_csv("id\n1\n\n2\n", ",");
        assert_eq!(rows, vec![vec!["1"], vec!["2"]]);
    }

    #[test]
    fn produces_one_insert_per_row_with_quoted_identifiers() {
        let sql = csv_to_insert_sql(PG, "users", "id,name\n1,Alice", ",");
        assert_eq!(
            sql,
            vec![r#"INSERT INTO "users" ("id", "name") VALUES (1, 'Alice')"#]
        );
    }

    #[test]
    fn treats_numeric_looking_values_as_numbers_and_others_as_strings() {
        let sql = csv_to_insert_sql(PG, "t", "a,b,c\n42,3.14,hello", ",");
        assert_eq!(
            sql[0],
            r#"INSERT INTO "t" ("a", "b", "c") VALUES (42, 3.14, 'hello')"#
        );
    }

    #[test]
    fn maps_empty_and_literal_null_to_sql_null() {
        let sql = csv_to_insert_sql(PG, "t", "a,b\n,NULL", ",");
        assert_eq!(sql[0], r#"INSERT INTO "t" ("a", "b") VALUES (NULL, NULL)"#);
    }

    #[test]
    fn escapes_single_quotes_to_prevent_broken_statements() {
        let sql = csv_to_insert_sql(PG, "t", "a\nO'Brien", ",");
        assert_eq!(sql[0], r#"INSERT INTO "t" ("a") VALUES ('O''Brien')"#);
    }

    #[test]
    fn returns_no_statements_for_empty_input() {
        assert_eq!(csv_to_insert_sql(PG, "t", "", ","), Vec::<String>::new());
    }

    #[test]
    fn pads_short_rows_and_truncates_long_rows_to_header_width() {
        // A short row (one value, two columns) pads with NULL; a long row (three
        // values, two columns) drops the extra so column/value counts match.
        let sql = csv_to_insert_sql(PG, "t", "a,b\n1\n2,3,4", ",");
        assert_eq!(sql[0], r#"INSERT INTO "t" ("a", "b") VALUES (1, NULL)"#);
        assert_eq!(sql[1], r#"INSERT INTO "t" ("a", "b") VALUES (2, 3)"#);
    }

    #[test]
    fn params_builder_keeps_columns_and_placeholders_balanced() {
        let stmts = csv_to_insert_params(PG, "t", "a,b\n1\n2,3,4", ",", &HashMap::new());
        // Short row: one bound value, second column NULL.
        assert_eq!(stmts[0].0, r#"INSERT INTO "t" ("a", "b") VALUES ($1, NULL)"#);
        assert_eq!(stmts[0].1, vec![Value::String("1".into())]);
        // Long row: truncated to two placeholders.
        assert_eq!(stmts[1].0, r#"INSERT INTO "t" ("a", "b") VALUES ($1, $2)"#);
        assert_eq!(stmts[1].1, vec![Value::String("2".into()), Value::String("3".into())]);
    }
}
