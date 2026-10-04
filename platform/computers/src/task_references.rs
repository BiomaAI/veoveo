//! Closed Task payloads shared by admission and the owning journal reader.
use crate::{
    ComputerError, Result,
    api::{ComputerId, ComputerTaskKind, ExecutionId, FileTransferId},
};
use serde::{Deserialize, Serialize};
use surrealdb::types::{SurrealValue, Value};
use veoveo_platform_store::{OpenObject, task_record_id};
use veoveo_task_runtime::ClaimedTask;
use veoveo_types::{TaskId, TaskTypeDefinition};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LifecycleReference {
    pub computer_id: ComputerId,
    pub operation_id: TaskId,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MaintenanceReference {
    pub computer_id: ComputerId,
    pub maintenance_id: TaskId,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CommandReference {
    pub computer_id: ComputerId,
    pub execution_id: ExecutionId,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FileReference {
    pub computer_id: ComputerId,
    pub transfer_id: FileTransferId,
}

/// Accepted Task ownership admits journal observation. Current policy and the
/// live Task lease are checked independently at effect dispatch and commit.
pub(crate) fn worker_bindings(
    claim: &ClaimedTask,
    kind: ComputerTaskKind,
    computer: ComputerId,
    task: TaskId,
) -> Result<Vec<(&'static str, Value)>> {
    if claim.snapshot.server != "computers" || claim.snapshot.task_type != kind.name() {
        return Err(ComputerError::InvalidInput);
    }
    if task.as_uuid().get_version_num() != 7 || claim.snapshot.task_id != task {
        return Err(ComputerError::StateConflict);
    }
    let actor: OpenObject = serde_json::from_value(
        serde_json::to_value(&claim.snapshot.owner).map_err(|_| ComputerError::Unavailable)?,
    )
    .map_err(|_| ComputerError::Unavailable)?;
    Ok(vec![
        ("task", task_record_id(task).into_value()),
        ("task_id", task.as_uuid().into_value()),
        ("computer_id", computer.as_uuid().into_value()),
        ("actor", actor.into_value()),
    ])
}
