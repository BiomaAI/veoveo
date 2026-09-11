use crate::{AcceptedAuthority, ComputerError, Result, identity::owner_key};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::OpenObject;
use veoveo_task_runtime::{TaskId, TaskOwner};

/// Installation-selected template; public callers never supply its fingerprint.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceTarget {
    pub template_id: String,
    pub template_fingerprint: String,
}
impl MaintenanceTarget {
    pub(super) fn validate(&self) -> Result<()> {
        if self.template_id.is_empty()
            || self.template_id.len() > 64
            || !self
                .template_id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || !fingerprint(&self.template_fingerprint)
        {
            return Err(ComputerError::InvalidInput);
        }
        Ok(())
    }
}
fn fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn native_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && value.bytes().all(|b| b.is_ascii_graphic())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaintenanceSource {
    Ready {
        resource_id: String,
        process_id: String,
    },
    Stopped {
        resource_id: String,
        process_id: String,
    },
    /// Original Create remains unresolved. Only allocator proof can later establish
    /// that its initial admission never acquired a writer. Current absence cannot.
    InitialFailure { operation_id: Uuid },
}
impl MaintenanceSource {
    pub(super) fn validate(&self) -> Result<()> {
        let valid = match self {
            Self::Ready {
                resource_id,
                process_id,
            }
            | Self::Stopped {
                resource_id,
                process_id,
            } => native_id(resource_id) && native_id(process_id),
            Self::InitialFailure { operation_id } => operation_id.get_version_num() == 7,
        };
        if valid {
            Ok(())
        } else {
            Err(ComputerError::InvalidInput)
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceStage {
    Queued,
    Stopping,
    Capturing,
    Retiring,
    Transferring,
    Creating,
    Restoring,
    Adopting,
    Succeeded,
    Cancelled,
    RecoveryRequired,
}

#[derive(Clone, Debug)]
pub struct MaintenanceOperation {
    pub operation_id: Uuid,
    pub request_id: Uuid,
    pub computer_id: Uuid,
    pub actor: TaskOwner,
    pub execution_authority: AcceptedAuthority,
    pub provider_instance_id: Uuid,
    pub source_instance_id: Uuid,
    pub source_template_id: String,
    pub source_template_fingerprint: String,
    pub source: MaintenanceSource,
    pub target_instance_id: Uuid,
    pub target: MaintenanceTarget,
    pub stage: MaintenanceStage,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl MaintenanceOperation {
    pub fn task_id(&self) -> TaskId {
        TaskId::from_uuid(self.operation_id)
    }
}

#[derive(Deserialize, SurrealValue)]
pub(super) struct MaintenanceRecord {
    operation_id: Uuid,
    request_id: Uuid,
    computer_id: Uuid,
    task: RecordId,
    owner_key: String,
    actor_context: OpenObject,
    execution_authority: OpenObject,
    provider_instance_id: Uuid,
    source_instance_id: Uuid,
    source_template_id: String,
    source_template_fingerprint: String,
    source: OpenObject,
    target_instance_id: Uuid,
    target_template_id: String,
    target_template_fingerprint: String,
    stage: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl TryFrom<MaintenanceRecord> for MaintenanceOperation {
    type Error = ComputerError;
    fn try_from(row: MaintenanceRecord) -> Result<Self> {
        let decode = || -> std::result::Result<Self, serde_json::Error> {
            Ok(Self {
                operation_id: row.operation_id,
                request_id: row.request_id,
                computer_id: row.computer_id,
                actor: serde_json::from_value(serde_json::to_value(row.actor_context)?)?,
                execution_authority: serde_json::from_value(serde_json::to_value(
                    row.execution_authority,
                )?)?,
                provider_instance_id: row.provider_instance_id,
                source_instance_id: row.source_instance_id,
                source_template_id: row.source_template_id,
                source_template_fingerprint: row.source_template_fingerprint,
                source: serde_json::from_value(serde_json::to_value(row.source)?)?,
                target_instance_id: row.target_instance_id,
                target: MaintenanceTarget {
                    template_id: row.target_template_id,
                    template_fingerprint: row.target_template_fingerprint,
                },
                stage: serde_json::from_value(serde_json::Value::String(row.stage))?,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
        };
        let op = decode().map_err(|_| ComputerError::Unavailable)?;
        op.execution_authority
            .validate()
            .map_err(|_| ComputerError::Unavailable)?;
        op.target
            .validate()
            .map_err(|_| ComputerError::Unavailable)?;
        op.source
            .validate()
            .map_err(|_| ComputerError::Unavailable)?;
        if op.operation_id.get_version_num() != 7
            || op.request_id.is_nil()
            || [
                op.computer_id,
                op.provider_instance_id,
                op.source_instance_id,
                op.target_instance_id,
            ]
            .iter()
            .any(Uuid::is_nil)
            || op.target_instance_id == op.computer_id
            || op.target_instance_id == op.source_instance_id
            || !fingerprint(&op.source_template_fingerprint)
            || op.source_template_id.is_empty()
            || op.source_template_id.len() > 64
            || row.task != op.task_id().record_id()
            || owner_key(&op.actor)? != row.owner_key
            || op.execution_authority.task_owner() != op.actor
            || matches!(op.source, MaintenanceSource::InitialFailure { .. })
                && op.source_instance_id != op.computer_id
        {
            return Err(ComputerError::Unavailable);
        }
        Ok(op)
    }
}
