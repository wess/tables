//! Analysis-tool results: schema-comparison diffs and per-column profiles.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiff {
    pub table: String,
    #[serde(rename = "type")]
    pub kind: String, // added | removed | modified
    pub details: String,
    pub sql: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopValue {
    pub value: String,
    pub count: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnProfile {
    pub column: String,
    pub data_type: String,
    pub total_rows: i64,
    pub null_count: i64,
    pub null_percent: f64, // rounded to 2 decimals
    pub distinct_count: i64,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub avg_value: Option<String>, // "1234.56" or null
    pub top_values: Vec<TopValue>,
}
