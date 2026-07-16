//! Mock row generation. Word lists and value rules are verbatim; rows that end
//! up empty are still emitted (filtering happens at insert).

use std::sync::OnceLock;

use rand::Rng;
use regex::Regex;
use serde_json::{Map, Value};

use model::{new_uuid, ColumnInfo};

const FIRST_NAMES: [&str; 20] = [
    "Alice", "Bob", "Charlie", "Diana", "Eve", "Frank", "Grace", "Henry", "Iris", "Jack", "Kate",
    "Leo", "Mia", "Noah", "Olivia", "Paul", "Quinn", "Ruby", "Sam", "Tara",
];
const LAST_NAMES: [&str; 20] = [
    "Smith", "Johnson", "Williams", "Brown", "Jones", "Garcia", "Miller", "Davis", "Wilson",
    "Moore", "Taylor", "Anderson", "Thomas", "Jackson", "White", "Harris", "Martin", "Thompson",
    "Clark", "Lewis",
];
const DOMAINS: [&str; 5] = ["example.com", "test.org", "demo.io", "sample.net", "mock.dev"];
const CITIES: [&str; 10] = [
    "New York",
    "London",
    "Tokyo",
    "Paris",
    "Berlin",
    "Sydney",
    "Toronto",
    "Mumbai",
    "Seoul",
    "Amsterdam",
];
const WORDS: [&str; 17] = [
    "lorem",
    "ipsum",
    "dolor",
    "sit",
    "amet",
    "consectetur",
    "adipiscing",
    "elit",
    "sed",
    "do",
    "eiusmod",
    "tempor",
    "incididunt",
    "labore",
    "dolore",
    "magna",
    "aliqua",
];
const STATUSES: [&str; 4] = ["active", "inactive", "pending", "archived"];
const COUNTRIES: [&str; 10] = ["US", "UK", "JP", "FR", "DE", "AU", "CA", "IN", "KR", "NL"];

fn random_int(rng: &mut impl Rng, min: i64, max: i64) -> i64 {
    rng.random_range(min..=max)
}

/// Random float rounded to 2 decimals.
fn random_float(rng: &mut impl Rng, min: f64, max: f64) -> f64 {
    let v = rng.random::<f64>() * (max - min) + min;
    (v * 100.0).round() / 100.0
}

fn pick<'a>(rng: &mut impl Rng, arr: &[&'a str]) -> &'a str {
    arr[rng.random_range(0..arr.len())]
}

/// `YYYY-MM-DD`, year 2020–2026, day 01–28 so every month is valid.
fn random_date(rng: &mut impl Rng) -> String {
    format!(
        "{}-{:02}-{:02}",
        random_int(rng, 2020, 2026),
        random_int(rng, 1, 12),
        random_int(rng, 1, 28)
    )
}

fn random_timestamp(rng: &mut impl Rng) -> String {
    format!(
        "{} {:02}:{:02}:{:02}",
        random_date(rng),
        random_int(rng, 0, 23),
        random_int(rng, 0, 59),
        random_int(rng, 0, 59)
    )
}

fn random_sentence(rng: &mut impl Rng, word_count: usize) -> String {
    (0..word_count)
        .map(|_| pick(rng, &WORDS))
        .collect::<Vec<_>>()
        .join(" ")
}

