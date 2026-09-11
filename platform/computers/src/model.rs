use crate::{ComputerError, Result, api::ComputerPhase};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::OpenObject;
use veoveo_task_runtime::TaskOwner;

/// Internal state, not an HTTP response or an authority token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Computer {
    pub computer_id: Uuid,
    pub owner: TaskOwner,
    pub provider_instance_id: Uuid,
    pub template_id: String,
    pub template_fingerprint: String,
    pub replacement_instance_id: Option<Uuid>,
    pub phase: ComputerPhase,
    pub provider_resource_id: Option<String>,
    pub process_id: Option<String>,
    pub active_operation: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct ComputerPage {
    pub computers: Vec<Computer>,
    pub next_cursor: Option<Uuid>,
}
impl Computer {
    pub fn instance_id(&self) -> Uuid {
        self.replacement_instance_id.unwrap_or(self.computer_id)
    }
}

#[derive(Deserialize, SurrealValue)]
pub(crate) struct ComputerRecord {
    pub computer_id: Uuid,
    pub owner_context: OpenObject,
    pub provider_instance_id: Uuid,
    pub template_id: String,
    pub template_fingerprint: String,
    pub replacement_instance_id: Option<Uuid>,
    pub phase: String,
    pub provider_resource_id: Option<String>,
    pub process_id: Option<String>,
    pub active_operation: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TryFrom<ComputerRecord> for Computer {
    type Error = ComputerError;
    fn try_from(value: ComputerRecord) -> Result<Self> {
        if value
            .replacement_instance_id
            .is_some_and(|id| id.is_nil() || id == value.computer_id)
        {
            return Err(ComputerError::Unavailable);
        }
        let decode = || {
            Ok(Self {
                computer_id: value.computer_id,
                owner: serde_json::from_value(serde_json::to_value(value.owner_context)?)?,
                provider_instance_id: value.provider_instance_id,
                template_id: value.template_id,
                template_fingerprint: value.template_fingerprint,
                replacement_instance_id: value.replacement_instance_id,
                phase: serde_json::from_value(serde_json::Value::String(value.phase))?,
                provider_resource_id: value.provider_resource_id,
                process_id: value.process_id,
                active_operation: value.active_operation,
                created_at: value.created_at,
                updated_at: value.updated_at,
            })
        };
        decode().map_err(|_: serde_json::Error| ComputerError::Unavailable)
    }
}

pub(crate) fn computer_record(id: Uuid) -> RecordId {
    RecordId::new("computer", surrealdb::types::Uuid::from(id))
}
