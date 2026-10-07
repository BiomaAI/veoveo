//! Known DuckDB usage facts converted to the Store's open metadata at the driver boundary.
use super::{DuckDbDatabaseId, DuckDbTableName, DuckDbTaskKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::ArtifactId;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    deny_unknown_fields,
    rename_all_fields = "camelCase"
)]
pub enum DuckDbQueryUsage {
    Inline {
        rows_returned: usize,
        truncated: bool,
    },
    Artifact {
        artifact: ArtifactId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "operation",
    rename_all = "snake_case",
    deny_unknown_fields,
    rename_all_fields = "camelCase"
)]
pub enum DuckDbUsageDetails {
    Query {
        result: DuckDbQueryUsage,
    },
    Execute {
        db: DuckDbDatabaseId,
        statements: u64,
    },
    Ingest {
        db: DuckDbDatabaseId,
        table: DuckDbTableName,
    },
    Export {
        db: DuckDbDatabaseId,
        artifact: ArtifactId,
    },
}

impl DuckDbUsageDetails {
    pub fn operation(&self) -> DuckDbTaskKind {
        match self {
            Self::Query { .. } => DuckDbTaskKind::Query,
            Self::Execute { .. } => DuckDbTaskKind::Execute,
            Self::Ingest { .. } => DuckDbTaskKind::Ingest,
            Self::Export { .. } => DuckDbTaskKind::Export,
        }
    }
}
