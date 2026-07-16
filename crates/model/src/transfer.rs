//! Import/export result shapes.

use serde::{Deserialize, Serialize};

/// Row-loading result of a CSV import or mock-data fill: how many statements
/// ran before the first failure, and that error.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub inserted: u64,
    pub total: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// SQL-import result — the statement either applied or reported an error.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportSqlResult {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub rows_affected: u64,
}

/// File-export result — the written path (None when no path was given) and how
/// many rows landed in it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportFileResult {
    pub path: Option<String>,
    pub rows: u64,
}
