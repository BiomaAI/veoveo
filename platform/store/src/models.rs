use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types as surrealdb_types;
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

macro_rules! string_enum {
    ($(#[$meta:meta])* pub enum $name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, SurrealValue)]
        #[surreal(untagged)]
        pub enum $name {
            $(
                #[serde(rename = $value)]
                #[surreal(value = $value)]
                $variant,
            )+
        }
    };
}

/// A genuinely open-ended JSON object used only at provider/configuration boundaries.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, SurrealValue)]
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

string_enum! {
    pub enum PrincipalKind {
        User => "user",
        Service => "service",
    }
}

string_enum! {
    pub enum OauthClientKind {
        Public => "public",
        Confidential => "confidential",
    }
}

string_enum! {
    pub enum ServerTransport {
        StreamableHttp => "streamable_http",
        Sse => "sse",
        Stdio => "stdio",
    }
}

string_enum! {
    pub enum PolicyState {
        Draft => "draft",
        Active => "active",
        Retired => "retired",
    }
}

string_enum! {
    pub enum GatewayControlRevisionSource {
        AdminApi => "admin_api",
        SeedFile => "seed_file",
    }
}

string_enum! {
    pub enum TaskStatus {
        Queued => "queued",
        Running => "running",
        Waiting => "waiting",
        Succeeded => "succeeded",
        Failed => "failed",
        CancelRequested => "cancel_requested",
        Cancelled => "cancelled",
    }
}

string_enum! {
    pub enum RecoveryClass {
        Resume => "resume",
        WebhookWait => "webhook_wait",
        InterruptedIndeterminate => "interrupted_indeterminate",
    }
}

string_enum! {
    pub enum ProviderJobState {
        Submitted => "submitted",
        Waiting => "waiting",
        Succeeded => "succeeded",
        Failed => "failed",
        CancelRequested => "cancel_requested",
        Cancelled => "cancelled",
    }
}

string_enum! {
    pub enum ArtifactWriteRedemptionState {
        Reserved => "reserved",
        Finalized => "finalized",
    }
}

string_enum! {
    pub enum MediaUsageKind {
        Estimate => "estimate",
        Actual => "actual",
    }
}

string_enum! {
    pub enum DomainUsageKind {
        Estimate => "estimate",
        Actual => "actual",
    }
}

string_enum! {
    pub enum MapReleaseState {
        Staged => "staged",
        Active => "active",
        Retired => "retired",
        Quarantined => "quarantined",
    }
}

string_enum! {
    pub enum MapAcquisitionState {
        Queued => "queued",
        Running => "running",
        Succeeded => "succeeded",
        Failed => "failed",
        CancelRequested => "cancel_requested",
        Cancelled => "cancelled",
    }
}

string_enum! {
    pub enum MapRouteState {
        PlanningAdvisory => "planning_advisory",
        Validated => "validated",
        Stale => "stale",
        Invalidated => "invalidated",
        Unavailable => "unavailable",
    }
}

string_enum! {
    pub enum MapDependencyKind {
        Release => "release",
        Restriction => "restriction",
        Facility => "facility",
    }
}

string_enum! {
    pub enum TimeDatasetKind {
        Tzdb => "tzdb",
        LeapSeconds => "leap_seconds",
    }
}

string_enum! {
    pub enum TimeAuthorityReleaseState {
        Staged => "staged",
        Active => "active",
        Retired => "retired",
        Quarantined => "quarantined",
    }
}

string_enum! {
    pub enum TimeAcquisitionState {
        Queued => "queued",
        Running => "running",
        Succeeded => "succeeded",
        Failed => "failed",
        CancelRequested => "cancel_requested",
        Cancelled => "cancelled",
    }
}

string_enum! {
    pub enum TimeCalendarState {
        Staged => "staged",
        Active => "active",
        Retired => "retired",
    }
}

string_enum! {
    pub enum TimeTemporalEventState {
        Scheduled => "scheduled",
        Due => "due",
        Cancelled => "cancelled",
    }
}

string_enum! {
    pub enum ArtifactReleaseState {
        Private => "private",
        Releasable => "releasable",
        Released => "released",
    }
}

string_enum! {
    pub enum GrantPermission {
        Read => "read",
        Write => "write",
        Admin => "admin",
    }
}

string_enum! {
    pub enum ArtifactGrantSubjectKind {
        Principal => "principal",
        Group => "group",
    }
}

string_enum! {
    pub enum WorkContextMembershipLevel {
        Viewer => "viewer",
        Contributor => "contributor",
        Custodian => "custodian",
        Owner => "owner",
    }
}

string_enum! {
    pub enum InvocationMode {
        Direct => "direct",
        Delegated => "delegated",
        Automated => "automated",
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkContextInitialGrantRecord {
    pub subject_kind: ArtifactGrantSubjectKind,
    pub subject_key: String,
    pub permission: GrantPermission,
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

string_enum! {
    pub enum RecordingState {
        Live => "live",
        Ready => "ready",
        Sealing => "sealing",
        Sealed => "sealed",
        Interrupted => "interrupted",
        Failed => "failed",
    }
}

string_enum! {
    pub enum SegmentState {
        Writing => "writing",
        Frozen => "frozen",
        Sealed => "sealed",
        Failed => "failed",
    }
}

string_enum! {
    pub enum RecordingIngestStreamState {
        Open => "open",
        Finished => "finished",
        Failed => "failed",
    }
}

string_enum! {
    pub enum RecordingIngestBatchState {
        Durable => "durable",
        Materialized => "materialized",
    }
}

string_enum! {
    pub enum AgentState {
        Idle => "idle",
        Running => "running",
        Waiting => "waiting",
        Disabled => "disabled",
        Failed => "failed",
    }
}

string_enum! {
    pub enum WakeKind {
        TaskResult => "task_result",
        ResourceChanged => "resource_changed",
        Timer => "timer",
        OperatorMessage => "operator_message",
        Elicitation => "elicitation",
    }
}

string_enum! {
    pub enum WakeState {
        Pending => "pending",
        Claimed => "claimed",
        Acked => "acked",
        Coalesced => "coalesced",
        Failed => "failed",
    }
}

string_enum! {
    pub enum AgentEpisodeState {
        Running => "running",
        Completed => "completed",
        BudgetTerminated => "budget_terminated",
        Failed => "failed",
        Crashed => "crashed",
    }
}

string_enum! {
    pub enum AgentTaskWatchState {
        Pending => "pending",
        Watching => "watching",
        Resolved => "resolved",
        Failed => "failed",
        Cancelled => "cancelled",
    }
}

string_enum! {
    pub enum AgentElicitationState {
        Parked => "parked",
        Answered => "answered",
        Declined => "declined",
        Cancelled => "cancelled",
    }
}

string_enum! {
    pub enum AuditOutcome {
        Allowed => "allowed",
        Denied => "denied",
        Failed => "failed",
    }
}

string_enum! {
    pub enum GatewayReplayKind {
        ClientAssertion => "client_assertion",
        IdJag => "id_jag",
    }
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
    pub output_policy: OpenObject,
    pub memberships: Vec<OpenObject>,
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
    pub task_type: String,
    pub status: TaskStatus,
    pub recovery_class: RecoveryClass,
    pub request: OpenObject,
    pub progress: f64,
    pub result: Option<OpenObject>,
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
pub struct FrameRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub frame_key: String,
    pub display_name: String,
    pub definition: OpenObject,
    pub proj_pipeline: Option<String>,
    pub classification: String,
    pub labels: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct CoordinateOperationRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub task: Option<RecordId>,
    pub operation_key: String,
    pub kind: String,
    pub provenance: OpenObject,
    pub classification: String,
    pub labels: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapSourceRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub source_key: String,
    pub dataset_key: String,
    pub name: String,
    pub adapter_kind: String,
    pub authority_class: String,
    pub map_families: Vec<String>,
    pub enabled: bool,
    pub canonical_json: String,
    pub record_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapDatasetReleaseRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub release_key: String,
    pub dataset_key: String,
    pub source_key: String,
    pub state: MapReleaseState,
    pub version_label: String,
    pub source_digest_sha256: String,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub canonical_json: String,
    pub record_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapActiveReleaseRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub dataset_key: String,
    pub release_key: String,
    pub previous_release_key: Option<String>,
    pub activated_by: RecordId,
    pub activated_at: DateTime<Utc>,
    pub record_version: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapMobilityProfileRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub profile_key: String,
    pub family: String,
    pub name: String,
    pub profile_version: i64,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub canonical_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapRestrictionRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub restriction_key: String,
    pub kind: String,
    pub effect_kind: String,
    pub affected_mobility_families: Vec<String>,
    pub valid_from: DateTime<Utc>,
    pub valid_until: Option<DateTime<Utc>>,
    pub cancelled_by: Option<String>,
    pub canonical_json: String,
    pub record_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapOperationalSnapshotRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub snapshot_key: String,
    pub departure_time: DateTime<Utc>,
    pub canonical_json: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapRouteRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub route_key: String,
    pub status: MapRouteState,
    pub mobility_profile_key: String,
    pub mobility_profile_version: i64,
    pub operational_snapshot_key: String,
    pub departure_time: DateTime<Utc>,
    pub arrival_time: Option<DateTime<Utc>>,
    pub cache_digest_sha256: String,
    pub canonical_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapRouteDependencyRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub route_key: String,
    pub dependency_kind: MapDependencyKind,
    pub dependency_key: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapRouteMatrixRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub matrix_key: String,
    pub mobility_profile_key: String,
    pub mobility_profile_version: i64,
    pub operational_snapshot_key: String,
    pub artifact_uri: Option<String>,
    pub canonical_json: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct MapAcquisitionRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub acquisition_key: String,
    pub source_key: String,
    pub idempotency_key: String,
    pub status: MapAcquisitionState,
    pub phase: String,
    pub staged_release_key: Option<String>,
    pub canonical_json: String,
    pub record_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct TimeSourceRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub source_key: String,
    pub name: String,
    pub dataset_kind: TimeDatasetKind,
    pub source_url: String,
    pub expected_content_type: String,
    pub enabled: bool,
    pub canonical_json: String,
    pub record_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct TimeAuthorityReleaseRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub release_key: String,
    pub source_key: String,
    pub dataset_kind: TimeDatasetKind,
    pub state: TimeAuthorityReleaseState,
    pub version_label: String,
    pub source_url: String,
    pub source_digest_sha256: String,
    pub artifact_path: String,
    pub retrieved_at: DateTime<Utc>,
    pub validated_at: DateTime<Utc>,
    pub canonical_json: String,
    pub record_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct TimeActiveAuthorityRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub dataset_kind: TimeDatasetKind,
    pub release_key: String,
    pub previous_release_key: Option<String>,
    pub activated_by: RecordId,
    pub activated_at: DateTime<Utc>,
    pub record_version: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct TimeAcquisitionRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub acquisition_key: String,
    pub source_key: String,
    pub expected_source_digest_sha256: Option<String>,
    pub idempotency_key: String,
    pub status: TimeAcquisitionState,
    pub phase: String,
    pub staged_release_key: Option<String>,
    pub canonical_json: String,
    pub record_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct TimeCalendarVersionRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub calendar_key: String,
    pub calendar_version: i64,
    pub name: String,
    pub zone_id: String,
    pub state: TimeCalendarState,
    pub canonical_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct TimeMissionEpochRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub epoch_key: String,
    pub name: String,
    pub epoch_version: i64,
    pub tai_seconds_since_1970: i64,
    pub nanosecond: i64,
    pub canonical_json: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct TimeTemporalEventRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub event_key: String,
    pub name: String,
    pub state: TimeTemporalEventState,
    pub due_tai_seconds_since_1970: i64,
    pub due_nanosecond: i64,
    pub idempotency_key: String,
    pub canonical_json: String,
    pub record_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct TimeClockPolicyRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub maximum_error_nanoseconds: i64,
    pub maximum_stratum: i64,
    pub minimum_source_diversity: i64,
    pub maximum_holdover_seconds: i64,
    pub record_version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct RecordingRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub dataset: String,
    pub application_id: String,
    pub recording_key: String,
    pub state: RecordingState,
    pub classification: String,
    pub labels: Vec<String>,
    pub metadata: OpenObject,
    pub manifest_artifact: Option<RecordId>,
    pub seal_task: Option<RecordId>,
    pub failure_reason: Option<String>,
    pub started_at: DateTime<Utc>,
    pub last_data_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub sealed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct SegmentRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub recording: RecordId,
    pub segment_key: String,
    pub ordinal: i64,
    pub relative_path: String,
    pub artifact: Option<RecordId>,
    pub state: SegmentState,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub byte_len: i64,
    pub message_count: i64,
    pub sha256: Option<String>,
    pub failure_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct RecordingIngestStreamRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub owner: RecordId,
    pub recording: RecordId,
    pub producer_id: String,
    pub oauth_client_id: String,
    pub source_stream_id: String,
    pub application_id: String,
    pub recording_key: String,
    pub dataset: String,
    pub state: RecordingIngestStreamState,
    pub next_sequence: i64,
    pub materialized_through_sequence: Option<i64>,
    pub byte_len: i64,
    pub message_count: i64,
    pub failure_reason: Option<String>,
    pub opened_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct RecordingIngestBatchRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub stream: RecordId,
    pub sequence: i64,
    pub payload_format: String,
    pub sha256: String,
    pub relative_path: String,
    pub byte_len: i64,
    pub message_count: i64,
    pub state: RecordingIngestBatchState,
    pub created_at: DateTime<Utc>,
    pub materialized_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AgentRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub agent_key: String,
    pub display_name: String,
    pub profile: RecordId,
    pub state: AgentState,
    pub manifest: OpenObject,
    pub memory_database: String,
    pub last_episode: Option<RecordId>,
    pub next_episode_sequence: i64,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub fence: i64,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WakeRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub agent: RecordId,
    pub kind: WakeKind,
    pub state: WakeState,
    pub dedupe_key: Option<String>,
    pub payload: OpenObject,
    pub available_at: DateTime<Utc>,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claim_expires_at: Option<DateTime<Utc>>,
    pub claim_fence: Option<i64>,
    pub attempts: i64,
    pub acked_at: Option<DateTime<Utc>>,
    pub acked_by_episode: Option<RecordId>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub revision: i64,
    pub coalesced_into: Option<RecordId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AgentEpisodeRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub agent: RecordId,
    pub sequence: i64,
    pub retention_pin: String,
    pub wake_note: String,
    pub state: AgentEpisodeState,
    pub final_output: Option<String>,
    pub summary: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub completion_calls: i64,
    pub tool_calls: i64,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AgentTaskRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub agent: RecordId,
    pub task: RecordId,
    pub tool_name: String,
    pub descriptor: OpenObject,
    pub descriptor_complete: bool,
    pub state: AgentTaskWatchState,
    pub result: Option<OpenObject>,
    pub result_is_error: bool,
    pub result_wake: Option<RecordId>,
    pub retention_pin: String,
    pub retention_pin_active: bool,
    pub attempt_count: i64,
    pub next_retry_at: DateTime<Utc>,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
    pub started_by_episode: RecordId,
    pub consumed_by_episode: Option<RecordId>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub revision: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AgentElicitationRecord {
    pub id: RecordId,
    pub tenant: RecordId,
    pub agent: RecordId,
    pub related_task: Option<RecordId>,
    pub message: String,
    pub requested_schema: Option<OpenObject>,
    pub state: AgentElicitationState,
    pub answer: Option<OpenObject>,
    pub answered_by: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub answered_at: Option<DateTime<Utc>>,
    pub revision: i64,
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
pub struct AuditEventRecord {
    pub id: RecordId,
    pub tenant: Option<RecordId>,
    pub actor: Option<RecordId>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub outcome: AuditOutcome,
    pub request_id: Option<String>,
    pub trace_id: Option<String>,
    pub source_ip: Option<String>,
    pub details: OpenObject,
    pub occurred_at: DateTime<Utc>,
    pub search_text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct OutboxEventRecord {
    pub id: RecordId,
    pub sequence: i64,
    pub tenant: Option<RecordId>,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub event_type: String,
    pub schema_version: i64,
    pub payload: OpenObject,
    pub occurred_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct RecordingSegmentEdge {
    pub id: RecordId,
    pub r#in: RecordId,
    pub out: RecordId,
    pub ordinal: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct AgentOwnerEdge {
    pub id: RecordId,
    pub r#in: RecordId,
    pub out: RecordId,
    pub role: String,
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
        assert_eq!(
            serde_json::to_string(&WakeKind::OperatorMessage).unwrap(),
            "\"operator_message\""
        );
    }

    #[test]
    fn open_object_rejects_non_objects() {
        assert!(serde_json::from_str::<OpenObject>(r#"{"key":"value"}"#).is_ok());
        assert!(serde_json::from_str::<OpenObject>("[]").is_err());
    }
}
