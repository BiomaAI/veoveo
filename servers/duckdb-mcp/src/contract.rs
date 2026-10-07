mod artifact_origin;
mod catalog;
mod export;
mod query_output;
mod request_text;
mod requests;
pub use request_text::*;
pub use requests::*;
mod resources;
mod scopes;
mod task_kind;
pub use artifact_origin::*;
pub use catalog::*;
pub use export::*;
pub use query_output::*;
pub use resources::*;
pub use scopes::DuckDbScope;
use std::fmt;
pub use task_kind::DuckDbTaskKind;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::ArtifactMetadata;
mod read_options;
pub use read_options::*;
mod read_sql;
mod source;
mod source_addresses;
pub use source_addresses::*;
mod usage;
mod usage_metadata;
pub use usage::*;
pub use usage_metadata::*;

pub use read_sql::{
    duckdb_quote_identifier, duckdb_quote_literal, duckdb_read_function_sql,
    duckdb_read_options_sql,
};
pub use source::{DuckDbFormat, DuckDbSource, DuckDbTabularSource};

/// Owner-scoped name of a mutable hosted database file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
            Err(DuckDbDatabaseIdError)
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

impl std::str::FromStr for DuckDbDatabaseId {
    type Err = DuckDbDatabaseIdError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}
impl JsonSchema for DuckDbDatabaseId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "DuckDbDatabaseId".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type":"string",
            "description":"Owner-scoped name of a mutable hosted database file.",
            "pattern":"^[a-z][a-z0-9_]{0,63}$"
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "expected a DuckDB database ID of 1..=64 lowercase ASCII letters, digits or underscores, starting with a letter"
)]
pub struct DuckDbDatabaseIdError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "mode",
    rename_all = "snake_case",
    deny_unknown_fields,
    rename_all_fields = "camelCase"
)]
pub enum DuckDbQueryOutputMode {
    /// Rows inline in the tool result, subject to the server's row/byte caps.
    Inline {},
    /// Rows written to an immutable artifact; the result carries the link.
    Artifact { format: DuckDbTabularFormat },
}
impl Default for DuckDbQueryOutputMode {
    fn default() -> Self {
        Self::Inline {}
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct DuckDbColumn {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct DuckDbExecuteOutput {
    pub db: DuckDbDatabaseId,
    pub statements: u64,
    pub rows_changed: u64,
    pub db_created: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct DuckDbIngestOutput {
    pub db: DuckDbDatabaseId,
    pub table: DuckDbTableName,
    pub rows_ingested: u64,
    pub db_created: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(try_from = "DuckDbExportWire")]
pub struct DuckDbExportOutput(veoveo_types::Checked<DuckDbExportWire>);
impl Serialize for DuckDbExportOutput {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct DuckDbExportWire {
    result_uri: veoveo_artifact_contract::ArtifactUri,
    db: DuckDbDatabaseId,
    rows_exported: u64,
    artifact: ArtifactMetadata,
}
impl DuckDbExportOutput {
    pub fn new(db: DuckDbDatabaseId, rows_exported: u64, artifact: ArtifactMetadata) -> Self {
        Self::try_from(DuckDbExportWire {
            result_uri: artifact.artifact_uri.clone(),
            db,
            rows_exported,
            artifact,
        })
        .expect("export address derives from its admitted Artifact")
    }
    pub fn db(&self) -> &DuckDbDatabaseId {
        &self.0.db
    }
    pub fn rows_exported(&self) -> u64 {
        self.0.rows_exported
    }
    pub fn artifact(&self) -> &ArtifactMetadata {
        &self.0.artifact
    }
}
impl veoveo_types::Check for DuckDbExportWire {
    type Error = &'static str;
    fn check(&self) -> Result<(), Self::Error> {
        if self.result_uri != self.artifact.artifact_uri {
            return Err("export product address differs from its Artifact");
        }
        Ok(())
    }
}
impl TryFrom<DuckDbExportWire> for DuckDbExportOutput {
    type Error = &'static str;
    fn try_from(value: DuckDbExportWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
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
        assert_eq!(inline, DuckDbQueryOutputMode::Inline {});
        let artifact: DuckDbQueryOutputMode =
            serde_json::from_str(r#"{"mode":"artifact","format":"parquet"}"#).unwrap();
        assert_eq!(
            artifact,
            DuckDbQueryOutputMode::Artifact {
                format: DuckDbTabularFormat::Parquet
            }
        );
    }
}
