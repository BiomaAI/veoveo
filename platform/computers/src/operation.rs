//! Durable accepted intent. A Task lease never replaces this Computer fence.
use crate::{
    ComputerError, Result,
    api::{Action, ComputerPhase},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::OpenObject;
use veoveo_task_runtime::{TaskId, TaskOwner};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStage {
    Queued,
    Dispatched,
    RecoveryRequired,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct Operation {
    pub operation_id: Uuid,
    pub computer_id: Uuid,
    pub actor: TaskOwner,
    pub provider_instance_id: Uuid,
    pub template_fingerprint: String,
    pub action: Action,
    pub stage: OperationStage,
    pub previous_phase: ComputerPhase,
    pub previous_resource_id: Option<String>,
    pub previous_process_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub dispatch_id: Option<Uuid>,
    pub dispatched_at: Option<DateTime<Utc>>,
    pub observation_deadline: Option<DateTime<Utc>>,
    pub observation_reads: u32,
    pub next_observation_at: Option<DateTime<Utc>>,
    pub last_observation_id: Option<Uuid>,
    pub settled_at: Option<DateTime<Utc>>,
    pub result_resource_id: Option<String>,
    pub result_process_id: Option<String>,
    pub completion_code: Option<crate::UndispatchedOutcome>,
    pub task_projected_at: Option<DateTime<Utc>>,
}
impl Operation {
    pub fn task_id(&self) -> TaskId {
        TaskId::from_uuid(self.operation_id)
    }
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.stage,
            OperationStage::Succeeded | OperationStage::Failed | OperationStage::Cancelled
        )
    }
}
#[derive(Deserialize, SurrealValue)]
pub(crate) struct OperationRecord {
    operation_id: Uuid,
    computer_id: Uuid,
    task: RecordId,
    actor_context: OpenObject,
    provider_instance_id: Uuid,
    template_fingerprint: String,
    action: String,
    stage: String,
    previous_phase: String,
    previous_resource_id: Option<String>,
    previous_process_id: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    dispatch_id: Option<Uuid>,
    dispatched_at: Option<DateTime<Utc>>,
    observation_deadline: Option<DateTime<Utc>>,
    observation_reads: u32,
    next_observation_at: Option<DateTime<Utc>>,
    last_observation_id: Option<Uuid>,
    settled_at: Option<DateTime<Utc>>,
    result_resource_id: Option<String>,
    result_process_id: Option<String>,
    completion_code: Option<String>,
    task_projected_at: Option<DateTime<Utc>>,
}
impl TryFrom<OperationRecord> for Operation {
    type Error = ComputerError;
    fn try_from(value: OperationRecord) -> Result<Self> {
        if value.operation_id.get_version_num() != 7
            || value.computer_id.is_nil()
            || value.provider_instance_id.is_nil()
            || value.task != TaskId::from_uuid(value.operation_id).record_id()
        {
            return Err(ComputerError::Unavailable);
        }
        let decode = || {
            Ok(Self {
                operation_id: value.operation_id,
                computer_id: value.computer_id,
                actor: serde_json::from_value(serde_json::to_value(value.actor_context)?)?,
                provider_instance_id: value.provider_instance_id,
                template_fingerprint: value.template_fingerprint,
                action: serde_json::from_value(serde_json::Value::String(value.action))?,
                stage: serde_json::from_value(serde_json::Value::String(value.stage))?,
                previous_phase: serde_json::from_value(serde_json::Value::String(
                    value.previous_phase,
                ))?,
                previous_resource_id: value.previous_resource_id,
                previous_process_id: value.previous_process_id,
                created_at: value.created_at,
                updated_at: value.updated_at,
                dispatch_id: value.dispatch_id,
                dispatched_at: value.dispatched_at,
                observation_deadline: value.observation_deadline,
                observation_reads: value.observation_reads,
                next_observation_at: value.next_observation_at,
                last_observation_id: value.last_observation_id,
                settled_at: value.settled_at,
                result_resource_id: value.result_resource_id,
                result_process_id: value.result_process_id,
                completion_code: value
                    .completion_code
                    .map(|code| serde_json::from_value(serde_json::Value::String(code)))
                    .transpose()?,
                task_projected_at: value.task_projected_at,
            })
        };
        decode().map_err(|_: serde_json::Error| ComputerError::Unavailable)
    }
}
