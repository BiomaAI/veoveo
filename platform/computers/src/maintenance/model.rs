use crate::{AcceptedAuthority, ComputerError, Result, identity::owner_key};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::task_record_id;
use veoveo_task_runtime::TaskOwner;
use veoveo_types::TaskId;

/// Installation-selected template; public callers never supply its fingerprint.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaintenanceTarget {
    pub template_id: crate::api::TemplateId,
    pub template_fingerprint: String,
}
impl MaintenanceTarget {
    pub(super) fn validate(&self) -> Result<()> {
        if !fingerprint(&self.template_fingerprint) {
            return Err(ComputerError::InvalidInput);
        }
        Ok(())
    }
}
pub(super) fn fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn native_id(value: &str) -> bool {
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
    InitialFailure { operation_id: veoveo_types::TaskId },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub(super) enum MaintenanceSourceKind {
    #[vocabulary(rename = "ready")]
    Ready,
    #[vocabulary(rename = "stopped")]
    Stopped,
    #[vocabulary(rename = "initial_failure")]
    InitialFailure,
}

impl MaintenanceSource {
    pub(super) fn lookup(&self) -> (Option<String>, Option<String>) {
        match self {
            Self::Ready {
                resource_id,
                process_id,
            }
            | Self::Stopped {
                resource_id,
                process_id,
            } => (Some(resource_id.clone()), Some(process_id.clone())),
            Self::InitialFailure { .. } => (None, None),
        }
    }
    pub(super) fn kind(&self) -> MaintenanceSourceKind {
        match self {
            Self::Ready { .. } => MaintenanceSourceKind::Ready,
            Self::Stopped { .. } => MaintenanceSourceKind::Stopped,
            Self::InitialFailure { .. } => MaintenanceSourceKind::InitialFailure,
        }
    }

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
            Self::InitialFailure { operation_id } => operation_id.as_uuid().get_version_num() == 7,
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
    pub operation_id: veoveo_types::TaskId,
    pub request_id: crate::api::RequestId,
    pub computer_id: veoveo_computers_contract::ComputerId,
    pub actor: TaskOwner,
    pub execution_authority: AcceptedAuthority,
    pub provider_instance_id: crate::api::ProviderInstanceId,
    pub source_instance_id: Uuid,
    pub source_template_id: crate::api::TemplateId,
    pub source_template_fingerprint: String,
    pub source: MaintenanceSource,
    pub target_instance_id: Uuid,
    pub target: MaintenanceTarget,
    pub stage: MaintenanceStage,
    pub(super) progress: super::progress::MaintenanceProgress,
    pub task_projected_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl MaintenanceOperation {
    pub fn task_id(&self) -> TaskId {
        self.operation_id
    }
}

#[derive(Deserialize, SurrealValue)]
pub(super) struct MaintenanceRecord {
    operation_id: Uuid,
    request_id: Uuid,
    computer_id: Uuid,
    task: RecordId,
    task_tenant: RecordId,
    owner_key: String,
    actor_context: veoveo_platform_store::TaskOwnerRecord,
    execution_authority: crate::AcceptedAuthority,
    provider_instance_id: Uuid,
    source_instance_id: Uuid,
    source_template_id: String,
    source_template_fingerprint: String,
    source: MaintenanceSource,
    source_resource_id: Option<String>,
    source_process_id: Option<String>,
    #[surreal(wrap)]
    source_kind: MaintenanceSourceKind,
    target_instance_id: Uuid,
    target_template_id: String,
    target_template_fingerprint: String,
    stage: String,
    progress: super::progress::MaintenanceProgress,
    task_projected_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
impl TryFrom<MaintenanceRecord> for MaintenanceOperation {
    type Error = ComputerError;
    fn try_from(row: MaintenanceRecord) -> Result<Self> {
        let source_kind = row.source_kind;
        let source_lookup = (
            row.source_resource_id.clone(),
            row.source_process_id.clone(),
        );
        let computer_id = crate::api::ComputerId::try_from(row.computer_id)
            .map_err(|_| ComputerError::Unavailable)?;
        let provider_instance_id =
            crate::api::ProviderInstanceId::try_from(row.provider_instance_id)
                .map_err(|_| ComputerError::Unavailable)?;
        let request_id = crate::api::RequestId::try_from(row.request_id)
            .map_err(|_| ComputerError::Unavailable)?;
        let source_template_id = row
            .source_template_id
            .parse::<crate::api::TemplateId>()
            .map_err(|_| ComputerError::Unavailable)?;
        let target_template_id = row
            .target_template_id
            .parse::<crate::api::TemplateId>()
            .map_err(|_| ComputerError::Unavailable)?;
        let decode = || -> std::result::Result<Self, serde_json::Error> {
            Ok(Self {
                operation_id: TaskId::from_uuid(row.operation_id),
                request_id,
                computer_id,
                actor: serde_json::from_value(serde_json::to_value(row.actor_context)?)?,
                execution_authority: row.execution_authority,
                provider_instance_id,
                source_instance_id: row.source_instance_id,
                source_template_id,
                source_template_fingerprint: row.source_template_fingerprint,
                source: row.source,
                target_instance_id: row.target_instance_id,
                target: MaintenanceTarget {
                    template_id: target_template_id,
                    template_fingerprint: row.target_template_fingerprint,
                },
                stage: serde_json::from_value(serde_json::Value::String(row.stage))?,
                progress: row.progress,
                task_projected_at: row.task_projected_at,
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
        if row.task_tenant != crate::identity::task_tenant(&op.actor)?
            || op.source.kind() != source_kind
            || op.source.lookup() != source_lookup
            || op.operation_id.as_uuid().get_version_num() != 7
            || [op.source_instance_id, op.target_instance_id]
                .iter()
                .any(Uuid::is_nil)
            || op.target_instance_id == op.computer_id.as_uuid()
            || op.target_instance_id == op.source_instance_id
            || !fingerprint(&op.source_template_fingerprint)
            || row.task != task_record_id(op.task_id())
            || owner_key(&op.actor)? != row.owner_key
            || op.execution_authority.task_owner() != op.actor
            || matches!(op.source, MaintenanceSource::InitialFailure { .. })
                && op.source_instance_id != op.computer_id.as_uuid()
        {
            return Err(ComputerError::Unavailable);
        }
        op.validate_progress()?;
        Ok(op)
    }
}

impl surrealdb::types::SurrealValue for MaintenanceSource {
    fn kind_of() -> surrealdb::types::Kind {
        surrealdb::types::Kind::Object
    }
    fn is_value(value: &surrealdb::types::Value) -> bool {
        Self::from_value(value.clone()).is_ok()
    }
    fn into_value(self) -> surrealdb::types::Value {
        crate::storage_codec::native(crate::storage_codec::encode(&self))
    }
    fn from_value(
        value: surrealdb::types::Value,
    ) -> std::result::Result<Self, surrealdb::types::Error> {
        let value: Self = crate::storage_codec::decode(value)?;
        value
            .validate()
            .map_err(|_| surrealdb::types::Error::internal("invalid maintenance source".into()))?;
        Ok(value)
    }
}
