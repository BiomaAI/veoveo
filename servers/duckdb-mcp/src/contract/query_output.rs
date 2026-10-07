//! Checked row shape, observed counts and exclusive inline/Artifact output.
use super::DuckDbColumn;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use veoveo_artifact_contract::ArtifactMetadata;

/// Construct results through `inline` or `exported`; callers cannot mutate
/// row/count or inline/Artifact relationships independently.
///
/// ```compile_fail
/// use veoveo_duckdb_mcp::contract::DuckDbQueryOutput;
/// fn change_count(mut output: DuckDbQueryOutput) { output.row_count = 7; }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "QueryOutputWire", into = "QueryOutputWire")]
#[schemars(transform = output_schema)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct DuckDbQueryOutput {
    columns: Vec<DuckDbColumn>,
    rows: Vec<Vec<Value>>,
    row_count: u64,
    truncated: bool,
    artifact: Option<ArtifactMetadata>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct QueryOutputWire {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_present_product_uri"
    )]
    #[schemars(with = "veoveo_artifact_contract::ArtifactUri")]
    result_uri: Option<veoveo_artifact_contract::ArtifactUri>,
    columns: Vec<DuckDbColumn>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rows: Vec<Vec<Value>>,
    row_count: u64,
    truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifact: Option<ArtifactMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "expected matching DuckDB row widths and observed count, or a complete Artifact output without inline data"
)]
pub struct DuckDbQueryOutputError;

impl DuckDbQueryOutput {
    /// `row_count` counts observed rows. With truncation it is a lower bound
    /// greater than the returned count; otherwise it equals the returned count.
    pub fn inline(
        columns: Vec<DuckDbColumn>,
        rows: Vec<Vec<Value>>,
        row_count: u64,
        truncated: bool,
    ) -> Result<Self, DuckDbQueryOutputError> {
        let returned = rows.len() as u64;
        if rows.iter().any(|row| row.len() != columns.len())
            || if truncated {
                row_count <= returned
            } else {
                row_count != returned
            }
        {
            return Err(DuckDbQueryOutputError);
        }
        Ok(Self {
            columns,
            rows,
            row_count,
            truncated,
            artifact: None,
        })
    }

    pub fn exported(artifact: ArtifactMetadata, row_count: u64) -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            row_count,
            truncated: false,
            artifact: Some(artifact),
        }
    }
    pub fn columns(&self) -> &[DuckDbColumn] {
        &self.columns
    }
    pub fn rows(&self) -> &[Vec<Value>] {
        &self.rows
    }
    pub fn row_count(&self) -> u64 {
        self.row_count
    }
    pub fn truncated(&self) -> bool {
        self.truncated
    }
    pub fn artifact(&self) -> Option<&ArtifactMetadata> {
        self.artifact.as_ref()
    }
}

impl TryFrom<QueryOutputWire> for DuckDbQueryOutput {
    type Error = DuckDbQueryOutputError;
    fn try_from(value: QueryOutputWire) -> Result<Self, Self::Error> {
        if value.result_uri
            != value
                .artifact
                .as_ref()
                .map(|artifact| artifact.artifact_uri.clone())
        {
            return Err(DuckDbQueryOutputError);
        }
        match value.artifact {
            Some(artifact) => {
                if !value.columns.is_empty() || !value.rows.is_empty() || value.truncated {
                    return Err(DuckDbQueryOutputError);
                }
                Ok(Self::exported(artifact, value.row_count))
            }
            None => Self::inline(value.columns, value.rows, value.row_count, value.truncated),
        }
    }
}
impl From<DuckDbQueryOutput> for QueryOutputWire {
    fn from(value: DuckDbQueryOutput) -> Self {
        Self {
            result_uri: value
                .artifact
                .as_ref()
                .map(|artifact| artifact.artifact_uri.clone()),
            columns: value.columns,
            rows: value.rows,
            row_count: value.row_count,
            truncated: value.truncated,
            artifact: value.artifact,
        }
    }
}

fn output_schema(schema: &mut schemars::Schema) {
    // JSON Schema expresses the two output forms. Row-width/count relationships
    // depend on instance values and are checked by the constructor and decoder.
    schema.insert(
        "oneOf".into(),
        serde_json::json!([
            {"properties":{"artifact":{"type":"null"}},"not":{"required":["resultUri"]}},
            {"required":["artifact","resultUri"],"properties":{
                "artifact":{"type":"object"},
                "columns":{"maxItems":0},"rows":{"maxItems":0},"truncated":{"const":false}
            }}
        ]),
    );
}

fn deserialize_present_product_uri<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<veoveo_artifact_contract::ArtifactUri>, D::Error> {
    <veoveo_artifact_contract::ArtifactUri as serde::Deserialize>::deserialize(deserializer)
        .map(Some)
}
