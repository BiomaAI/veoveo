use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use veoveo_mcp_contract::{ArtifactMetadata, DuckDbSource};

/// Owner-scoped name of a mutable hosted database file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct DuckDbDatabaseId(String);

impl DuckDbDatabaseId {
    pub fn new(value: impl Into<String>) -> Result<Self, DuckDbDatabaseIdError> {
        let value = value.into();
        let valid_len = (1..=64).contains(&value.len());
        let valid_chars = value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
        let valid_start = value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase());
        if valid_len && valid_chars && valid_start {
            Ok(Self(value))
        } else {
            Err(DuckDbDatabaseIdError { value })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for DuckDbDatabaseId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DuckDbDatabaseId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for DuckDbDatabaseId {
    type Error = DuckDbDatabaseIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<DuckDbDatabaseId> for String {
    fn from(value: DuckDbDatabaseId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuckDbDatabaseIdError {
    pub value: String,
}

impl fmt::Display for DuckDbDatabaseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid duckdb database id `{}`: expected 1..=64 chars of [a-z0-9_] starting with a letter",
            self.value
        )
    }
}

impl std::error::Error for DuckDbDatabaseIdError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DuckDbExportFormat {
    Parquet,
    Csv,
    /// Full database snapshot as one immutable `.duckdb` file.
    DuckDb,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum DuckDbQueryOutputMode {
    /// Rows inline in the tool result, subject to the server's row/byte caps.
    #[default]
    Inline,
    /// Rows written to an immutable artifact; the result carries the link.
    Artifact { format: DuckDbExportFormat },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuckDbQueryRequest {
    pub db: DuckDbDatabaseId,
    /// Read-only SQL. Enforced by a read-only connection, not by parsing.
    pub sql: String,
    /// Additional readable databases attached read-only under their db ids.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attach: Vec<DuckDbDatabaseId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub output: DuckDbQueryOutputMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuckDbExecuteRequest {
    pub db: DuckDbDatabaseId,
    /// DDL/DML SQL executed on a writable connection.
    pub sql: String,
    #[serde(default)]
    pub create_if_missing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DuckDbIngestMode {
    /// Create the table; error if it already exists.
    Create,
    /// Append to an existing table.
    Append,
    /// Replace the table contents.
    Replace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuckDbIngestRequest {
    pub db: DuckDbDatabaseId,
    pub table: String,
    pub source: DuckDbSource,
    pub mode: DuckDbIngestMode,
    #[serde(default)]
    pub create_db_if_missing: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DuckDbExportSelection {
    Table {
        table: String,
    },
    /// Read-only SQL whose result set is exported.
    Sql {
        sql: String,
    },
    /// The whole database as a snapshot (format must be `duck_db`).
    Database,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuckDbExportRequest {
    pub db: DuckDbDatabaseId,
    pub selection: DuckDbExportSelection,
    pub format: DuckDbExportFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DuckDbColumn {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuckDbQueryOutput {
    pub columns: Vec<DuckDbColumn>,
    /// Row-major values; present only for inline output.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<Vec<Value>>,
    pub row_count: u64,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<ArtifactMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuckDbExecuteOutput {
    pub db: DuckDbDatabaseId,
    pub statements: u64,
    pub rows_changed: u64,
    pub db_created: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuckDbIngestOutput {
    pub db: DuckDbDatabaseId,
    pub table: String,
    pub rows_ingested: u64,
    pub db_created: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuckDbExportOutput {
    pub db: DuckDbDatabaseId,
    pub rows_exported: u64,
    pub artifact: ArtifactMetadata,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_id_accepts_snake_case() {
        assert!(DuckDbDatabaseId::new("robot_metrics_v2").is_ok());
    }

    #[test]
    fn database_id_rejects_bad_shapes() {
        for bad in ["", "2fast", "UPPER", "has-dash", "a".repeat(65).as_str()] {
            assert!(DuckDbDatabaseId::new(bad).is_err(), "accepted `{bad}`");
        }
    }

    #[test]
    fn query_output_mode_wire_shape() {
        let inline: DuckDbQueryOutputMode = serde_json::from_str(r#"{"mode":"inline"}"#).unwrap();
        assert_eq!(inline, DuckDbQueryOutputMode::Inline);
        let artifact: DuckDbQueryOutputMode =
            serde_json::from_str(r#"{"mode":"artifact","format":"parquet"}"#).unwrap();
        assert_eq!(
            artifact,
            DuckDbQueryOutputMode::Artifact {
                format: DuckDbExportFormat::Parquet
            }
        );
    }
}
