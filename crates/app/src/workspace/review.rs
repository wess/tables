//! SQL preview for the change-review modal. Display-only: the actual writes go
//! through the host's row ops on commit.

use serde_json::Value;

use crate::state::PendingChange;
use model::Row;

fn escape(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        other => {
            let text = match other {
                Value::String(s) => s.clone(),
                _ => other.to_string(),
            };
            format!("'{}'", text.replace('\'', "''"))
        }
    }
}

fn where_clause(primary_key: &Row) -> String {
    primary_key
        .iter()
        .map(|(k, v)| match v {
            Value::Null => format!("\"{k}\" IS NULL"),
            _ => format!("\"{k}\" = {}", escape(v)),
        })
        .collect::<Vec<_>>()
        .join(" AND ")
}

pub fn generate_sql(change: &PendingChange) -> String {
    match change {
        PendingChange::Update { table, primary_key, changes } => {
            let set = changes
                .iter()
                .map(|(k, v)| format!("\"{k}\" = {}", escape(v)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("UPDATE \"{table}\" SET {set} WHERE {};", where_clause(primary_key))
        }
        PendingChange::Delete { table, primary_key } => {
            format!("DELETE FROM \"{table}\" WHERE {};", where_clause(primary_key))
        }
        PendingChange::Insert { table, row } => {
            if row.is_empty() {
                return format!("INSERT INTO \"{table}\" DEFAULT VALUES;");
            }
            let cols = row.keys().map(|k| format!("\"{k}\"")).collect::<Vec<_>>().join(", ");
            let vals = row.values().map(escape).collect::<Vec<_>>().join(", ");
            format!("INSERT INTO \"{table}\" ({cols}) VALUES ({vals});")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(pairs: &[(&str, Value)]) -> Row {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn update_quotes_strings_and_numbers() {
        let change = PendingChange::Update {
            table: "users".into(),
            primary_key: row(&[("id", json!(7))]),
            changes: row(&[("name", json!("O'Brien"))]),
        };
        assert_eq!(
            generate_sql(&change),
            "UPDATE \"users\" SET \"name\" = 'O''Brien' WHERE \"id\" = 7;"
        );
    }

    #[test]
    fn delete_uses_is_null_for_null_key() {
        let change = PendingChange::Delete {
            table: "t".into(),
            primary_key: row(&[("id", Value::Null)]),
        };
        assert_eq!(generate_sql(&change), "DELETE FROM \"t\" WHERE \"id\" IS NULL;");
    }

    #[test]
    fn insert_renders_bools_as_keywords() {
        let change = PendingChange::Insert {
            table: "t".into(),
            row: row(&[("a", json!(1)), ("b", json!(true))]),
        };
        assert_eq!(generate_sql(&change), "INSERT INTO \"t\" (\"a\", \"b\") VALUES (1, TRUE);");
    }

    #[test]
    fn empty_insert_is_default_values() {
        let change = PendingChange::Insert { table: "t".into(), row: Row::new() };
        assert_eq!(generate_sql(&change), "INSERT INTO \"t\" DEFAULT VALUES;");
    }
}
