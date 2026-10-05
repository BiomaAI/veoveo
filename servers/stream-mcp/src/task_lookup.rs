//! Typed owner lookup values joined to kernel Task creation and terminal CAS.
use crate::contract::{PipelineId, StreamTaskKind};
use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_modules::TableName;
use veoveo_task_runtime::{
    OwnedTaskTable, TaskContribution, TaskContributions, TaskCreation, TaskError, TaskRuntime,
    TaskSettlement, TaskSnapshot,
};
use veoveo_types::{TaskTypeDefinition, TaskTypeName};

#[derive(Clone, SurrealValue)]
struct Identity {
    #[surreal(wrap)]
    pipeline_id: PipelineId,
    tenant: RecordId,
}
fn identity(
    owner: &veoveo_task_runtime::TaskOwner,
    value: &serde_json::Value,
) -> Result<Identity, TaskError> {
    let request: crate::task_request::DurableStreamRequest = serde_json::from_value(value.clone())?;
    let crate::task_request::StreamTaskInput::RunRecording(input) = request.input;
    Ok(Identity {
        pipeline_id: input.pipeline_id,
        tenant: veoveo_platform_store::deterministic_tenant_id(owner.tenant_key())?.record_id(),
    })
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Terminal {
    #[vocabulary(rename = "succeeded")]
    Succeeded,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Outcome {
    #[vocabulary(rename = "product")]
    Product,
    #[vocabulary(rename = "tool_error")]
    ToolError,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}
#[derive(SurrealValue)]
struct Settlement {
    #[surreal(wrap)]
    outcome: Outcome,
    #[surreal(wrap)]
    status: Terminal,
    completed_at: DateTime<Utc>,
    results: Option<RecordId>,
    annotations: Option<RecordId>,
    source_clip: Option<RecordId>,
}
struct Contributions {
    table: OwnedTaskTable,
    kinds: Vec<TaskTypeName>,
}
pub fn bind(runtime: TaskRuntime) -> Result<TaskRuntime, TaskError> {
    let table = OwnedTaskTable::new(
        &crate::schema::ownership().map_err(|e| TaskError::InvalidRecord(e.to_string()))?,
        TableName::new("stream_run").map_err(|e| TaskError::InvalidRecord(e.to_string()))?,
    )?;
    let kinds = vec![StreamTaskKind::RunRecording.name()];
    runtime
        .requiring_contributions(kinds.clone())?
        .bind_contributions(std::sync::Arc::new(Contributions { table, kinds }))
}
impl TaskContributions for Contributions {
    fn table(&self) -> &OwnedTaskTable {
        &self.table
    }
    fn task_types(&self) -> &[TaskTypeName] {
        &self.kinds
    }
    fn created(&self, creation: TaskCreation<'_>) -> Result<TaskContribution, TaskError> {
        TaskContribution::create(
            self.table.clone(),
            identity(&creation.draft.owner, &creation.draft.request)?,
        )
    }
    fn settled(
        &self,
        current: &TaskSnapshot,
        settlement: TaskSettlement<'_>,
        at: DateTime<Utc>,
    ) -> Result<TaskContribution, TaskError> {
        let identity = identity(&current.owner, &current.request)?;
        let mut values = Settlement {
            outcome: Outcome::Product,
            status: Terminal::Succeeded,
            completed_at: at,
            results: None,
            annotations: None,
            source_clip: None,
        };
        let result = match settlement {
            TaskSettlement::Succeeded { result } => result,
            TaskSettlement::Failed { .. } => {
                values.status = Terminal::Failed;
                values.outcome = Outcome::Failed;
                return TaskContribution::settle(self.table.clone(), identity, values);
            }
            TaskSettlement::Cancelled => {
                values.status = Terminal::Cancelled;
                values.outcome = Outcome::Cancelled;
                return TaskContribution::settle(self.table.clone(), identity, values);
            }
        };
        let envelope: rmcp::model::CallToolResult = serde_json::from_value(result.clone())?;
        let Some(output) = crate::task_product::validate(&envelope)
            .map_err(|error| TaskError::InvalidRecord(error.to_string()))?
        else {
            values.outcome = Outcome::ToolError;
            return TaskContribution::settle(self.table.clone(), identity, values);
        };
        if output.run_id().task_id() != current.task_id
            || *output.pipeline_uri.id() != identity.pipeline_id
        {
            return Err(TaskError::InvalidRecord(
                "owner settlement differs from its Task or pipeline".into(),
            ));
        }
        let link = |id: veoveo_artifact_contract::ArtifactId| {
            RecordId::new(
                "artifact_occurrence",
                surrealdb::types::Uuid::from(id.as_uuid()),
            )
        };
        values.results = Some(link(output.results_artifact.artifact_id()));
        values.annotations = Some(link(output.annotations_artifact.artifact_id()));
        values.source_clip = output
            .source_clip_artifact
            .as_ref()
            .map(|a| link(a.artifact_id()));

        TaskContribution::settle(self.table.clone(), identity, values)
    }
}
