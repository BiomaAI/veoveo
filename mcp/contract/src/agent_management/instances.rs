use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{AgentDefinitionId, AgentManagedInstanceId, AgentTemplateId};
use crate::{OAuthClientId, Sha256Digest, WorkContextId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InstanceDesired {
    Running,
    Paused,
    Archived,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InstancePhase {
    Queued,
    Credentials,
    Storage,
    Draining,
    Workload,
    Ready,
    Paused,
    Archived,
    Failed,
    Superseded,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedInstance {
    pub id: AgentManagedInstanceId,
    pub name: String,
    pub definition: AgentDefinitionId,
    pub owner: Uuid,
    pub work_context: WorkContextId,
    pub template: AgentTemplateId,
    pub requested_revision: Sha256Digest,
    pub active_revision: Option<Sha256Digest>,
    pub generation: i64,
    pub active_generation: i64,
    pub desired: InstanceDesired,
    pub observed: InstancePhase,
    pub client_id: OAuthClientId,
    pub principal: Uuid,
    pub storage_gib: u32,
    pub operation: Uuid,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstancePage {
    pub items: Vec<ManagedInstance>,
    pub next: Option<AgentManagedInstanceId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProvisionInstance {
    pub request_id: Uuid,
    pub id: AgentManagedInstanceId,
    pub name: String,
    pub definition: AgentDefinitionId,
    pub revision: Sha256Digest,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InstanceChange {
    State { desired: InstanceDesired },
    Revision { revision: Sha256Digest },
    Stop,
    Retry,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateInstance {
    pub request_id: Uuid,
    pub expected_generation: i64,
    pub change: InstanceChange,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LifecycleOperation {
    pub id: Uuid,
    pub instance: AgentManagedInstanceId,
    pub generation: i64,
    pub phase: InstancePhase,
    pub message: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// The kernel checks current model/template authority before dispatch. This
/// internal request cannot select another model, principal or tool allowlist.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedDispatch {
    pub generation: i64,
    pub epoch: i64,
}

/// Signed repository-owned OAuth claim. Current registration still overrides it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedAgentToken {
    pub instance: AgentManagedInstanceId,
    pub generation: i64,
    pub epoch: i64,
}
