//! Shared DuckDB contract types.
//!
//! These model DuckDB-readable data sources and the hosted DuckDB server's
//! typed request/output envelopes. The SQL text itself is a genuinely
//! open-ended boundary and stays a raw string; everything around it is typed.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ArtifactMetadata;

/// Error building DuckDB SQL fragments from typed inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuckDbSqlBuildError(pub String);

impl fmt::Display for DuckDbSqlBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DuckDbSqlBuildError {}

pub fn duckdb_quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub fn duckdb_quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// Render a `read_*` table function call for one typed source expression
/// (a quoted path/URI literal or a `[...]` list of them).
pub fn duckdb_read_function_sql(
    source_expr: &str,
    format: &DuckDbFormat,
    options: &DuckDbReadOptions,
) -> Result<String, DuckDbSqlBuildError> {
    let options = duckdb_read_options_sql(options)?;
    Ok(match format {
        DuckDbFormat::Auto => format!("read_csv_auto({source_expr}{options})"),
        DuckDbFormat::Csv => format!("read_csv({source_expr}{options})"),
        DuckDbFormat::Parquet => format!("read_parquet({source_expr}{options})"),
        DuckDbFormat::Json => format!("read_json({source_expr}{options})"),
        DuckDbFormat::Ndjson => format!("read_ndjson({source_expr}{options})"),
    })
}

pub fn duckdb_read_options_sql(options: &DuckDbReadOptions) -> Result<String, DuckDbSqlBuildError> {
    let mut fields = Vec::new();
    if let Some(header) = options.header {
        fields.push(format!(
            "header = {}",
            if header { "true" } else { "false" }
        ));
    }
    if let Some(delimiter) = &options.delimiter {
        fields.push(format!("delim = {}", duckdb_quote_literal(delimiter)));
    }
    if let Some(timestamp_format) = &options.timestamp_format {
        fields.push(format!(
            "timestampformat = {}",
            duckdb_quote_literal(timestamp_format)
        ));
    }
    let mut seen = BTreeSet::from([
        "header".to_string(),
        "delim".to_string(),
        "timestampformat".to_string(),
    ]);
    for (key, value) in &options.extra {
        validate_option_key(key)?;
        if !seen.insert(key.clone()) {
            return Err(DuckDbSqlBuildError(format!(
                "duplicate DuckDB read option `{key}`"
            )));
        }
        fields.push(format!("{key} = {}", option_value_sql(value)?));
    }
    if fields.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!(", {}", fields.join(", ")))
    }
}

fn validate_option_key(key: &str) -> Result<(), DuckDbSqlBuildError> {
    if key.is_empty()
        || !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(DuckDbSqlBuildError(format!(
            "invalid DuckDB read option key `{key}`"
        )));
    }
    Ok(())
}

fn option_value_sql(value: &Value) -> Result<String, DuckDbSqlBuildError> {
    match value {
        Value::Bool(value) => Ok(if *value { "true" } else { "false" }.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::String(value) => Ok(duckdb_quote_literal(value)),
        Value::Array(values) => {
            let values = values
                .iter()
                .map(option_value_sql)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ");
            Ok(format!("[{values}]"))
        }
        Value::Null | Value::Object(_) => Err(DuckDbSqlBuildError(
            "DuckDB read option values must be bool, number, string, or arrays".to_string(),
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DuckDbFormat {
    Auto,
    Csv,
    Parquet,
    Json,
    Ndjson,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DuckDbReadOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp_format: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DuckDbSource {
    InlineCsv {
        csv: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(default)]
        options: DuckDbReadOptions,
    },
    Uri {
        uri: String,
        format: DuckDbFormat,
        #[serde(default)]
        options: DuckDbReadOptions,
    },
    Uris {
        uris: Vec<String>,
        format: DuckDbFormat,
        #[serde(default)]
        options: DuckDbReadOptions,
    },
    /// A neutral `artifact://{sha}` reference resolved through the shared
    /// artifact plane under the caller's identity — the cross-server input path.
    /// Any artifact produced by any hosted server (a media output, a timeseries
    /// RRD, an optimization DuckDB snapshot) can be read here, gated by the same
    /// grant + label checks as any other plane read. The server resolves and
    /// materializes the bytes; the SQL engine never touches the network.
    Artifact {
        uri: String,
        format: DuckDbFormat,
        #[serde(default)]
        options: DuckDbReadOptions,
    },
}

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

    #[test]
    fn source_wire_shape_is_unchanged_by_extraction() {
        let source: DuckDbSource = serde_json::from_str(
            r#"{"kind":"inline_csv","csv":"a,b\n1,2\n","options":{"header":true}}"#,
        )
        .unwrap();
        let DuckDbSource::InlineCsv { csv, options, .. } = source else {
            panic!("expected inline csv");
        };
        assert_eq!(csv, "a,b\n1,2\n");
        assert_eq!(options.header, Some(true));
    }

    #[test]
    fn artifact_source_wire_shape() {
        let sha = "a".repeat(64);
        let json = format!(r#"{{"kind":"artifact","uri":"artifact://{sha}","format":"parquet"}}"#);
        let source: DuckDbSource = serde_json::from_str(&json).unwrap();
        let DuckDbSource::Artifact { uri, format, .. } = &source else {
            panic!("expected artifact source");
        };
        assert_eq!(uri, &format!("artifact://{sha}"));
        assert_eq!(format, &DuckDbFormat::Parquet);
        // round-trips
        let back: DuckDbSource =
            serde_json::from_str(&serde_json::to_string(&source).unwrap()).unwrap();
        assert_eq!(back, source);
    }
}
