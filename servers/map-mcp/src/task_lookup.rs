//! Map owns travel-model Task input and retained-result integrity.
use crate::contract::{BuildTravelModelRequest, MapTaskKind, TravelModelId, TravelModelRecord};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{Object, RecordId, SurrealValue, Value};
use veoveo_mcp_contract::{GatewayInternalIdentity, IssuedArtifactWriteCapability};
use veoveo_task_runtime::{
    OwnedTaskTable, TaskContribution, TaskContributions, TaskCreation, TaskError, TaskRuntime,
    TaskSettlement, TaskSnapshot,
};
use veoveo_types::{PrincipalId, TaskTypeDefinition, TaskTypeName, WorkContextId};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DurableTravelModelRequest {
    pub input: BuildTravelModelRequest,
    pub identity: GatewayInternalIdentity,
    pub travel_model_id: TravelModelId,
    pub created_at: chrono::DateTime<Utc>,
    pub artifact_write_capability: IssuedArtifactWriteCapability,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    content = "request",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Input {
    BuildTravelModel(DurableTravelModelRequest),
}
#[derive(Clone, SurrealValue)]
pub(crate) struct Identity {
    #[surreal(wrap)]
    pub(crate) travel_model_id: TravelModelId,
    #[surreal(wrap)]
    pub(crate) created_by: PrincipalId,
    #[surreal(wrap)]
    pub(crate) work_context: WorkContextId,
    pub(crate) expected_input: Value,
    pub(crate) tenant: RecordId,
    pub(crate) profile: String,
    pub(crate) tenant_key: Option<String>,
    pub(crate) data_labels: Vec<String>,
    pub(crate) authority_tenant: String,
}
impl Identity {
    // Native optional fields omit NONE on storage. Both contribution phases
    // must supply the same object for the complete identity comparison.
    fn into_stored_value(self) -> Value {
        let Value::Object(mut fields) = self.into_value() else {
            unreachable!("Map identity is an object")
        };
        if matches!(fields.get("tenant_key"), Some(Value::None)) {
            fields.remove("tenant_key");
        }
        Value::Object(fields)
    }
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub(crate) enum Terminal {
    #[vocabulary(rename = "succeeded")]
    Succeeded,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub(crate) enum Outcome {
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
pub(crate) struct Settlement {
    #[surreal(wrap)]
    pub(crate) status: Terminal,
    #[surreal(wrap)]
    pub(crate) outcome: Outcome,
    pub(crate) completed_at: DateTime<Utc>,
    pub(crate) expected_result: Option<Value>,
}
#[derive(SurrealValue)]
pub(crate) struct Row {
    pub(crate) task: RecordId,
    pub(crate) identity: Identity,
    pub(crate) settlement: Settlement,
}
pub(crate) fn native_json(value: serde_json::Value) -> Value {
    let Value::Object(mut fields) =
        veoveo_platform_store::TaskResultRecord::new(value).into_value()
    else {
        unreachable!()
    };
    fields.remove("payload").expect("result payload")
}
pub(crate) fn json(value: Value) -> Result<serde_json::Value, TaskError> {
    let mut wrapper = Object::new();
    wrapper.insert("payload", value);
    Ok(veoveo_platform_store::TaskResultRecord::from_value(Value::Object(wrapper))?.into_payload())
}
fn identity(
    owner: &veoveo_task_runtime::TaskOwner,
    value: &serde_json::Value,
    task_id: veoveo_types::TaskId,
) -> Result<Identity, TaskError> {
    let Input::BuildTravelModel(request) = serde_json::from_value(value.clone())?;
    request
        .input
        .validate()
        .map_err(|error| TaskError::InvalidRecord(error.to_string()))?;
    let actor = &request.identity.actor;
    let kind = match actor.kind {
        veoveo_mcp_contract::PrincipalKind::User => veoveo_task_runtime::PrincipalKind::User,
        veoveo_mcp_contract::PrincipalKind::Service => veoveo_task_runtime::PrincipalKind::Service,
    };
    if veoveo_types::TaskId::parse(&request.artifact_write_capability.task_id)
        .map_err(|e| TaskError::InvalidRecord(e.to_string()))?
        != task_id
        || request.identity.server.as_str() != "map"
        || actor.id.as_str() != owner.principal_key
        || kind != owner.principal_kind
        || actor.issuer.as_str() != owner.issuer
        || actor.subject.as_str() != owner.subject
        || actor.tenant.as_ref().map(ToString::to_string) != owner.tenant_key
        || actor
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect::<std::collections::BTreeSet<_>>()
            != owner.data_labels
        || request.identity.profile.as_str() != owner.profile
        || request.identity.authority != owner.authority
    {
        return Err(TaskError::InvalidRecord(
            "Map Task input identity differs from its owner".into(),
        ));
    }
    Ok(Identity {
        travel_model_id: request.travel_model_id,
        created_by: actor.id.clone(),
        work_context: owner.authority.work_context.clone(),
        expected_input: native_json(value.clone()),
        tenant: veoveo_platform_store::deterministic_tenant_id(owner.tenant_key())
            .map_err(|e| TaskError::InvalidRecord(e.to_string()))?
            .record_id(),
        profile: owner.profile.clone(),
        tenant_key: owner.tenant_key.clone(),
        data_labels: owner.data_labels.iter().cloned().collect(),
        authority_tenant: owner.authority.tenant.to_string(),
    })
}
pub(crate) fn verify_projection(identity: &Identity) -> Result<(), TaskError> {
    let Input::BuildTravelModel(request) =
        serde_json::from_value(json(identity.expected_input.clone())?)?;
    request
        .input
        .validate()
        .map_err(|e| TaskError::InvalidRecord(e.to_string()))?;
    let actor = &request.identity.actor;
    let expected_tenant =
        veoveo_platform_store::deterministic_tenant_id(request.identity.authority.tenant.as_str())
            .map_err(|e| TaskError::InvalidRecord(e.to_string()))?
            .record_id();
    if request.identity.server.as_str() != "map"
        || request.travel_model_id != identity.travel_model_id
        || actor.id != identity.created_by
        || request.identity.profile.as_str() != identity.profile
        || actor.tenant.as_ref().map(ToString::to_string) != identity.tenant_key
        || actor
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect::<std::collections::BTreeSet<_>>()
            != identity
                .data_labels
                .iter()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
        || request.identity.authority.work_context != identity.work_context
        || request.identity.authority.tenant.as_str() != identity.authority_tenant
        || expected_tenant != identity.tenant
    {
        return Err(TaskError::InvalidRecord(
            "Map lookup differs from retained input identity".into(),
        ));
    }
    Ok(())
}
struct Contributions {
    table: OwnedTaskTable,
    kinds: Vec<TaskTypeName>,
}
pub fn bind(runtime: TaskRuntime) -> Result<TaskRuntime, TaskError> {
    let table = OwnedTaskTable::new(
        &crate::schema::ownership().map_err(|e| TaskError::InvalidRecord(e.to_string()))?,
        veoveo_modules::TableName::new("map_travel_model_task")
            .map_err(|e| TaskError::InvalidRecord(e.to_string()))?,
    )?;
    let kinds = vec![MapTaskKind::BuildTravelModel.name()];
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
            identity(
                &creation.draft.owner,
                &creation.draft.request,
                creation.draft.task_id,
            )?
            .into_stored_value(),
        )
    }
    fn settled(
        &self,
        current: &TaskSnapshot,
        settlement: TaskSettlement<'_>,
        at: DateTime<Utc>,
    ) -> Result<TaskContribution, TaskError> {
        let identity = identity(&current.owner, &current.request, current.task_id)?;
        let mut values = Settlement {
            status: Terminal::Succeeded,
            outcome: Outcome::Product,
            completed_at: at,
            expected_result: None,
        };
        match settlement {
            TaskSettlement::Failed { .. } => {
                values.status = Terminal::Failed;
                values.outcome = Outcome::Failed;
            }
            TaskSettlement::Cancelled => {
                values.status = Terminal::Cancelled;
                values.outcome = Outcome::Cancelled;
            }
            TaskSettlement::Succeeded { result } => {
                let envelope: rmcp::model::CallToolResult = serde_json::from_value(result.clone())?;
                if envelope.is_error != Some(true) {
                    let record: TravelModelRecord =
                        serde_json::from_value(envelope.structured_content.ok_or_else(|| {
                            TaskError::InvalidRecord(
                                "Map travel result has no structured content".into(),
                            )
                        })?)?;
                    record
                        .validate_identity()
                        .map_err(|e| TaskError::InvalidRecord(e.to_string()))?;
                    if record.travel_model_id != identity.travel_model_id
                        || record.created_by != identity.created_by
                        || record.work_context != identity.work_context
                    {
                        return Err(TaskError::InvalidRecord(
                            "Map travel result differs from its creation identity".into(),
                        ));
                    }
                    values.expected_result = Some(native_json(result.clone()));
                } else {
                    values.outcome = Outcome::ToolError;
                }
            }
        }
        TaskContribution::settle(self.table.clone(), identity.into_stored_value(), values)
    }
}
