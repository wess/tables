//! JSON value → SQL literal rules shared by row ops, exports, and mock data.
//! String literals are escaped through the engine's `Dialect`.

use serde_json::Value;

use db::Dialect;

/// `String(v)` for a JSON number: integral floats print bare.
fn number_string(n: &serde_json::Number) -> String {
    if let Some(i) = n.as_i64() {
        i.to_string()
    } else if let Some(u) = n.as_u64() {
        u.to_string()
    } else {
        n.as_f64().unwrap_or(0.0).to_string()
    }
}

fn stringify(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// null → NULL, number → bare, everything else — booleans included — becomes an
/// engine-escaped quoted string ('true'/'false').
pub fn literal(dialect: Dialect, v: &Value) -> String {
    match v {
        Value::Null => "NULL".into(),
        Value::Number(n) => number_string(n),
        other => dialect.quote_string(&stringify(other)),
    }
}

/// Like [`literal`] but booleans become the TRUE/FALSE keywords — the
/// file-export and mock-data variant.
pub fn literal_bool_kw(dialect: Dialect, v: &Value) -> String {
    match v {
        Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        other => literal(dialect, other),
    }
}

/// `"col" = <lit>`, or `"col" IS NULL` for null — the WHERE fragments used by
/// row update / delete.
pub fn where_eq(dialect: Dialect, col: &str, v: &Value) -> String {
    match v {
        Value::Null => format!("{} IS NULL", dialect.quote_ident(col)),
        other => format!("{} = {}", dialect.quote_ident(col), literal(dialect, other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PG: Dialect = Dialect::Postgres;
    const MY: Dialect = Dialect::Mysql;

    #[test]
    fn null_is_bare_keyword() {
        assert_eq!(literal(PG, &Value::Null), "NULL");
        assert_eq!(literal_bool_kw(PG, &Value::Null), "NULL");
    }

    #[test]
    fn numbers_stay_bare() {
        assert_eq!(literal(PG, &json!(42)), "42");
        assert_eq!(literal(PG, &json!(-7)), "-7");
        assert_eq!(literal(PG, &json!(2.5)), "2.5");
        assert_eq!(literal(PG, &json!(3.0)), "3");
    }

    #[test]
    fn booleans_fall_through_to_string_branch() {
        assert_eq!(literal(PG, &json!(true)), "'true'");
        assert_eq!(literal(PG, &json!(false)), "'false'");
    }

    #[test]
    fn bool_kw_variant_uses_keywords() {
        assert_eq!(literal_bool_kw(PG, &json!(true)), "TRUE");
        assert_eq!(literal_bool_kw(PG, &json!(false)), "FALSE");
        assert_eq!(literal_bool_kw(PG, &json!("x")), "'x'");
        assert_eq!(literal_bool_kw(PG, &json!(2)), "2");
    }

    #[test]
    fn strings_are_quoted_and_escaped() {
        assert_eq!(literal(PG, &json!("hello")), "'hello'");
        assert_eq!(literal(PG, &json!("O'Brien")), "'O''Brien'");
        // MySQL also escapes backslashes.
        assert_eq!(literal(MY, &json!("a\\b")), "'a\\\\b'");
    }

    #[test]
    fn objects_and_arrays_serialize_then_quote() {
        assert_eq!(literal(PG, &json!({"a": 1})), "'{\"a\":1}'");
        assert_eq!(literal(PG, &json!([1, 2])), "'[1,2]'");
    }

    #[test]
    fn where_eq_handles_null_and_values() {
        assert_eq!(where_eq(PG, "id", &Value::Null), "\"id\" IS NULL");
        assert_eq!(where_eq(PG, "id", &json!(3)), "\"id\" = 3");
        assert_eq!(where_eq(PG, "name", &json!("Bob")), "\"name\" = 'Bob'");
        assert_eq!(where_eq(MY, "ok", &json!(true)), "`ok` = 'true'");
    }
}
