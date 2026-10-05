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