/// None means the key is omitted from the row.
fn generate_value(col: &ColumnInfo, row_index: usize, rng: &mut impl Rng) -> Option<Value> {
    let t = col.data_type.to_lowercase();
    let name = col.name.to_lowercase();

    // Auto-increment / serial — skip
    if col.is_primary_key
        && (t.contains("serial") || t.contains("identity") || t.contains("autoincrement"))
    {
        return None;
    }
    if col.is_primary_key && t.contains("int") {
        return Some(Value::from(row_index as i64 + 1));
    }

    // Nullable — 10% chance of null
    if col.nullable && rng.random::<f64>() < 0.1 {
        return Some(Value::Null);
    }

    // Name-based heuristics
    if name.contains("email") {
        return Some(Value::from(format!(
            "{}.{}@{}",
            pick(rng, &FIRST_NAMES).to_lowercase(),
            pick(rng, &LAST_NAMES).to_lowercase(),
            pick(rng, &DOMAINS)
        )));
    }
    if name.contains("first_name") || name == "firstname" {
        return Some(Value::from(pick(rng, &FIRST_NAMES)));
    }
    if name.contains("last_name") || name == "lastname" {
        return Some(Value::from(pick(rng, &LAST_NAMES)));
    }
    if name.contains("name") && !name.contains("table") {
        return Some(Value::from(format!(
            "{} {}",
            pick(rng, &FIRST_NAMES),
            pick(rng, &LAST_NAMES)
        )));
    }
    if name.contains("phone") {
        return Some(Value::from(format!(
            "+1{}{}{}",
            random_int(rng, 200, 999),
            random_int(rng, 100, 999),
            random_int(rng, 1000, 9999)
        )));
    }
    if name.contains("city") {
        return Some(Value::from(pick(rng, &CITIES)));
    }
    if name.contains("url") || name.contains("website") {
        return Some(Value::from(format!(
            "https://{}/{}",
            pick(rng, &DOMAINS),
            pick(rng, &WORDS)
        )));
    }
    if name.contains("description") || name.contains("bio") || name.contains("notes") {
        let n = random_int(rng, 8, 20) as usize;
        return Some(Value::from(random_sentence(rng, n)));
    }
    if name.contains("title") || name.contains("subject") {
        let n = random_int(rng, 3, 6) as usize;
        return Some(Value::from(random_sentence(rng, n)));
    }
    if name.contains("status") {
        return Some(Value::from(pick(rng, &STATUSES)));
    }
    if name.contains("country") {
        return Some(Value::from(pick(rng, &COUNTRIES)));
    }
    if name.contains("uuid") || name.contains("guid") {
        return Some(Value::from(new_uuid()));
    }

    // Type-based
    if t.contains("bool") {
        return Some(Value::from(rng.random::<f64>() > 0.5));
    }
    if t.contains("uuid") {
        return Some(Value::from(new_uuid()));
    }
    if t.contains("timestamp") {
        return Some(Value::from(random_timestamp(rng)));
    }
    if t.contains("date") {
        return Some(Value::from(random_date(rng)));
    }
    if t.contains("time") {
        return Some(Value::from(format!(
            "{:02}:{:02}:00",
            random_int(rng, 0, 23),
            random_int(rng, 0, 59)
        )));
    }
    if t.contains("int") || t.contains("serial") {
        return Some(Value::from(random_int(rng, 1, 10000)));
    }
    if t.contains("float")
        || t.contains("double")
        || t.contains("decimal")
        || t.contains("numeric")
        || t.contains("real")
        || t.contains("money")
    {
        return Some(Value::from(random_float(rng, 0.0, 10000.0)));
    }
    if t.contains("json") {
        return Some(Value::from(format!(
            "{{\"key\":\"{}\",\"value\":{}}}",
            pick(rng, &WORDS),
            random_int(rng, 1, 100)
        )));
    }
    if t.contains("text") || t.contains("varchar") || t.contains("char") || t.contains("string") {
        static LEN: OnceLock<Regex> = OnceLock::new();
        let len_re = LEN.get_or_init(|| Regex::new(r"\(([0-9]+)\)").unwrap());
        let max_len = len_re.captures(&t).and_then(|c| c[1].parse::<usize>().ok());
        let len = max_len.map(|n| n.min(50)).unwrap_or(50);
        let words = len.div_ceil(6).min(10);
        let sentence = random_sentence(rng, words);
        return Some(Value::from(sentence.chars().take(len).collect::<String>()));
    }

    Some(Value::from(pick(rng, &WORDS)))
}

pub fn generate_mock_rows(columns: &[ColumnInfo], count: usize) -> Vec<Map<String, Value>> {
    let mut rng = rand::rng();
    (0..count)
        .map(|i| {
            let mut row = Map::new();
            for col in columns {
                if let Some(value) = generate_value(col, i, &mut rng) {
                    row.insert(col.name.clone(), value);
                }
            }
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: "text".into(),
            nullable: false,
            default_value: None,
            is_primary_key: false,
            comment: None,
        }
    }

    #[test]
    fn generates_the_requested_number_of_rows() {
        let rows = generate_mock_rows(&[col("name")], 25);
        assert_eq!(rows.len(), 25);
    }

    #[test]
    fn omits_auto_increment_serial_primary_keys() {
        let mut id = col("id");
        id.data_type = "serial".into();
        id.is_primary_key = true;
        let rows = generate_mock_rows(&[id, col("name")], 3);
        assert!(rows.iter().all(|r| !r.contains_key("id")));
        assert!(rows.iter().all(|r| r.contains_key("name")));
    }

    #[test]
    fn assigns_sequential_integer_primary_keys() {
        let mut id = col("id");
        id.data_type = "integer".into();
        id.is_primary_key = true;
        let rows = generate_mock_rows(&[id], 3);
        let ids: Vec<i64> = rows.iter().map(|r| r["id"].as_i64().unwrap()).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn produces_email_shaped_values_for_email_columns() {
        let rows = generate_mock_rows(&[col("email")], 5);
        assert!(rows.iter().all(|r| r["email"].as_str().unwrap().contains('@')));
    }

    #[test]
    fn respects_boolean_column_types() {
        let mut active = col("active");
        active.data_type = "boolean".into();
        let rows = generate_mock_rows(&[active], 10);
        assert!(rows.iter().all(|r| r["active"].is_boolean()));
    }

    #[test]
    fn produces_integers_within_range_for_int_columns() {
        let mut qty = col("qty");
        qty.data_type = "integer".into();
        let rows = generate_mock_rows(&[qty], 20);
        assert!(rows.iter().all(|r| r["qty"].as_i64().is_some()));
    }
}
