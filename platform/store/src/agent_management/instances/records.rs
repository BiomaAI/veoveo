use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types as surrealdb_types;
use surrealdb::types::{RecordId, SurrealValue, Value};
use uuid::Uuid;

use crate::WorkContextMembershipLevel;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
pub enum ManagedAgentDesired {
    #[surreal(value = "running")]
    Running,
    #[surreal(value = "paused")]
    Paused,
    #[surreal(value = "archived")]
    Archived,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
pub enum ManagedAgentPhase {
    #[surreal(value = "queued")]
    Queued,
    #[surreal(value = "credentials")]
    Credentials,
    #[surreal(value = "storage")]
    Storage,
    #[surreal(value = "draining")]
    Draining,
    #[surreal(value = "workload")]
    Workload,
    #[surreal(value = "ready")]
    Ready,
    #[surreal(value = "paused")]
    Paused,
    #[surreal(value = "archived")]
    Archived,
    #[surreal(value = "failed")]
    Failed,
    #[surreal(value = "superseded")]
    Superseded,
}

/// Reviewed installation authority. It is not a public mutation body.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
#[serde(deny_unknown_fields)]
pub struct ManagedAgentIdentity {
    pub client_id: String,
    pub issuer: String,
    pub authorization_server: String,
    pub profile: String,
    pub resource: String,
    pub scopes: Vec<String>,
    pub roles: Vec<String>,
    pub membership: WorkContextMembershipLevel,
}

/// Deterministic resource identities, admitted before the first Kubernetes write.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(deny_unknown_fields)]
pub struct ManagedAgentResources {
    pub namespace: String,
    pub workload: String,
    pub credential_secret: String,
    pub volume_claim: String,
    pub template_config_map: String,
    pub image: String,
    pub storage_gib: u32,
}

/// The registration contains public verification material only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(deny_unknown_fields)]
pub struct ManagedAgentPublicKey {
    pub kid: String,
    pub n: String,
    pub e: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ManagedAgentInstance {
    pub id: RecordId,
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub owner: RecordId,
    pub deployed_by: RecordId,
    pub key: String,
    pub name: String,
    pub definition: RecordId,
    pub requested_revision: RecordId,
    pub active_revision: Option<RecordId>,
    pub generation: i64,
    pub active_generation: i64,
    pub dispatch_epoch: i64,
    pub desired: ManagedAgentDesired,
    pub observed: ManagedAgentPhase,
    pub principal: RecordId,
    pub identity: ManagedAgentIdentity,
    pub resources: ManagedAgentResources,
    pub public_key: Option<ManagedAgentPublicKey>,
    pub operation: RecordId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ManagedAgentOperation {
    pub id: RecordId,
    pub instance: RecordId,
    pub tenant: RecordId,
    pub work_context: RecordId,
    pub actor: RecordId,
    pub request_id: Uuid,
    pub fingerprint: String,
    pub generation: i64,
    pub phase: ManagedAgentPhase,
    pub message: Option<String>,
    pub lease_owner: Option<Uuid>,
    pub lease_fence: i64,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Server-side admission decision after validating the published revision and
/// the complete installation template. Never deserialize this from HTTP.
#[derive(Clone, Debug, Serialize, SurrealValue)]
pub struct ManagedAgentProvision {
    pub name: String,
    pub definition_key: String,
    pub revision: String,
    pub identity: ManagedAgentIdentity,
    pub resources: ManagedAgentResources,
}

/// Quotas count retained instances and retained PVCs, including archived ones.
#[derive(Clone, Copy, SurrealValue)]
pub struct ManagedAgentLimits {
    pub instances: u32,
    pub storage_gib: u32,
}

#[derive(Clone, Debug, Serialize, SurrealValue)]
#[surreal(tag = "kind")]
pub enum ManagedAgentMutation {
    #[surreal(rename = "provision")]
    Provision { plan: Box<ManagedAgentProvision> },
    #[surreal(rename = "state")]
    State { desired: ManagedAgentDesired },
    #[surreal(rename = "revision")]
    Revision { digest: String },
    #[surreal(rename = "stop")]
    Stop,
    #[surreal(rename = "retry")]
    Retry,
}

/// A controller's claim is bound to one generation and has no public decoder.
#[derive(Clone, SurrealValue)]
pub struct ManagedAgentClaim {
    pub operation: RecordId,
    pub instance: RecordId,
    pub generation: i64,
    pub owner: Uuid,
    pub fence: i64,
}

impl ManagedAgentOperation {
    pub fn claim(&self, owner: Uuid) -> Option<ManagedAgentClaim> {
        (self.lease_owner == Some(owner)).then(|| ManagedAgentClaim {
            operation: self.id.clone(),
            instance: self.instance.clone(),
            generation: self.generation,
            owner,
            fence: self.lease_fence,
        })
    }
}

#[derive(Clone, SurrealValue)]
pub(super) struct InstanceCommand {
    pub instance: RecordId,
    pub operation: RecordId,
    pub request_id: Uuid,
    pub fingerprint: String,
    pub key: String,
    pub expected_generation: Option<i64>,
    pub mutation: ManagedAgentMutation,
    pub definition: Option<RecordId>,
    pub principal: Option<RecordId>,
    pub limits: ManagedAgentLimits,
}
