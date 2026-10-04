//! SDK value/serde declarations captured before the Vocabulary migration.

use surrealdb::types as surrealdb_types;
use surrealdb::types::SurrealValue;
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum PrincipalKind {
    #[serde(rename = "user")]
    #[surreal(value = "user")]
    User,
    #[serde(rename = "service")]
    #[surreal(value = "service")]
    Service,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum OauthClientKind {
    #[serde(rename = "public")]
    #[surreal(value = "public")]
    Public,
    #[serde(rename = "confidential")]
    #[surreal(value = "confidential")]
    Confidential,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum ServerTransport {
    #[serde(rename = "streamable_http")]
    #[surreal(value = "streamable_http")]
    StreamableHttp,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum PolicyState {
    #[serde(rename = "draft")]
    #[surreal(value = "draft")]
    Draft,
    #[serde(rename = "active")]
    #[surreal(value = "active")]
    Active,
    #[serde(rename = "retired")]
    #[surreal(value = "retired")]
    Retired,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum GatewayControlRevisionSource {
    #[serde(rename = "admin_api")]
    #[surreal(value = "admin_api")]
    AdminApi,
    #[serde(rename = "seed_file")]
    #[surreal(value = "seed_file")]
    SeedFile,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum TaskStatus {
    #[serde(rename = "queued")]
    #[surreal(value = "queued")]
    Queued,
    #[serde(rename = "running")]
    #[surreal(value = "running")]
    Running,
    #[serde(rename = "waiting")]
    #[surreal(value = "waiting")]
    Waiting,
    #[serde(rename = "succeeded")]
    #[surreal(value = "succeeded")]
    Succeeded,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
    #[serde(rename = "cancel_requested")]
    #[surreal(value = "cancel_requested")]
    CancelRequested,
    #[serde(rename = "cancelled")]
    #[surreal(value = "cancelled")]
    Cancelled,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum RecoveryClass {
    #[serde(rename = "resume")]
    #[surreal(value = "resume")]
    Resume,
    #[serde(rename = "webhook_wait")]
    #[surreal(value = "webhook_wait")]
    WebhookWait,
    #[serde(rename = "provider_wait")]
    #[surreal(value = "provider_wait")]
    ProviderWait,
    #[serde(rename = "interrupted_indeterminate")]
    #[surreal(value = "interrupted_indeterminate")]
    InterruptedIndeterminate,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum ProviderJobState {
    #[serde(rename = "submitted")]
    #[surreal(value = "submitted")]
    Submitted,
    #[serde(rename = "waiting")]
    #[surreal(value = "waiting")]
    Waiting,
    #[serde(rename = "succeeded")]
    #[surreal(value = "succeeded")]
    Succeeded,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
    #[serde(rename = "cancel_requested")]
    #[surreal(value = "cancel_requested")]
    CancelRequested,
    #[serde(rename = "cancelled")]
    #[surreal(value = "cancelled")]
    Cancelled,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum ArtifactWriteRedemptionState {
    #[serde(rename = "reserved")]
    #[surreal(value = "reserved")]
    Reserved,
    #[serde(rename = "finalized")]
    #[surreal(value = "finalized")]
    Finalized,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum ArtifactAccessRequestState {
    #[serde(rename = "pending")]
    #[surreal(value = "pending")]
    Pending,
    #[serde(rename = "approved")]
    #[surreal(value = "approved")]
    Approved,
    #[serde(rename = "denied")]
    #[surreal(value = "denied")]
    Denied,
    #[serde(rename = "cancelled")]
    #[surreal(value = "cancelled")]
    Cancelled,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum MediaUsageKind {
    #[serde(rename = "estimate")]
    #[surreal(value = "estimate")]
    Estimate,
    #[serde(rename = "actual")]
    #[surreal(value = "actual")]
    Actual,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum DomainUsageKind {
    #[serde(rename = "estimate")]
    #[surreal(value = "estimate")]
    Estimate,
    #[serde(rename = "actual")]
    #[surreal(value = "actual")]
    Actual,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum MapReleaseState {
    #[serde(rename = "staged")]
    #[surreal(value = "staged")]
    Staged,
    #[serde(rename = "active")]
    #[surreal(value = "active")]
    Active,
    #[serde(rename = "retired")]
    #[surreal(value = "retired")]
    Retired,
    #[serde(rename = "quarantined")]
    #[surreal(value = "quarantined")]
    Quarantined,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum MapAcquisitionState {
    #[serde(rename = "queued")]
    #[surreal(value = "queued")]
    Queued,
    #[serde(rename = "running")]
    #[surreal(value = "running")]
    Running,
    #[serde(rename = "succeeded")]
    #[surreal(value = "succeeded")]
    Succeeded,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
    #[serde(rename = "cancel_requested")]
    #[surreal(value = "cancel_requested")]
    CancelRequested,
    #[serde(rename = "cancelled")]
    #[surreal(value = "cancelled")]
    Cancelled,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum MapRouteState {
    #[serde(rename = "planning_advisory")]
    #[surreal(value = "planning_advisory")]
    PlanningAdvisory,
    #[serde(rename = "validated")]
    #[surreal(value = "validated")]
    Validated,
    #[serde(rename = "stale")]
    #[surreal(value = "stale")]
    Stale,
    #[serde(rename = "invalidated")]
    #[surreal(value = "invalidated")]
    Invalidated,
    #[serde(rename = "unavailable")]
    #[surreal(value = "unavailable")]
    Unavailable,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum MapDependencyKind {
    #[serde(rename = "release")]
    #[surreal(value = "release")]
    Release,
    #[serde(rename = "restriction")]
    #[surreal(value = "restriction")]
    Restriction,
    #[serde(rename = "facility")]
    #[surreal(value = "facility")]
    Facility,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum ArtifactReleaseState {
    #[serde(rename = "private")]
    #[surreal(value = "private")]
    Private,
    #[serde(rename = "releasable")]
    #[surreal(value = "releasable")]
    Releasable,
    #[serde(rename = "released")]
    #[surreal(value = "released")]
    Released,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum GrantPermission {
    #[serde(rename = "read")]
    #[surreal(value = "read")]
    Read,
    #[serde(rename = "write")]
    #[surreal(value = "write")]
    Write,
    #[serde(rename = "admin")]
    #[surreal(value = "admin")]
    Admin,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum ArtifactGrantSubjectKind {
    #[serde(rename = "principal")]
    #[surreal(value = "principal")]
    Principal,
    #[serde(rename = "group")]
    #[surreal(value = "group")]
    Group,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum WorkContextMembershipLevel {
    #[serde(rename = "viewer")]
    #[surreal(value = "viewer")]
    Viewer,
    #[serde(rename = "contributor")]
    #[surreal(value = "contributor")]
    Contributor,
    #[serde(rename = "custodian")]
    #[surreal(value = "custodian")]
    Custodian,
    #[serde(rename = "owner")]
    #[surreal(value = "owner")]
    Owner,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum InvocationMode {
    #[serde(rename = "direct")]
    #[surreal(value = "direct")]
    Direct,
    #[serde(rename = "delegated")]
    #[surreal(value = "delegated")]
    Delegated,
    #[serde(rename = "automated")]
    #[surreal(value = "automated")]
    Automated,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum RecordingState {
    #[serde(rename = "live")]
    #[surreal(value = "live")]
    Live,
    #[serde(rename = "ready")]
    #[surreal(value = "ready")]
    Ready,
    #[serde(rename = "sealing")]
    #[surreal(value = "sealing")]
    Sealing,
    #[serde(rename = "sealed")]
    #[surreal(value = "sealed")]
    Sealed,
    #[serde(rename = "interrupted")]
    #[surreal(value = "interrupted")]
    Interrupted,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum RecordingRetentionMode {
    #[serde(rename = "installation_default")]
    #[surreal(value = "installation_default")]
    InstallationDefault,
    #[serde(rename = "retain_until")]
    #[surreal(value = "retain_until")]
    RetainUntil,
    #[serde(rename = "retain_forever")]
    #[surreal(value = "retain_forever")]
    RetainForever,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum RecordingLayerKind {
    #[serde(rename = "capture")]
    #[surreal(value = "capture")]
    Capture,
    #[serde(rename = "properties")]
    #[surreal(value = "properties")]
    Properties,
    #[serde(rename = "derived")]
    #[surreal(value = "derived")]
    Derived,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum RecordingLayerState {
    #[serde(rename = "writing")]
    #[surreal(value = "writing")]
    Writing,
    #[serde(rename = "staged")]
    #[surreal(value = "staged")]
    Staged,
    #[serde(rename = "committed")]
    #[surreal(value = "committed")]
    Committed,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum RecordingReadGrantClass {
    #[serde(rename = "viewer_segment")]
    #[surreal(value = "viewer_segment")]
    ViewerSegment,
    #[serde(rename = "catalog_dataset")]
    #[surreal(value = "catalog_dataset")]
    CatalogDataset,
    #[serde(rename = "app_projection")]
    #[surreal(value = "app_projection")]
    AppProjection,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum RecordingProjectionState {
    #[serde(rename = "reserved")]
    #[surreal(value = "reserved")]
    Reserved,
    #[serde(rename = "materializing")]
    #[surreal(value = "materializing")]
    Materializing,
    #[serde(rename = "ready")]
    #[surreal(value = "ready")]
    Ready,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    #[surreal(value = "cancelled")]
    Cancelled,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum RecordingIngestStreamState {
    #[serde(rename = "open")]
    #[surreal(value = "open")]
    Open,
    #[serde(rename = "finished")]
    #[surreal(value = "finished")]
    Finished,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum RecordingIngestBatchState {
    #[serde(rename = "durable")]
    #[surreal(value = "durable")]
    Durable,
    #[serde(rename = "materialized")]
    #[surreal(value = "materialized")]
    Materialized,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum AgentState {
    #[serde(rename = "idle")]
    #[surreal(value = "idle")]
    Idle,
    #[serde(rename = "running")]
    #[surreal(value = "running")]
    Running,
    #[serde(rename = "waiting")]
    #[surreal(value = "waiting")]
    Waiting,
    #[serde(rename = "disabled")]
    #[surreal(value = "disabled")]
    Disabled,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum WakeKind {
    #[serde(rename = "task_result")]
    #[surreal(value = "task_result")]
    TaskResult,
    #[serde(rename = "resource_changed")]
    #[surreal(value = "resource_changed")]
    ResourceChanged,
    #[serde(rename = "timer")]
    #[surreal(value = "timer")]
    Timer,
    #[serde(rename = "operator_message")]
    #[surreal(value = "operator_message")]
    OperatorMessage,
    #[serde(rename = "input_request")]
    #[surreal(value = "input_request")]
    InputRequest,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum WakeState {
    #[serde(rename = "pending")]
    #[surreal(value = "pending")]
    Pending,
    #[serde(rename = "claimed")]
    #[surreal(value = "claimed")]
    Claimed,
    #[serde(rename = "acked")]
    #[surreal(value = "acked")]
    Acked,
    #[serde(rename = "coalesced")]
    #[surreal(value = "coalesced")]
    Coalesced,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum AgentEpisodeState {
    #[serde(rename = "running")]
    #[surreal(value = "running")]
    Running,
    #[serde(rename = "completed")]
    #[surreal(value = "completed")]
    Completed,
    #[serde(rename = "budget_terminated")]
    #[surreal(value = "budget_terminated")]
    BudgetTerminated,
    #[serde(rename = "stopped")]
    #[surreal(value = "stopped")]
    Stopped,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
    #[serde(rename = "crashed")]
    #[surreal(value = "crashed")]
    Crashed,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum AgentTaskWatchState {
    #[serde(rename = "pending")]
    #[surreal(value = "pending")]
    Pending,
    #[serde(rename = "watching")]
    #[surreal(value = "watching")]
    Watching,
    #[serde(rename = "resolved")]
    #[surreal(value = "resolved")]
    Resolved,
    #[serde(rename = "failed")]
    #[surreal(value = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    #[surreal(value = "cancelled")]
    Cancelled,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum AgentInputRequestState {
    #[serde(rename = "pending")]
    #[surreal(value = "pending")]
    Pending,
    #[serde(rename = "answered")]
    #[surreal(value = "answered")]
    Answered,
    #[serde(rename = "declined")]
    #[surreal(value = "declined")]
    Declined,
    #[serde(rename = "cancelled")]
    #[surreal(value = "cancelled")]
    Cancelled,
}
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    surrealdb::types::SurrealValue,
)]
#[surreal(untagged)]
enum GatewayReplayKind {
    #[serde(rename = "client_assertion")]
    #[surreal(value = "client_assertion")]
    ClientAssertion,
    #[serde(rename = "id_jag")]
    #[surreal(value = "id_jag")]
    IdJag,
}
fn compare<A, B>(actual: A, before: B, spelling: &str)
where
    A: Copy
        + std::fmt::Debug
        + PartialEq
        + SurrealValue
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>
        + veoveo_types::Vocabulary,
    B: Copy + SurrealValue + serde::Serialize,
{
    let database = before.into_value();
    assert_eq!(actual.into_value(), database);
    assert_eq!(A::from_value(database.clone()).unwrap(), actual);
    assert_eq!(A::is_value(&database), B::is_value(&database));
    for invalid in [
        surrealdb::types::Value::String("unknown".into()),
        surrealdb::types::Value::None,
    ] {
        assert_eq!(A::is_value(&invalid), B::is_value(&invalid));
        assert_eq!(
            A::from_value(invalid.clone()).is_err(),
            B::from_value(invalid).is_err()
        );
    }
    let wire = serde_json::to_value(before).unwrap();
    assert_eq!(serde_json::to_value(actual).unwrap(), wire);
    assert_eq!(serde_json::from_value::<A>(wire).unwrap(), actual);
    assert_eq!(actual.as_str(), spelling);
}
#[test]
fn database_vocabulary_values_preserve_the_published_profile() {
    assert_eq!(super::PrincipalKind::kind_of(), PrincipalKind::kind_of());
    compare(super::PrincipalKind::User, PrincipalKind::User, "user");
    compare(
        super::PrincipalKind::Service,
        PrincipalKind::Service,
        "service",
    );
    assert_eq!(
        super::OauthClientKind::kind_of(),
        OauthClientKind::kind_of()
    );
    compare(
        super::OauthClientKind::Public,
        OauthClientKind::Public,
        "public",
    );
    compare(
        super::OauthClientKind::Confidential,
        OauthClientKind::Confidential,
        "confidential",
    );
    assert_eq!(
        super::ServerTransport::kind_of(),
        ServerTransport::kind_of()
    );
    compare(
        super::ServerTransport::StreamableHttp,
        ServerTransport::StreamableHttp,
        "streamable_http",
    );
    assert_eq!(super::PolicyState::kind_of(), PolicyState::kind_of());
    compare(super::PolicyState::Draft, PolicyState::Draft, "draft");
    compare(super::PolicyState::Active, PolicyState::Active, "active");
    compare(super::PolicyState::Retired, PolicyState::Retired, "retired");
    assert_eq!(
        super::GatewayControlRevisionSource::kind_of(),
        GatewayControlRevisionSource::kind_of()
    );
    compare(
        super::GatewayControlRevisionSource::AdminApi,
        GatewayControlRevisionSource::AdminApi,
        "admin_api",
    );
    compare(
        super::GatewayControlRevisionSource::SeedFile,
        GatewayControlRevisionSource::SeedFile,
        "seed_file",
    );
    assert_eq!(super::TaskStatus::kind_of(), TaskStatus::kind_of());
    compare(super::TaskStatus::Queued, TaskStatus::Queued, "queued");
    compare(super::TaskStatus::Running, TaskStatus::Running, "running");
    compare(super::TaskStatus::Waiting, TaskStatus::Waiting, "waiting");
    compare(
        super::TaskStatus::Succeeded,
        TaskStatus::Succeeded,
        "succeeded",
    );
    compare(super::TaskStatus::Failed, TaskStatus::Failed, "failed");
    compare(
        super::TaskStatus::CancelRequested,
        TaskStatus::CancelRequested,
        "cancel_requested",
    );
    compare(
        super::TaskStatus::Cancelled,
        TaskStatus::Cancelled,
        "cancelled",
    );
    assert_eq!(super::RecoveryClass::kind_of(), RecoveryClass::kind_of());
    compare(
        super::RecoveryClass::Resume,
        RecoveryClass::Resume,
        "resume",
    );
    compare(
        super::RecoveryClass::WebhookWait,
        RecoveryClass::WebhookWait,
        "webhook_wait",
    );
    compare(
        super::RecoveryClass::ProviderWait,
        RecoveryClass::ProviderWait,
        "provider_wait",
    );
    compare(
        super::RecoveryClass::InterruptedIndeterminate,
        RecoveryClass::InterruptedIndeterminate,
        "interrupted_indeterminate",
    );
    assert_eq!(
        super::ProviderJobState::kind_of(),
        ProviderJobState::kind_of()
    );
    compare(
        super::ProviderJobState::Submitted,
        ProviderJobState::Submitted,
        "submitted",
    );
    compare(
        super::ProviderJobState::Waiting,
        ProviderJobState::Waiting,
        "waiting",
    );
    compare(
        super::ProviderJobState::Succeeded,
        ProviderJobState::Succeeded,
        "succeeded",
    );
    compare(
        super::ProviderJobState::Failed,
        ProviderJobState::Failed,
        "failed",
    );
    compare(
        super::ProviderJobState::CancelRequested,
        ProviderJobState::CancelRequested,
        "cancel_requested",
    );
    compare(
        super::ProviderJobState::Cancelled,
        ProviderJobState::Cancelled,
        "cancelled",
    );
    assert_eq!(
        super::ArtifactWriteRedemptionState::kind_of(),
        ArtifactWriteRedemptionState::kind_of()
    );
    compare(
        super::ArtifactWriteRedemptionState::Reserved,
        ArtifactWriteRedemptionState::Reserved,
        "reserved",
    );
    compare(
        super::ArtifactWriteRedemptionState::Finalized,
        ArtifactWriteRedemptionState::Finalized,
        "finalized",
    );
    assert_eq!(
        super::ArtifactAccessRequestState::kind_of(),
        ArtifactAccessRequestState::kind_of()
    );
    compare(
        super::ArtifactAccessRequestState::Pending,
        ArtifactAccessRequestState::Pending,
        "pending",
    );
    compare(
        super::ArtifactAccessRequestState::Approved,
        ArtifactAccessRequestState::Approved,
        "approved",
    );
    compare(
        super::ArtifactAccessRequestState::Denied,
        ArtifactAccessRequestState::Denied,
        "denied",
    );
    compare(
        super::ArtifactAccessRequestState::Cancelled,
        ArtifactAccessRequestState::Cancelled,
        "cancelled",
    );
    assert_eq!(super::MediaUsageKind::kind_of(), MediaUsageKind::kind_of());
    compare(
        super::MediaUsageKind::Estimate,
        MediaUsageKind::Estimate,
        "estimate",
    );
    compare(
        super::MediaUsageKind::Actual,
        MediaUsageKind::Actual,
        "actual",
    );
    assert_eq!(
        super::DomainUsageKind::kind_of(),
        DomainUsageKind::kind_of()
    );
    compare(
        super::DomainUsageKind::Estimate,
        DomainUsageKind::Estimate,
        "estimate",
    );
    compare(
        super::DomainUsageKind::Actual,
        DomainUsageKind::Actual,
        "actual",
    );
    assert_eq!(
        super::MapReleaseState::kind_of(),
        MapReleaseState::kind_of()
    );
    compare(
        super::MapReleaseState::Staged,
        MapReleaseState::Staged,
        "staged",
    );
    compare(
        super::MapReleaseState::Active,
        MapReleaseState::Active,
        "active",
    );
    compare(
        super::MapReleaseState::Retired,
        MapReleaseState::Retired,
        "retired",
    );
    compare(
        super::MapReleaseState::Quarantined,
        MapReleaseState::Quarantined,
        "quarantined",
    );
    assert_eq!(
        super::MapAcquisitionState::kind_of(),
        MapAcquisitionState::kind_of()
    );
    compare(
        super::MapAcquisitionState::Queued,
        MapAcquisitionState::Queued,
        "queued",
    );
    compare(
        super::MapAcquisitionState::Running,
        MapAcquisitionState::Running,
        "running",
    );
    compare(
        super::MapAcquisitionState::Succeeded,
        MapAcquisitionState::Succeeded,
        "succeeded",
    );
    compare(
        super::MapAcquisitionState::Failed,
        MapAcquisitionState::Failed,
        "failed",
    );
    compare(
        super::MapAcquisitionState::CancelRequested,
        MapAcquisitionState::CancelRequested,
        "cancel_requested",
    );
    compare(
        super::MapAcquisitionState::Cancelled,
        MapAcquisitionState::Cancelled,
        "cancelled",
    );
    assert_eq!(super::MapRouteState::kind_of(), MapRouteState::kind_of());
    compare(
        super::MapRouteState::PlanningAdvisory,
        MapRouteState::PlanningAdvisory,
        "planning_advisory",
    );
    compare(
        super::MapRouteState::Validated,
        MapRouteState::Validated,
        "validated",
    );
    compare(super::MapRouteState::Stale, MapRouteState::Stale, "stale");
    compare(
        super::MapRouteState::Invalidated,
        MapRouteState::Invalidated,
        "invalidated",
    );
    compare(
        super::MapRouteState::Unavailable,
        MapRouteState::Unavailable,
        "unavailable",
    );
    assert_eq!(
        super::MapDependencyKind::kind_of(),
        MapDependencyKind::kind_of()
    );
    compare(
        super::MapDependencyKind::Release,
        MapDependencyKind::Release,
        "release",
    );
    compare(
        super::MapDependencyKind::Restriction,
        MapDependencyKind::Restriction,
        "restriction",
    );
    compare(
        super::MapDependencyKind::Facility,
        MapDependencyKind::Facility,
        "facility",
    );
    assert_eq!(
        super::ArtifactReleaseState::kind_of(),
        ArtifactReleaseState::kind_of()
    );
    compare(
        super::ArtifactReleaseState::Private,
        ArtifactReleaseState::Private,
        "private",
    );
    compare(
        super::ArtifactReleaseState::Releasable,
        ArtifactReleaseState::Releasable,
        "releasable",
    );
    compare(
        super::ArtifactReleaseState::Released,
        ArtifactReleaseState::Released,
        "released",
    );
    assert_eq!(
        super::GrantPermission::kind_of(),
        GrantPermission::kind_of()
    );
    compare(super::GrantPermission::Read, GrantPermission::Read, "read");
    compare(
        super::GrantPermission::Write,
        GrantPermission::Write,
        "write",
    );
    compare(
        super::GrantPermission::Admin,
        GrantPermission::Admin,
        "admin",
    );
    assert_eq!(
        super::ArtifactGrantSubjectKind::kind_of(),
        ArtifactGrantSubjectKind::kind_of()
    );
    compare(
        super::ArtifactGrantSubjectKind::Principal,
        ArtifactGrantSubjectKind::Principal,
        "principal",
    );
    compare(
        super::ArtifactGrantSubjectKind::Group,
        ArtifactGrantSubjectKind::Group,
        "group",
    );
    assert_eq!(
        super::WorkContextMembershipLevel::kind_of(),
        WorkContextMembershipLevel::kind_of()
    );
    compare(
        super::WorkContextMembershipLevel::Viewer,
        WorkContextMembershipLevel::Viewer,
        "viewer",
    );
    compare(
        super::WorkContextMembershipLevel::Contributor,
        WorkContextMembershipLevel::Contributor,
        "contributor",
    );
    compare(
        super::WorkContextMembershipLevel::Custodian,
        WorkContextMembershipLevel::Custodian,
        "custodian",
    );
    compare(
        super::WorkContextMembershipLevel::Owner,
        WorkContextMembershipLevel::Owner,
        "owner",
    );
    assert_eq!(super::InvocationMode::kind_of(), InvocationMode::kind_of());
    compare(
        super::InvocationMode::Direct,
        InvocationMode::Direct,
        "direct",
    );
    compare(
        super::InvocationMode::Delegated,
        InvocationMode::Delegated,
        "delegated",
    );
    compare(
        super::InvocationMode::Automated,
        InvocationMode::Automated,
        "automated",
    );
    assert_eq!(super::RecordingState::kind_of(), RecordingState::kind_of());
    compare(super::RecordingState::Live, RecordingState::Live, "live");
    compare(super::RecordingState::Ready, RecordingState::Ready, "ready");
    compare(
        super::RecordingState::Sealing,
        RecordingState::Sealing,
        "sealing",
    );
    compare(
        super::RecordingState::Sealed,
        RecordingState::Sealed,
        "sealed",
    );
    compare(
        super::RecordingState::Interrupted,
        RecordingState::Interrupted,
        "interrupted",
    );
    compare(
        super::RecordingState::Failed,
        RecordingState::Failed,
        "failed",
    );
    assert_eq!(
        super::RecordingRetentionMode::kind_of(),
        RecordingRetentionMode::kind_of()
    );
    compare(
        super::RecordingRetentionMode::InstallationDefault,
        RecordingRetentionMode::InstallationDefault,
        "installation_default",
    );
    compare(
        super::RecordingRetentionMode::RetainUntil,
        RecordingRetentionMode::RetainUntil,
        "retain_until",
    );
    compare(
        super::RecordingRetentionMode::RetainForever,
        RecordingRetentionMode::RetainForever,
        "retain_forever",
    );
    assert_eq!(
        super::RecordingLayerKind::kind_of(),
        RecordingLayerKind::kind_of()
    );
    compare(
        super::RecordingLayerKind::Capture,
        RecordingLayerKind::Capture,
        "capture",
    );
    compare(
        super::RecordingLayerKind::Properties,
        RecordingLayerKind::Properties,
        "properties",
    );
    compare(
        super::RecordingLayerKind::Derived,
        RecordingLayerKind::Derived,
        "derived",
    );
    assert_eq!(
        super::RecordingLayerState::kind_of(),
        RecordingLayerState::kind_of()
    );
    compare(
        super::RecordingLayerState::Writing,
        RecordingLayerState::Writing,
        "writing",
    );
    compare(
        super::RecordingLayerState::Staged,
        RecordingLayerState::Staged,
        "staged",
    );
    compare(
        super::RecordingLayerState::Committed,
        RecordingLayerState::Committed,
        "committed",
    );
    compare(
        super::RecordingLayerState::Failed,
        RecordingLayerState::Failed,
        "failed",
    );
    assert_eq!(
        super::RecordingReadGrantClass::kind_of(),
        RecordingReadGrantClass::kind_of()
    );
    compare(
        super::RecordingReadGrantClass::ViewerSegment,
        RecordingReadGrantClass::ViewerSegment,
        "viewer_segment",
    );
    compare(
        super::RecordingReadGrantClass::CatalogDataset,
        RecordingReadGrantClass::CatalogDataset,
        "catalog_dataset",
    );
    compare(
        super::RecordingReadGrantClass::AppProjection,
        RecordingReadGrantClass::AppProjection,
        "app_projection",
    );
    assert_eq!(
        super::RecordingProjectionState::kind_of(),
        RecordingProjectionState::kind_of()
    );
    compare(
        super::RecordingProjectionState::Reserved,
        RecordingProjectionState::Reserved,
        "reserved",
    );
    compare(
        super::RecordingProjectionState::Materializing,
        RecordingProjectionState::Materializing,
        "materializing",
    );
    compare(
        super::RecordingProjectionState::Ready,
        RecordingProjectionState::Ready,
        "ready",
    );
    compare(
        super::RecordingProjectionState::Failed,
        RecordingProjectionState::Failed,
        "failed",
    );
    compare(
        super::RecordingProjectionState::Cancelled,
        RecordingProjectionState::Cancelled,
        "cancelled",
    );
    assert_eq!(
        super::RecordingIngestStreamState::kind_of(),
        RecordingIngestStreamState::kind_of()
    );
    compare(
        super::RecordingIngestStreamState::Open,
        RecordingIngestStreamState::Open,
        "open",
    );
    compare(
        super::RecordingIngestStreamState::Finished,
        RecordingIngestStreamState::Finished,
        "finished",
    );
    compare(
        super::RecordingIngestStreamState::Failed,
        RecordingIngestStreamState::Failed,
        "failed",
    );
    assert_eq!(
        super::RecordingIngestBatchState::kind_of(),
        RecordingIngestBatchState::kind_of()
    );
    compare(
        super::RecordingIngestBatchState::Durable,
        RecordingIngestBatchState::Durable,
        "durable",
    );
    compare(
        super::RecordingIngestBatchState::Materialized,
        RecordingIngestBatchState::Materialized,
        "materialized",
    );
    assert_eq!(super::AgentState::kind_of(), AgentState::kind_of());
    compare(super::AgentState::Idle, AgentState::Idle, "idle");
    compare(super::AgentState::Running, AgentState::Running, "running");
    compare(super::AgentState::Waiting, AgentState::Waiting, "waiting");
    compare(
        super::AgentState::Disabled,
        AgentState::Disabled,
        "disabled",
    );
    compare(super::AgentState::Failed, AgentState::Failed, "failed");
    assert_eq!(super::WakeKind::kind_of(), WakeKind::kind_of());
    compare(
        super::WakeKind::TaskResult,
        WakeKind::TaskResult,
        "task_result",
    );
    compare(
        super::WakeKind::ResourceChanged,
        WakeKind::ResourceChanged,
        "resource_changed",
    );
    compare(super::WakeKind::Timer, WakeKind::Timer, "timer");
    compare(
        super::WakeKind::OperatorMessage,
        WakeKind::OperatorMessage,
        "operator_message",
    );
    compare(
        super::WakeKind::InputRequest,
        WakeKind::InputRequest,
        "input_request",
    );
    assert_eq!(super::WakeState::kind_of(), WakeState::kind_of());
    compare(super::WakeState::Pending, WakeState::Pending, "pending");
    compare(super::WakeState::Claimed, WakeState::Claimed, "claimed");
    compare(super::WakeState::Acked, WakeState::Acked, "acked");
    compare(
        super::WakeState::Coalesced,
        WakeState::Coalesced,
        "coalesced",
    );
    compare(super::WakeState::Failed, WakeState::Failed, "failed");
    assert_eq!(
        super::AgentEpisodeState::kind_of(),
        AgentEpisodeState::kind_of()
    );
    compare(
        super::AgentEpisodeState::Running,
        AgentEpisodeState::Running,
        "running",
    );
    compare(
        super::AgentEpisodeState::Completed,
        AgentEpisodeState::Completed,
        "completed",
    );
    compare(
        super::AgentEpisodeState::BudgetTerminated,
        AgentEpisodeState::BudgetTerminated,
        "budget_terminated",
    );
    compare(
        super::AgentEpisodeState::Stopped,
        AgentEpisodeState::Stopped,
        "stopped",
    );
    compare(
        super::AgentEpisodeState::Failed,
        AgentEpisodeState::Failed,
        "failed",
    );
    compare(
        super::AgentEpisodeState::Crashed,
        AgentEpisodeState::Crashed,
        "crashed",
    );
    assert_eq!(
        super::AgentTaskWatchState::kind_of(),
        AgentTaskWatchState::kind_of()
    );
    compare(
        super::AgentTaskWatchState::Pending,
        AgentTaskWatchState::Pending,
        "pending",
    );
    compare(
        super::AgentTaskWatchState::Watching,
        AgentTaskWatchState::Watching,
        "watching",
    );
    compare(
        super::AgentTaskWatchState::Resolved,
        AgentTaskWatchState::Resolved,
        "resolved",
    );
    compare(
        super::AgentTaskWatchState::Failed,
        AgentTaskWatchState::Failed,
        "failed",
    );
    compare(
        super::AgentTaskWatchState::Cancelled,
        AgentTaskWatchState::Cancelled,
        "cancelled",
    );
    assert_eq!(
        super::AgentInputRequestState::kind_of(),
        AgentInputRequestState::kind_of()
    );
    compare(
        super::AgentInputRequestState::Pending,
        AgentInputRequestState::Pending,
        "pending",
    );
    compare(
        super::AgentInputRequestState::Answered,
        AgentInputRequestState::Answered,
        "answered",
    );
    compare(
        super::AgentInputRequestState::Declined,
        AgentInputRequestState::Declined,
        "declined",
    );
    compare(
        super::AgentInputRequestState::Cancelled,
        AgentInputRequestState::Cancelled,
        "cancelled",
    );
    assert_eq!(
        super::GatewayReplayKind::kind_of(),
        GatewayReplayKind::kind_of()
    );
    compare(
        super::GatewayReplayKind::ClientAssertion,
        GatewayReplayKind::ClientAssertion,
        "client_assertion",
    );
    compare(
        super::GatewayReplayKind::IdJag,
        GatewayReplayKind::IdJag,
        "id_jag",
    );
}
