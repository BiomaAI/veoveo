//! Media-owned Task receipts, private dispatch bindings and provider associations.
use crate::contract::{MediaPredictionId, MediaTaskKind};
use chrono::{DateTime, Utc};
mod records;
use records::{GenerationResultRecord, RunRequestRecord};
use surrealdb::types::{RecordId, SurrealValue, Value};
use veoveo_modules::TableName;
use veoveo_platform_store::{TaskOwnerRecord, WebhookJobBinding};
use veoveo_task_runtime::{
    OwnedTaskTable, TaskAssociation, TaskContribution, TaskContributions, TaskCreation,
    TaskDispatch, TaskError, TaskRuntime, TaskSettlement, TaskSnapshot,
};
use veoveo_types::{ExtensionName, Sha256Digest, TaskTypeDefinition, TaskTypeName};

#[derive(SurrealValue)]
struct Identity {
    tenant: RecordId,
    owner_context: TaskOwnerRecord,
    request: RunRequestRecord,
}
fn identity(
    owner: &veoveo_task_runtime::TaskOwner,
    request: &serde_json::Value,
) -> Result<Identity, TaskError> {
    let request = RunRequestRecord::new(request.clone())?;
    Ok(Identity {
        tenant: veoveo_platform_store::deterministic_tenant_id(owner.tenant_key())?.record_id(),
        owner_context: TaskOwnerRecord::try_from(owner)?,
        request,
    })
}
#[derive(SurrealValue)]
struct Association {
    job: RecordId,
    #[surreal(wrap)]
    provider: ExtensionName,
    #[surreal(wrap)]
    prediction: MediaPredictionId,
}
#[derive(SurrealValue)]
struct Dispatch {
    #[surreal(wrap)]
    callback_digest: Sha256Digest,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
enum Terminal {
    #[vocabulary(rename = "succeeded")]
    Succeeded,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}
#[derive(SurrealValue)]
struct Settlement {
    status: Terminal,
    completed_at: DateTime<Utc>,
    expected_result: Option<GenerationResultRecord>,
    #[surreal(wrap)]
    prediction: Option<MediaPredictionId>,
}
struct Contributions {
    table: OwnedTaskTable,
    kinds: Vec<TaskTypeName>,
}
fn table() -> Result<OwnedTaskTable, TaskError> {
    let module =
        crate::schema::ownership().map_err(|error| TaskError::InvalidRecord(error.to_string()))?;
    OwnedTaskTable::new(
        &module,
        TableName::new("media_task")
            .map_err(|error| TaskError::InvalidRecord(error.to_string()))?,
    )
}
/// Bind the declared owner adapter before accepting Media Tasks or callbacks.
pub fn bind(runtime: TaskRuntime) -> Result<TaskRuntime, TaskError> {
    let kinds = vec![MediaTaskKind::Run.name()];
    runtime
        .requiring_contributions(kinds.clone())?
        .bind_contributions(std::sync::Arc::new(Contributions {
            table: table()?,
            kinds,
        }))
}
/// Prepare one private binding receipt from the existing capability context.
pub fn dispatch(
    current: &TaskSnapshot,
    callback_digest: Sha256Digest,
) -> Result<TaskDispatch, TaskError> {
    TaskDispatch::prepare(
        table()?,
        identity(&current.owner, &current.request)?,
        Dispatch { callback_digest },
    )
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
    fn dispatch_prepared(
        &self,
        current: &TaskSnapshot,
        value: &TaskDispatch,
    ) -> Result<TaskDispatch, TaskError> {
        let Value::Object(object) = value.metadata() else {
            return Err(TaskError::InvalidRecord(
                "invalid Media dispatch metadata".into(),
            ));
        };
        if object.len() != 1 || !object.contains_key("callback_digest") {
            return Err(TaskError::InvalidRecord(
                "invalid Media dispatch metadata fields".into(),
            ));
        }
        let dispatch = Dispatch::from_value(value.metadata().clone())
            .map_err(|error| TaskError::InvalidRecord(error.to_string()))?;
        TaskDispatch::prepare(
            self.table.clone(),
            identity(&current.owner, &current.request)?,
            dispatch,
        )
    }
    fn provider_associated(
        &self,
        current: &TaskSnapshot,
        binding: &WebhookJobBinding,
    ) -> Result<TaskAssociation, TaskError> {
        if binding.task_id != current.task_id
            || binding.task_type != current.task_type
            || binding.server.as_str() != current.server
            || binding.tenant
                != veoveo_platform_store::deterministic_tenant_id(current.owner.tenant_key())?
            || binding.provider.as_str() != "media"
        {
            return Err(TaskError::InvalidRecord(
                "Media provider association disagrees with Task identity".into(),
            ));
        }
        let association = Association {
            job: binding.job_id.record_id(),
            provider: binding.provider.clone(),
            prediction: MediaPredictionId::new(binding.external_job_id.to_string())
                .map_err(|error| TaskError::InvalidRecord(error.to_string()))?,
        };
        TaskAssociation::associate(
            self.table.clone(),
            identity(&current.owner, &current.request)?,
            association,
        )
    }
    fn settled(
        &self,
        current: &TaskSnapshot,
        settlement: TaskSettlement<'_>,
        at: DateTime<Utc>,
    ) -> Result<TaskContribution, TaskError> {
        let (status, expected_result, prediction) = match settlement {
            TaskSettlement::Succeeded { result } => {
                let expected_result = GenerationResultRecord::new(result.clone())?;
                let generation = expected_result.generation();
                let input = RunRequestRecord::new(current.request.clone())?;
                if generation.task_id() != current.task_id
                    || generation.prediction().model_id != input.request().model
                {
                    return Err(TaskError::InvalidRecord(
                        "Media completion disagrees with Task request".into(),
                    ));
                }
                let prediction = generation.prediction().id.clone();
                (Terminal::Succeeded, Some(expected_result), Some(prediction))
            }
            TaskSettlement::Failed { .. } => (Terminal::Failed, None, None),
            TaskSettlement::Cancelled => (Terminal::Cancelled, None, None),
        };
        TaskContribution::settle(
            self.table.clone(),
            identity(&current.owner, &current.request)?,
            Settlement {
                status,
                completed_at: at,
                expected_result,
                prediction,
            },
        )
    }
}

#[derive(SurrealValue)]
struct DispatchRow {
    dispatch: Option<Dispatch>,
}
/// Verify callback correlation from the owner receipt even after private context cleanup.
pub async fn callback_digest(
    runtime: &TaskRuntime,
    task: veoveo_types::TaskId,
) -> Result<Option<Sha256Digest>, TaskError> {
    let current = runtime
        .get(task)
        .await?
        .ok_or_else(|| TaskError::NotFound(task.to_string()))?;
    let expected = identity(&current.owner, &current.request)?;
    let mut response = runtime
        .platform_store()
        .client()
        .query(include_str!("queries/task_lookup/dispatch.surql"))
        .bind(("task", veoveo_platform_store::task_record_id(task)))
        .bind(("tenant", expected.tenant))
        .bind(("owner_context", expected.owner_context))
        .bind(("request", expected.request))
        .bind((
            "server",
            RecordId::new("mcp_server", runtime.server().to_owned()),
        ))
        .bind(("task_types", vec![MediaTaskKind::Run.name().to_string()]))
        .await?
        .check()?;
    let mut rows: Vec<DispatchRow> = response.take(0)?;
    if rows.len() > 1 {
        return Err(TaskError::InvalidRecord(
            "multiple Media Task dispatch receipts".into(),
        ));
    }
    Ok(rows
        .pop()
        .and_then(|row| row.dispatch)
        .map(|dispatch| dispatch.callback_digest))
}
