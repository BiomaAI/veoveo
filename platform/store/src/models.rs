use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize, SurrealValue)]
#[serde(transparent)]
pub struct RedactedSecret(String);

impl RedactedSecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for RedactedSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("<redacted>")
    }
}

#[cfg(test)]
mod secret_tests {
    use super::RedactedSecret;

    #[test]
    fn debug_never_exposes_secret_value() {
        let secret = RedactedSecret::new("sensitive-capability-secret");
        assert_eq!(format!("{secret:?}"), "<redacted>");
        assert_eq!(secret.expose_secret(), "sensitive-capability-secret");
    }
}

/// A genuinely open-ended JSON object used only at provider/configuration boundaries.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OpenObject(BTreeMap<String, serde_json::Value>);

impl OpenObject {
    pub fn new(values: BTreeMap<String, serde_json::Value>) -> Self {
        Self(values)
    }

    pub fn as_map(&self) -> &BTreeMap<String, serde_json::Value> {
        &self.0
    }

    pub fn into_map(self) -> BTreeMap<String, serde_json::Value> {
        self.0
    }
}

impl From<BTreeMap<String, serde_json::Value>> for OpenObject {
    fn from(value: BTreeMap<String, serde_json::Value>) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum PrincipalKind {
    #[vocabulary(rename = "user")]
    User,
    #[vocabulary(rename = "service")]
    Service,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum OauthClientKind {
    #[vocabulary(rename = "public")]
    Public,
    #[vocabulary(rename = "confidential")]
    Confidential,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum ServerTransport {
    #[vocabulary(rename = "streamable_http")]
    StreamableHttp,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum PolicyState {
    #[vocabulary(rename = "draft")]
    Draft,
    #[vocabulary(rename = "active")]
    Active,
    #[vocabulary(rename = "retired")]
    Retired,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum GatewayControlRevisionSource {
    #[vocabulary(rename = "admin_api")]
    AdminApi,
    #[vocabulary(rename = "seed_file")]
    SeedFile,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum TaskStatus {
    #[vocabulary(rename = "queued")]
    Queued,
    #[vocabulary(rename = "running")]
    Running,
    #[vocabulary(rename = "waiting")]
    Waiting,
    #[vocabulary(rename = "succeeded")]
    Succeeded,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancel_requested")]
    CancelRequested,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum RecoveryClass {
    #[vocabulary(rename = "resume")]
    Resume,
    #[vocabulary(rename = "webhook_wait")]
    WebhookWait,
    #[vocabulary(rename = "provider_wait")]
    ProviderWait,
    #[vocabulary(rename = "interrupted_indeterminate")]
    InterruptedIndeterminate,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum ProviderJobState {
    #[vocabulary(rename = "submitted")]
    Submitted,
    #[vocabulary(rename = "waiting")]
    Waiting,
    #[vocabulary(rename = "succeeded")]
    Succeeded,
    #[vocabulary(rename = "failed")]
    Failed,
    #[vocabulary(rename = "cancel_requested")]
    CancelRequested,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum ArtifactWriteRedemptionState {
    #[vocabulary(rename = "reserved")]
    Reserved,
    #[vocabulary(rename = "finalized")]
    Finalized,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum ArtifactAccessRequestState {
    #[vocabulary(rename = "pending")]
    Pending,
    #[vocabulary(rename = "approved")]
    Approved,
    #[vocabulary(rename = "denied")]
    Denied,
    #[vocabulary(rename = "cancelled")]
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum MediaUsageKind {
    #[vocabulary(rename = "estimate")]
    Estimate,
    #[vocabulary(rename = "actual")]
    Actual,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum DomainUsageKind {
    #[vocabulary(rename = "estimate")]
    Estimate,
    #[vocabulary(rename = "actual")]
    Actual,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum ArtifactReleaseState {
    #[vocabulary(rename = "private")]
    Private,
    #[vocabulary(rename = "releasable")]
    Releasable,
    #[vocabulary(rename = "released")]
    Released,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum GrantPermission {
    #[vocabulary(rename = "read")]
    Read,
    #[vocabulary(rename = "write")]
    Write,
    #[vocabulary(rename = "admin")]
    Admin,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum ArtifactGrantSubjectKind {
    #[vocabulary(rename = "principal")]
    Principal,
    #[vocabulary(rename = "group")]
    Group,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum WorkContextMembershipLevel {
    #[vocabulary(rename = "viewer")]
    Viewer,
    #[vocabulary(rename = "contributor")]
    Contributor,
    #[vocabulary(rename = "custodian")]
    Custodian,
    #[vocabulary(rename = "owner")]
    Owner,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum InvocationMode {
    #[vocabulary(rename = "direct")]
    Direct,
    #[vocabulary(rename = "delegated")]
    Delegated,
    #[vocabulary(rename = "automated")]
    Automated,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkContextInitialGrantRecord {
    pub subject_kind: ArtifactGrantSubjectKind,
    pub subject_key: String,
    pub permission: GrantPermission,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkContextOutputPolicyRecord {
    pub owner_kind: ArtifactGrantSubjectKind,
    pub owner_key: String,
    pub initial_grants: Vec<WorkContextInitialGrantRecord>,
    pub classification: Option<String>,
    pub data_labels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkContextMembershipRuleRecord {
    pub level: WorkContextMembershipLevel,
    pub principals: Vec<String>,
    pub groups: Vec<String>,
    pub roles: Vec<String>,
    pub oauth_clients: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct InvocationAuthorityRecord {
    pub context_key: String,
    pub membership: WorkContextMembershipLevel,
    pub policy_revision: String,
    pub owner_kind: ArtifactGrantSubjectKind,
    pub owner_key: String,
    pub initial_grants: Vec<WorkContextInitialGrantRecord>,
    pub classification: Option<String>,
    pub data_labels: Vec<String>,
    pub invocation_mode: InvocationMode,
    pub initiator_key: Option<String>,
    pub delegation_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, veoveo_types::Vocabulary)]
#[vocabulary(surreal)]
pub enum GatewayReplayKind {
    #[vocabulary(rename = "client_assertion")]
    ClientAssertion,
    #[vocabulary(rename = "id_jag")]
    IdJag,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct EnterpriseRecord {
    pub id: RecordId,
    pub slug: String,
    pub name: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct TenantRecord {
    pub id: RecordId,
    pub enterprise: RecordId,
    pub slug: String,
    pub name: String,
    pub classification_ceiling: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct PrincipalRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub kind: PrincipalKind,
    pub issuer: String,
    pub subject: String,
    pub display_name: String,
    pub email: Option<String>,
    pub claims_hash: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct GroupRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub external_id: String,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct OauthClientRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub client_id: String,
    pub kind: OauthClientKind,
    pub display_name: String,
    pub secret_hash: Option<String>,
    pub redirect_uris: Vec<String>,
    pub grant_types: Vec<String>,
    pub scopes: Vec<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct McpServerRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub server_key: String,
    pub display_name: String,
    pub endpoint: String,
    pub transport: ServerTransport,
    pub manifest: OpenObject,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ProfileRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub profile_key: String,
    pub display_name: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct PolicyRevisionRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub policy_key: String,
    pub revision: i64,
    pub state: PolicyState,
    pub content_hash: String,
    pub document: OpenObject,
    pub created_by: RecordId,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkContextRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub context_key: String,
    pub title: String,
    pub policy_revision: String,
    pub output_policy: WorkContextOutputPolicyRecord,
    pub memberships: Vec<WorkContextMembershipRuleRecord>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct GatewayControlRevisionRecord {
    pub id: RecordId,
    pub revision_id: String,
    pub sha256: String,
    pub source: GatewayControlRevisionSource,
    pub applied_at: DateTime<Utc>,
    pub applied_by: String,
    pub tenant: Option<String>,
    pub control_plane: OpenObject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct GatewayControlRevisionContent {
    pub revision_id: String,
    pub sha256: String,
    pub source: GatewayControlRevisionSource,
    pub applied_at: DateTime<Utc>,
    pub applied_by: String,
    pub tenant: Option<String>,
    pub control_plane: OpenObject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct GatewayControlObjectRecord {
    pub id: RecordId,
    pub revision: RecordId,
    pub tenant: Option<String>,
    pub object_kind: String,
    pub object_id: String,
    pub document: OpenObject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct GatewayControlObjectContent {
    pub revision: RecordId,
    pub tenant: Option<String>,
    pub object_kind: String,
    pub object_id: String,
    pub document: OpenObject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct GatewayControlActiveRecord {
    pub id: RecordId,
    pub revision: RecordId,
    pub revision_id: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct TaskRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub work_context: RecordId,
    pub initiator: Option<RecordId>,
    pub invocation_mode: InvocationMode,
    pub delegation_id: Option<String>,
    pub policy_revision: String,
    pub authority: InvocationAuthorityRecord,
    pub profile: RecordId,
    pub server: RecordId,
    #[surreal(wrap)]
    pub task_type: veoveo_types::TaskTypeName,
    pub status: TaskStatus,
    pub recovery_class: RecoveryClass,
    pub request: OpenObject,
    pub progress: f64,
    pub result: Option<crate::TaskResultRecord>,
    pub error: Option<OpenObject>,
    pub result_artifact: Option<RecordId>,
    pub idempotency_key: Option<String>,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub cancel_requested_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub retention_expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub retention_pins: Vec<String>,
    pub search_text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct TaskIdempotencyRecord {
    pub id: RecordId,
    pub task: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub server: RecordId,
    pub key: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct TaskInputRecord {
    pub id: RecordId,
    pub task: RecordId,
    pub request_key: String,
    pub request: OpenObject,
    pub response: Option<OpenObject>,
    pub created_at: DateTime<Utc>,
    pub responded_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ProviderJobRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub task: RecordId,
    pub provider: String,
    pub external_job_id: String,
    pub state: ProviderJobState,
    pub provider_payload: OpenObject,
    pub submitted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ProviderEventRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub provider_job: RecordId,
    pub provider: String,
    pub event_id: String,
    pub signing_key_id: Option<String>,
    pub payload: OpenObject,
    pub received_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
    pub processing_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactBlobRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub sha256: String,
    pub byte_len: i64,
    pub object_key: String,
    pub content_type: String,
    pub encryption: OpenObject,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactOccurrenceRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub blob: RecordId,
    pub owner: RecordId,
    pub owner_kind: ArtifactGrantSubjectKind,
    pub owner_key: String,
    pub work_context: RecordId,
    pub producer: RecordId,
    pub producer_key: String,
    pub initiator: Option<RecordId>,
    pub initiator_key: Option<String>,
    pub invocation_mode: InvocationMode,
    pub delegation_id: Option<String>,
    pub policy_revision: String,
    pub authority: InvocationAuthorityRecord,
    pub task: Option<RecordId>,
    pub filename: Option<String>,
    pub media_type: String,
    pub classification: String,
    pub labels: Vec<String>,
    pub metadata: OpenObject,
    pub release_state: ArtifactReleaseState,
    pub retention_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub search_text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ShareLinkRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub artifact: RecordId,
    pub created_by: RecordId,
    pub token_hash: String,
    pub permission: GrantPermission,
    pub expires_at: DateTime<Utc>,
    pub max_downloads: Option<i64>,
    pub download_count: i64,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactWriteCapabilityRecord {
    pub audit: crate::audit::AuditContextRecord,
    pub id: RecordId,
    pub tenant: RecordId,
    pub actor: RecordId,
    pub work_context: RecordId,
    pub authority: InvocationAuthorityRecord,
    pub tenant_key: String,
    pub actor_key: String,
    pub actor_kind: PrincipalKind,
    pub actor_issuer: String,
    pub actor_subject: String,
    pub profile_key: String,
    pub server_key: String,
    pub task_id: String,
    pub token_hash: String,
    pub labels: Vec<String>,
    pub max_artifact_count: i64,
    pub max_total_bytes: i64,
    pub used_artifact_count: i64,
    pub used_total_bytes: i64,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactAccessRequestRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub artifact: RecordId,
    pub work_context: RecordId,
    pub work_context_key: String,
    pub requester: RecordId,
    pub requester_key: String,
    pub requested_level: GrantPermission,
    pub justification: String,
    pub state: ArtifactAccessRequestState,
    pub decided_by: Option<RecordId>,
    pub decided_by_key: Option<String>,
    pub decision_note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactWriteRedemptionRecord {
    pub id: RecordId,
    pub capability: RecordId,
    pub tenant: RecordId,
    pub task: RecordId,
    pub task_id: String,
    pub idempotency_key: String,
    pub request_hash: String,
    pub byte_len: i64,
    pub artifact: RecordId,
    pub state: ArtifactWriteRedemptionState,
    pub reserved_at: DateTime<Utc>,
    pub finalized_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MediaTaskContextRecord {
    pub id: RecordId,
    pub task: RecordId,
    pub tenant: RecordId,
    pub capability: RecordId,
    pub capability_secret: RedactedSecret,
    pub capability_expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MediaUsageRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub task: RecordId,
    pub provider_job: Option<RecordId>,
    pub source_id: Option<String>,
    pub model_id: String,
    pub kind: MediaUsageKind,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub amount: Option<f64>,
    pub currency: Option<String>,
    pub metadata: OpenObject,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct DomainUsageRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub task: RecordId,
    pub server: RecordId,
    pub source_id: Option<String>,
    pub provider_job_id: Option<String>,
    pub model_id: String,
    pub kind: DomainUsageKind,
    pub quantity: Option<f64>,
    pub unit: Option<String>,
    pub amount: Option<f64>,
    pub currency: Option<String>,
    pub metadata: OpenObject,
    pub recorded_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct GatewayResourceSubscriptionRecord {
    pub id: RecordId,
    pub profile: String,
    pub owner: String,
    pub upstream_server: String,
    pub resource_uri: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub payload: OpenObject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct GatewayJwtRevocationRecord {
    pub id: RecordId,
    pub profile: String,
    pub issuer: String,
    pub jwt_id: String,
    pub revoked_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub reason: Option<String>,
    pub payload: OpenObject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct GatewayReplayRecord {
    pub id: RecordId,
    pub kind: GatewayReplayKind,
    pub authorization_server: String,
    pub client_id: String,
    pub jwt_id: String,
    pub seen_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct GatewayAuthorizationRequestRecord {
    pub id: RecordId,
    pub idp_state: String,
    pub profile: String,
    pub oauth_client_id: String,
    pub work_context: String,
    pub oidc_client: String,
    pub redirect_uri: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub payload: OpenObject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct GatewayAuthorizationCodeStateRecord {
    pub id: RecordId,
    pub code: String,
    pub profile: String,
    pub oauth_client_id: String,
    pub work_context: String,
    pub oidc_client: String,
    pub principal: String,
    pub redirect_uri: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub payload: OpenObject,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct GatewayRefreshFamilyRecord {
    pub id: RecordId,
    pub authorization_server: String,
    pub profile: String,
    pub oauth_client_id: String,
    pub work_context: String,
    pub principal_id: String,
    pub tenant: Option<String>,
    pub scopes: Vec<String>,
    pub principal: OpenObject,
    pub current_generation: i64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revocation_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct GatewayRefreshTokenRecord {
    pub id: RecordId,
    pub family: RecordId,
    pub token_hash: String,
    pub generation: i64,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub replacement: Option<RecordId>,
    pub replay_detected_at: Option<DateTime<Utc>>,
    pub delivery_envelope: Option<RedactedSecret>,
    pub delivery_expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MembershipEdge {
    pub id: RecordId,
    pub r#in: RecordId,
    pub out: RecordId,
    pub role: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ArtifactGrantEdge {
    pub id: RecordId,
    pub r#in: RecordId,
    pub out: RecordId,
    pub subject_kind: ArtifactGrantSubjectKind,
    pub subject_key: String,
    pub permission: GrantPermission,
    pub labels: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by: RecordId,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct ProfileServerEdge {
    pub id: RecordId,
    pub r#in: RecordId,
    pub out: RecordId,
    pub namespace: String,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct NamedProvenanceEdge {
    pub id: RecordId,
    pub r#in: RecordId,
    pub out: RecordId,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct DerivationEdge {
    pub id: RecordId,
    pub r#in: RecordId,
    pub out: RecordId,
    pub relation: String,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use surrealdb::types::{SurrealValue, Value};

    use super::*;

    #[test]
    fn enum_wire_values_match_schema_literals() {
        assert_eq!(
            TaskStatus::CancelRequested.into_value(),
            Value::String("cancel_requested".to_owned())
        );
        assert_eq!(
            RecoveryClass::InterruptedIndeterminate.into_value(),
            Value::String("interrupted_indeterminate".to_owned())
        );
    }

    #[test]
    fn open_object_rejects_non_objects() {
        assert!(serde_json::from_str::<OpenObject>(r#"{"key":"value"}"#).is_ok());
        assert!(serde_json::from_str::<OpenObject>("[]").is_err());
    }
}

#[cfg(test)]
mod vocabulary_baseline;
