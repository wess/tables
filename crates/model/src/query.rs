//! Query and row-browsing shapes: the raw adapter result, the UI-facing query
//! result, and the paged rows request/response with filters and sort.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A row as the drivers return it: column name → JSON value, in column order
/// (serde_json's `preserve_order` feature keeps insertion order).
pub type Row = Map<String, Value>;

/// What an adapter returns for one statement. `column_types` is always empty
/// — typed column metadata was never carried over the wire.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RawResult {
    pub columns: Vec<String>,
    pub column_types: Map<String, Value>,
    pub rows: Vec<Row>,
    pub rows_affected: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub column_types: Map<String, Value>,
    pub rows: Vec<Row>,
    pub rows_affected: u64,
    pub execution_time: u64, // wall-clock ms, rounded
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Only present in multi-statement results — the trimmed statement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sql: Option<String>,
}

impl QueryResult {
    pub fn from_raw(raw: RawResult, execution_time: u64) -> Self {
        QueryResult {
            columns: raw.columns,
            column_types: raw.column_types,
            rows: raw.rows,
            rows_affected: raw.rows_affected,
            execution_time,
            error: None,
            sql: None,
        }
    }

    pub fn failure(error: String, execution_time: u64) -> Self {
        QueryResult {
            execution_time,
            error: Some(error),
            ..Default::default()
        }
    }
}

/// A single row write, applied as part of an atomic reviewed batch.
#[derive(Clone, Debug, PartialEq)]
pub enum RowWrite {
    Update {
        table: String,
        primary_key: Row,
        changes: Row,
    },
    Insert {
        table: String,
        row: Row,
    },
    Delete {
        table: String,
        primary_key: Row,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterCondition {
    pub id: String,
    pub column: String,
    pub operator: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value2: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortSpec {
    pub column: String,
    pub direction: String, // asc | desc
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RowsRequest {
    pub table: String,
    pub page: u64, // 1-based
    pub page_size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<SortSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<Vec<FilterCondition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_logic: Option<String>, // and | or
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RowsResponse {
    pub rows: Vec<Row>,
    pub columns: Vec<String>,
    pub column_types: Map<String, Value>,
    pub total: i64,
    pub page: u64,
    pub page_size: u64,
}
