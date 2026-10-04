//! Domain-owned operation metadata carried by Artifact-plane outputs.
use super::{DuckDbDatabaseId, DuckDbTableName, DuckDbTaskKind, usage::task_identity};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::TaskId;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DuckDbArtifactOperation {
    Query {
        row_count: u64,
    },
    ExportSql {
        row_count: u64,
    },
    ExportTable {
        table: DuckDbTableName,
        row_count: u64,
    },
    Snapshot {},
}

impl DuckDbArtifactOperation {
    pub fn task_kind(&self) -> DuckDbTaskKind {
        match self {
            Self::Query { .. } => DuckDbTaskKind::Query,
            Self::ExportSql { .. } | Self::ExportTable { .. } | Self::Snapshot {} => {
                DuckDbTaskKind::Export
            }
        }
    }
}

/// Operation facts checked before publication. The Artifact plane owns authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "OriginWire", into = "OriginWire")]
pub struct DuckDbArtifactOrigin {
    db: DuckDbDatabaseId,
    operation: DuckDbArtifactOperation,
    task_id: Option<TaskId>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OriginWire {
    db: DuckDbDatabaseId,
    operation: DuckDbArtifactOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    task_id: Option<TaskId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("expected a native Task UUIDv7 in DuckDB operation metadata")]
pub struct DuckDbArtifactOriginError;

impl DuckDbArtifactOrigin {
    /// A direct call carries no Task association.
    pub fn new(db: DuckDbDatabaseId, operation: DuckDbArtifactOperation) -> Self {
        Self {
            db,
            operation,
            task_id: None,
        }
    }

    /// ```compile_fail
    /// use veoveo_duckdb_mcp::contract::DuckDbArtifactOrigin;
    /// fn associate(origin: DuckDbArtifactOrigin) { origin.with_task("call-not-a-task"); }
    /// ```
    pub fn with_task(mut self, task_id: TaskId) -> Result<Self, DuckDbArtifactOriginError> {
        self.task_id = Some(task_identity(task_id).map_err(|_| DuckDbArtifactOriginError)?);
        Ok(self)
    }
    pub fn database(&self) -> &DuckDbDatabaseId {
        &self.db
    }
    pub fn operation(&self) -> &DuckDbArtifactOperation {
        &self.operation
    }
    pub fn task_id(&self) -> Option<TaskId> {
        self.task_id
    }
}

impl TryFrom<OriginWire> for DuckDbArtifactOrigin {
    type Error = DuckDbArtifactOriginError;
    fn try_from(wire: OriginWire) -> Result<Self, Self::Error> {
        let origin = Self::new(wire.db, wire.operation);
        match wire.task_id {
            Some(task_id) => origin.with_task(task_id),
            None => Ok(origin),
        }
    }
}
impl From<DuckDbArtifactOrigin> for OriginWire {
    fn from(value: DuckDbArtifactOrigin) -> Self {
        Self {
            db: value.db,
            operation: value.operation,
            task_id: value.task_id,
        }
    }
}
