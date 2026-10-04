//! Frozen vocabulary formats captured from the declarations before derive migration.
mod baseline {
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum AuditClass {
        ApiActivity,
        Authentication,
        AccountChange,
        ArtifactActivity,
        LiveViewAccess,
        ComputerActivity,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum AuditOutcome {
        Allowed,
        Denied,
        Succeeded,
        Failed,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum AuditReason {
        Accepted,
        PolicyDenied,
        TenantMismatch,
        InsufficientClearance,
        InsufficientAccess,
        Unauthenticated,
        InvalidCredential,
        ExpiredCredential,
        Replay,
        Revoked,
        InvalidRequest,
        NotFound,
        Conflict,
        Unavailable,
        QuotaExceeded,
        Cancelled,
        TimedOut,
        UpstreamFailure,
        InternalFailure,
        UnknownProfile,
        UnknownServer,
        UnknownTool,
        UnknownResource,
        UnknownPrompt,
        UnknownTask,
        UnknownArtifact,
        UnknownPrincipal,
        UnknownScope,
        UnknownDataLabel,
        UnknownTenant,
        UnknownTokenIssuer,
        MissingPrincipal,
        MissingTenant,
        MissingGroup,
        MissingRole,
        MissingScope,
        MissingDataLabel,
        MissingPrincipalAssurance,
        TokenAudienceMismatch,
        TokenNotYetValid,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum AuditReadMethod {
        ResourceRead,
        PromptGet,
        Completion,
        Subscription,
        Status,
        Usage,
        AuditView,
        AuditExport,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum DiscoveryKind {
        Tools,
        Resources,
        ResourceTemplates,
        Prompts,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum ToolResultKind {
        Complete,
        ErrorResult,
        InputRequired,
        TaskCreated,
        OtherResponse,
        ProtocolError,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum AuthenticationActivity {
        Issue,
        Refresh,
        DuplicateRefresh,
        Revoke,
        Replay,
        CredentialDenial,
        Login,
        Logout,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum AdministrativeOperation {
        ControlPlane,
        JwtRevoke,
        JwtPrune,
        AgentMessage,
        AgentConversation,
        AgentInputRequests,
        AgentInputDecision,
        AgentDefinitionsRead,
        AgentDefinitionsReadContent,
        AgentDefinitionsCreate,
        AgentDefinitionsEdit,
        AgentDefinitionsPublish,
        AgentDefinitionsUse,
        AgentDefinitionsControl,
        AgentDefinitionsArchive,
        AgentDefinitionsTransfer,
        AgentInstancesDeploy,
        AgentInstancesControl,
        ArtifactRelease,
        ArtifactGrant,
        ArtifactRevoke,
        ArtifactShare,
        ArtifactUnshare,
        ArtifactAccessRequestCreate,
        ArtifactAccessRequestList,
        ArtifactAccessRequestDecide,
        ArtifactAccessRequestCancel,
        ConsoleCluster,
        ConsoleSnapshot,
        ConsoleStream,
        ConsoleArtifact,
        ServerHealth,
        ServerProxy,
        TaskCancel,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum AdministrativeAccess {
        Read,
        Write,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum AdminOperationFailure {
        AgentManagement,
        AgentConversation,
        AgentInputRequest,
        AgentMessage,
        ArtifactGrant,
        ArtifactGrantRevoke,
        ArtifactReleaseState,
        ArtifactShareLink,
        ArtifactShareLinkRevoke,
        BuildHttpClient,
        CancelTask,
        ControlPlaneSha,
        ExpiredRevocation,
        InvalidControlPlane,
        IssueInternalToken,
        LatestRevisionRead,
        PersistControlPlaneRevision,
        PersistJwtRevocation,
        PruneJwtRevocations,
        RevisionId,
        ServerAdminProxy,
        TaskOwnership,
        TaskRoute,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum AccountActivity {
        Create,
        Update,
        Delete,
        MembershipChange,
        Grant,
        Revoke,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum ArtifactActivity {
        Publish,
        Download,
        Grant,
        Revoke,
        Share,
        Unshare,
        Delete,
        Inspect,
        GrantsRead,
        Release,
        AccessRequestCreate,
        AccessRequestRead,
        AccessRequestDecide,
        AccessRequestCancel,
        ReadCapabilityIssue,
        ReadCapabilityRevoke,
        WriteCapabilityIssue,
        WriteCapabilityRedeem,
        ShareRedeem,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum LiveViewActivity {
        Issue,
        Renew,
        Close,
        Expire,
        Revoke,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum ComputerActivity {
        Create,
        Start,
        Stop,
        Delete,
        Attach,
        Grant,
        Renew,
        Revoke,
        Command,
        FileTransfer,
        Maintain,
        Close,
        ObserveRestart,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum ComputerAuditStage {
        Reserved,
        Queued,
        Dispatched,
        Observed,
        Settled,
        Aborted,
        GrantIssued,
        GrantRenewed,
        GrantRevoked,
        Attached,
        Closed,
        Terminated,
        ContainmentRequested,
        StopDispatched,
        MaintenanceProgress,
        Resumed,
        RecoveryRequired,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum TaskActivity {
        Update,
        Cancel,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum RecordingActivity {
        StreamOpen,
        StreamStatus,
        AppendDenied,
        BlueprintPublish,
        Finish,
        LayerPublish,
        PlaybackGrant,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum DictationEnd {
        Completed,
        Cancelled,
        TimedOut,
        Disconnected,
        Failed,
    }
    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
    )]
    #[serde(rename_all = "snake_case")]
    pub enum KnowledgeReadStatus {
        Read,
        NotModified,
        Missing,
        Denied,
        Failed,
    }
}
fn compare<
    A: serde::Serialize + for<'de> serde::Deserialize<'de> + PartialEq + std::fmt::Debug,
    B: serde::Serialize,
>(
    actual: A,
    before: B,
) {
    let wire = serde_json::to_value(&before).unwrap();
    assert_eq!(serde_json::to_value(&actual).unwrap(), wire);
    assert_eq!(serde_json::from_value::<A>(wire).unwrap(), actual);
    let token = before.serialize(UnitVariantProbe).unwrap();
    assert_eq!(actual.serialize(UnitVariantProbe).unwrap(), token);
    let decoded = A::deserialize(serde::de::value::EnumAccessDeserializer::new(Ordinal(
        token.1,
    )))
    .unwrap();
    assert_eq!(decoded, actual);
}
#[test]
fn frozen_activity_vocabulary_wire_and_schemas() {
    compare(
        super::AuditClass::ApiActivity,
        baseline::AuditClass::ApiActivity,
    );
    compare(
        super::AuditClass::Authentication,
        baseline::AuditClass::Authentication,
    );
    compare(
        super::AuditClass::AccountChange,
        baseline::AuditClass::AccountChange,
    );
    compare(
        super::AuditClass::ArtifactActivity,
        baseline::AuditClass::ArtifactActivity,
    );
    compare(
        super::AuditClass::LiveViewAccess,
        baseline::AuditClass::LiveViewAccess,
    );
    compare(
        super::AuditClass::ComputerActivity,
        baseline::AuditClass::ComputerActivity,
    );
    assert_eq!(
        schemars::schema_for!(super::AuditClass),
        schemars::schema_for!(baseline::AuditClass)
    );
    compare(
        super::AuditOutcome::Allowed,
        baseline::AuditOutcome::Allowed,
    );
    compare(super::AuditOutcome::Denied, baseline::AuditOutcome::Denied);
    compare(
        super::AuditOutcome::Succeeded,
        baseline::AuditOutcome::Succeeded,
    );
    compare(super::AuditOutcome::Failed, baseline::AuditOutcome::Failed);
    assert_eq!(
        schemars::schema_for!(super::AuditOutcome),
        schemars::schema_for!(baseline::AuditOutcome)
    );
    compare(
        super::AuditReason::Accepted,
        baseline::AuditReason::Accepted,
    );
    compare(
        super::AuditReason::PolicyDenied,
        baseline::AuditReason::PolicyDenied,
    );
    compare(
        super::AuditReason::TenantMismatch,
        baseline::AuditReason::TenantMismatch,
    );
    compare(
        super::AuditReason::InsufficientClearance,
        baseline::AuditReason::InsufficientClearance,
    );
    compare(
        super::AuditReason::InsufficientAccess,
        baseline::AuditReason::InsufficientAccess,
    );
    compare(
        super::AuditReason::Unauthenticated,
        baseline::AuditReason::Unauthenticated,
    );
    compare(
        super::AuditReason::InvalidCredential,
        baseline::AuditReason::InvalidCredential,
    );
    compare(
        super::AuditReason::ExpiredCredential,
        baseline::AuditReason::ExpiredCredential,
    );
    compare(super::AuditReason::Replay, baseline::AuditReason::Replay);
    compare(super::AuditReason::Revoked, baseline::AuditReason::Revoked);
    compare(
        super::AuditReason::InvalidRequest,
        baseline::AuditReason::InvalidRequest,
    );
    compare(
        super::AuditReason::NotFound,
        baseline::AuditReason::NotFound,
    );
    compare(
        super::AuditReason::Conflict,
        baseline::AuditReason::Conflict,
    );
    compare(
        super::AuditReason::Unavailable,
        baseline::AuditReason::Unavailable,
    );
    compare(
        super::AuditReason::QuotaExceeded,
        baseline::AuditReason::QuotaExceeded,
    );
    compare(
        super::AuditReason::Cancelled,
        baseline::AuditReason::Cancelled,
    );
    compare(
        super::AuditReason::TimedOut,
        baseline::AuditReason::TimedOut,
    );
    compare(
        super::AuditReason::UpstreamFailure,
        baseline::AuditReason::UpstreamFailure,
    );
    compare(
        super::AuditReason::InternalFailure,
        baseline::AuditReason::InternalFailure,
    );
    compare(
        super::AuditReason::UnknownProfile,
        baseline::AuditReason::UnknownProfile,
    );
    compare(
        super::AuditReason::UnknownServer,
        baseline::AuditReason::UnknownServer,
    );
    compare(
        super::AuditReason::UnknownTool,
        baseline::AuditReason::UnknownTool,
    );
    compare(
        super::AuditReason::UnknownResource,
        baseline::AuditReason::UnknownResource,
    );
    compare(
        super::AuditReason::UnknownPrompt,
        baseline::AuditReason::UnknownPrompt,
    );
    compare(
        super::AuditReason::UnknownTask,
        baseline::AuditReason::UnknownTask,
    );
    compare(
        super::AuditReason::UnknownArtifact,
        baseline::AuditReason::UnknownArtifact,
    );
    compare(
        super::AuditReason::UnknownPrincipal,
        baseline::AuditReason::UnknownPrincipal,
    );
    compare(
        super::AuditReason::UnknownScope,
        baseline::AuditReason::UnknownScope,
    );
    compare(
        super::AuditReason::UnknownDataLabel,
        baseline::AuditReason::UnknownDataLabel,
    );
    compare(
        super::AuditReason::UnknownTenant,
        baseline::AuditReason::UnknownTenant,
    );
    compare(
        super::AuditReason::UnknownTokenIssuer,
        baseline::AuditReason::UnknownTokenIssuer,
    );
    compare(
        super::AuditReason::MissingPrincipal,
        baseline::AuditReason::MissingPrincipal,
    );
    compare(
        super::AuditReason::MissingTenant,
        baseline::AuditReason::MissingTenant,
    );
    compare(
        super::AuditReason::MissingGroup,
        baseline::AuditReason::MissingGroup,
    );
    compare(
        super::AuditReason::MissingRole,
        baseline::AuditReason::MissingRole,
    );
    compare(
        super::AuditReason::MissingScope,
        baseline::AuditReason::MissingScope,
    );
    compare(
        super::AuditReason::MissingDataLabel,
        baseline::AuditReason::MissingDataLabel,
    );
    compare(
        super::AuditReason::MissingPrincipalAssurance,
        baseline::AuditReason::MissingPrincipalAssurance,
    );
    compare(
        super::AuditReason::TokenAudienceMismatch,
        baseline::AuditReason::TokenAudienceMismatch,
    );
    compare(
        super::AuditReason::TokenNotYetValid,
        baseline::AuditReason::TokenNotYetValid,
    );
    assert_eq!(
        schemars::schema_for!(super::AuditReason),
        schemars::schema_for!(baseline::AuditReason)
    );
    compare(
        super::AuditReadMethod::ResourceRead,
        baseline::AuditReadMethod::ResourceRead,
    );
    compare(
        super::AuditReadMethod::PromptGet,
        baseline::AuditReadMethod::PromptGet,
    );
    compare(
        super::AuditReadMethod::Completion,
        baseline::AuditReadMethod::Completion,
    );
    compare(
        super::AuditReadMethod::Subscription,
        baseline::AuditReadMethod::Subscription,
    );
    compare(
        super::AuditReadMethod::Status,
        baseline::AuditReadMethod::Status,
    );
    compare(
        super::AuditReadMethod::Usage,
        baseline::AuditReadMethod::Usage,
    );
    compare(
        super::AuditReadMethod::AuditView,
        baseline::AuditReadMethod::AuditView,
    );
    compare(
        super::AuditReadMethod::AuditExport,
        baseline::AuditReadMethod::AuditExport,
    );
    assert_eq!(
        schemars::schema_for!(super::AuditReadMethod),
        schemars::schema_for!(baseline::AuditReadMethod)
    );
    compare(super::DiscoveryKind::Tools, baseline::DiscoveryKind::Tools);
    compare(
        super::DiscoveryKind::Resources,
        baseline::DiscoveryKind::Resources,
    );
    compare(
        super::DiscoveryKind::ResourceTemplates,
        baseline::DiscoveryKind::ResourceTemplates,
    );
    compare(
        super::DiscoveryKind::Prompts,
        baseline::DiscoveryKind::Prompts,
    );
    assert_eq!(
        schemars::schema_for!(super::DiscoveryKind),
        schemars::schema_for!(baseline::DiscoveryKind)
    );
    compare(
        super::ToolResultKind::Complete,
        baseline::ToolResultKind::Complete,
    );
    compare(
        super::ToolResultKind::ErrorResult,
        baseline::ToolResultKind::ErrorResult,
    );
    compare(
        super::ToolResultKind::InputRequired,
        baseline::ToolResultKind::InputRequired,
    );
    compare(
        super::ToolResultKind::TaskCreated,
        baseline::ToolResultKind::TaskCreated,
    );
    compare(
        super::ToolResultKind::OtherResponse,
        baseline::ToolResultKind::OtherResponse,
    );
    compare(
        super::ToolResultKind::ProtocolError,
        baseline::ToolResultKind::ProtocolError,
    );
    assert_eq!(
        schemars::schema_for!(super::ToolResultKind),
        schemars::schema_for!(baseline::ToolResultKind)
    );
    compare(
        super::AuthenticationActivity::Issue,
        baseline::AuthenticationActivity::Issue,
    );
    compare(
        super::AuthenticationActivity::Refresh,
        baseline::AuthenticationActivity::Refresh,
    );
    compare(
        super::AuthenticationActivity::DuplicateRefresh,
        baseline::AuthenticationActivity::DuplicateRefresh,
    );
    compare(
        super::AuthenticationActivity::Revoke,
        baseline::AuthenticationActivity::Revoke,
    );
    compare(
        super::AuthenticationActivity::Replay,
        baseline::AuthenticationActivity::Replay,
    );
    compare(
        super::AuthenticationActivity::CredentialDenial,
        baseline::AuthenticationActivity::CredentialDenial,
    );
    compare(
        super::AuthenticationActivity::Login,
        baseline::AuthenticationActivity::Login,
    );
    compare(
        super::AuthenticationActivity::Logout,
        baseline::AuthenticationActivity::Logout,
    );
    assert_eq!(
        schemars::schema_for!(super::AuthenticationActivity),
        schemars::schema_for!(baseline::AuthenticationActivity)
    );
    compare(
        super::AdministrativeOperation::ControlPlane,
        baseline::AdministrativeOperation::ControlPlane,
    );
    compare(
        super::AdministrativeOperation::JwtRevoke,
        baseline::AdministrativeOperation::JwtRevoke,
    );
    compare(
        super::AdministrativeOperation::JwtPrune,
        baseline::AdministrativeOperation::JwtPrune,
    );
    compare(
        super::AdministrativeOperation::AgentMessage,
        baseline::AdministrativeOperation::AgentMessage,
    );
    compare(
        super::AdministrativeOperation::AgentConversation,
        baseline::AdministrativeOperation::AgentConversation,
    );
    compare(
        super::AdministrativeOperation::AgentInputRequests,
        baseline::AdministrativeOperation::AgentInputRequests,
    );
    compare(
        super::AdministrativeOperation::AgentInputDecision,
        baseline::AdministrativeOperation::AgentInputDecision,
    );
    compare(
        super::AdministrativeOperation::AgentDefinitionsRead,
        baseline::AdministrativeOperation::AgentDefinitionsRead,
    );
    compare(
        super::AdministrativeOperation::AgentDefinitionsReadContent,
        baseline::AdministrativeOperation::AgentDefinitionsReadContent,
    );
    compare(
        super::AdministrativeOperation::AgentDefinitionsCreate,
        baseline::AdministrativeOperation::AgentDefinitionsCreate,
    );
    compare(
        super::AdministrativeOperation::AgentDefinitionsEdit,
        baseline::AdministrativeOperation::AgentDefinitionsEdit,
    );
    compare(
        super::AdministrativeOperation::AgentDefinitionsPublish,
        baseline::AdministrativeOperation::AgentDefinitionsPublish,
    );
    compare(
        super::AdministrativeOperation::AgentDefinitionsUse,
        baseline::AdministrativeOperation::AgentDefinitionsUse,
    );
    compare(
        super::AdministrativeOperation::AgentDefinitionsControl,
        baseline::AdministrativeOperation::AgentDefinitionsControl,
    );
    compare(
        super::AdministrativeOperation::AgentDefinitionsArchive,
        baseline::AdministrativeOperation::AgentDefinitionsArchive,
    );
    compare(
        super::AdministrativeOperation::AgentDefinitionsTransfer,
        baseline::AdministrativeOperation::AgentDefinitionsTransfer,
    );
    compare(
        super::AdministrativeOperation::AgentInstancesDeploy,
        baseline::AdministrativeOperation::AgentInstancesDeploy,
    );
    compare(
        super::AdministrativeOperation::AgentInstancesControl,
        baseline::AdministrativeOperation::AgentInstancesControl,
    );
    compare(
        super::AdministrativeOperation::ArtifactRelease,
        baseline::AdministrativeOperation::ArtifactRelease,
    );
    compare(
        super::AdministrativeOperation::ArtifactGrant,
        baseline::AdministrativeOperation::ArtifactGrant,
    );
    compare(
        super::AdministrativeOperation::ArtifactRevoke,
        baseline::AdministrativeOperation::ArtifactRevoke,
    );
    compare(
        super::AdministrativeOperation::ArtifactShare,
        baseline::AdministrativeOperation::ArtifactShare,
    );
    compare(
        super::AdministrativeOperation::ArtifactUnshare,
        baseline::AdministrativeOperation::ArtifactUnshare,
    );
    compare(
        super::AdministrativeOperation::ArtifactAccessRequestCreate,
        baseline::AdministrativeOperation::ArtifactAccessRequestCreate,
    );
    compare(
        super::AdministrativeOperation::ArtifactAccessRequestList,
        baseline::AdministrativeOperation::ArtifactAccessRequestList,
    );
    compare(
        super::AdministrativeOperation::ArtifactAccessRequestDecide,
        baseline::AdministrativeOperation::ArtifactAccessRequestDecide,
    );
    compare(
        super::AdministrativeOperation::ArtifactAccessRequestCancel,
        baseline::AdministrativeOperation::ArtifactAccessRequestCancel,
    );
    compare(
        super::AdministrativeOperation::ConsoleCluster,
        baseline::AdministrativeOperation::ConsoleCluster,
    );
    compare(
        super::AdministrativeOperation::ConsoleSnapshot,
        baseline::AdministrativeOperation::ConsoleSnapshot,
    );
    compare(
        super::AdministrativeOperation::ConsoleStream,
        baseline::AdministrativeOperation::ConsoleStream,
    );
    compare(
        super::AdministrativeOperation::ConsoleArtifact,
        baseline::AdministrativeOperation::ConsoleArtifact,
    );
    compare(
        super::AdministrativeOperation::ServerHealth,
        baseline::AdministrativeOperation::ServerHealth,
    );
    compare(
        super::AdministrativeOperation::ServerProxy,
        baseline::AdministrativeOperation::ServerProxy,
    );
    compare(
        super::AdministrativeOperation::TaskCancel,
        baseline::AdministrativeOperation::TaskCancel,
    );
    assert_eq!(
        schemars::schema_for!(super::AdministrativeOperation),
        schemars::schema_for!(baseline::AdministrativeOperation)
    );
    compare(
        super::AdministrativeAccess::Read,
        baseline::AdministrativeAccess::Read,
    );
    compare(
        super::AdministrativeAccess::Write,
        baseline::AdministrativeAccess::Write,
    );
    assert_eq!(
        schemars::schema_for!(super::AdministrativeAccess),
        schemars::schema_for!(baseline::AdministrativeAccess)
    );
    compare(
        super::AdminOperationFailure::AgentManagement,
        baseline::AdminOperationFailure::AgentManagement,
    );
    compare(
        super::AdminOperationFailure::AgentConversation,
        baseline::AdminOperationFailure::AgentConversation,
    );
    compare(
        super::AdminOperationFailure::AgentInputRequest,
        baseline::AdminOperationFailure::AgentInputRequest,
    );
    compare(
        super::AdminOperationFailure::AgentMessage,
        baseline::AdminOperationFailure::AgentMessage,
    );
    compare(
        super::AdminOperationFailure::ArtifactGrant,
        baseline::AdminOperationFailure::ArtifactGrant,
    );
    compare(
        super::AdminOperationFailure::ArtifactGrantRevoke,
        baseline::AdminOperationFailure::ArtifactGrantRevoke,
    );
    compare(
        super::AdminOperationFailure::ArtifactReleaseState,
        baseline::AdminOperationFailure::ArtifactReleaseState,
    );
    compare(
        super::AdminOperationFailure::ArtifactShareLink,
        baseline::AdminOperationFailure::ArtifactShareLink,
    );
    compare(
        super::AdminOperationFailure::ArtifactShareLinkRevoke,
        baseline::AdminOperationFailure::ArtifactShareLinkRevoke,
    );
    compare(
        super::AdminOperationFailure::BuildHttpClient,
        baseline::AdminOperationFailure::BuildHttpClient,
    );
    compare(
        super::AdminOperationFailure::CancelTask,
        baseline::AdminOperationFailure::CancelTask,
    );
    compare(
        super::AdminOperationFailure::ControlPlaneSha,
        baseline::AdminOperationFailure::ControlPlaneSha,
    );
    compare(
        super::AdminOperationFailure::ExpiredRevocation,
        baseline::AdminOperationFailure::ExpiredRevocation,
    );
    compare(
        super::AdminOperationFailure::InvalidControlPlane,
        baseline::AdminOperationFailure::InvalidControlPlane,
    );
    compare(
        super::AdminOperationFailure::IssueInternalToken,
        baseline::AdminOperationFailure::IssueInternalToken,
    );
    compare(
        super::AdminOperationFailure::LatestRevisionRead,
        baseline::AdminOperationFailure::LatestRevisionRead,
    );
    compare(
        super::AdminOperationFailure::PersistControlPlaneRevision,
        baseline::AdminOperationFailure::PersistControlPlaneRevision,
    );
    compare(
        super::AdminOperationFailure::PersistJwtRevocation,
        baseline::AdminOperationFailure::PersistJwtRevocation,
    );
    compare(
        super::AdminOperationFailure::PruneJwtRevocations,
        baseline::AdminOperationFailure::PruneJwtRevocations,
    );
    compare(
        super::AdminOperationFailure::RevisionId,
        baseline::AdminOperationFailure::RevisionId,
    );
    compare(
        super::AdminOperationFailure::ServerAdminProxy,
        baseline::AdminOperationFailure::ServerAdminProxy,
    );
    compare(
        super::AdminOperationFailure::TaskOwnership,
        baseline::AdminOperationFailure::TaskOwnership,
    );
    compare(
        super::AdminOperationFailure::TaskRoute,
        baseline::AdminOperationFailure::TaskRoute,
    );
    assert_eq!(
        schemars::schema_for!(super::AdminOperationFailure),
        schemars::schema_for!(baseline::AdminOperationFailure)
    );
    compare(
        super::AccountActivity::Create,
        baseline::AccountActivity::Create,
    );
    compare(
        super::AccountActivity::Update,
        baseline::AccountActivity::Update,
    );
    compare(
        super::AccountActivity::Delete,
        baseline::AccountActivity::Delete,
    );
    compare(
        super::AccountActivity::MembershipChange,
        baseline::AccountActivity::MembershipChange,
    );
    compare(
        super::AccountActivity::Grant,
        baseline::AccountActivity::Grant,
    );
    compare(
        super::AccountActivity::Revoke,
        baseline::AccountActivity::Revoke,
    );
    assert_eq!(
        schemars::schema_for!(super::AccountActivity),
        schemars::schema_for!(baseline::AccountActivity)
    );
    compare(
        super::ArtifactActivity::Publish,
        baseline::ArtifactActivity::Publish,
    );
    compare(
        super::ArtifactActivity::Download,
        baseline::ArtifactActivity::Download,
    );
    compare(
        super::ArtifactActivity::Grant,
        baseline::ArtifactActivity::Grant,
    );
    compare(
        super::ArtifactActivity::Revoke,
        baseline::ArtifactActivity::Revoke,
    );
    compare(
        super::ArtifactActivity::Share,
        baseline::ArtifactActivity::Share,
    );
    compare(
        super::ArtifactActivity::Unshare,
        baseline::ArtifactActivity::Unshare,
    );
    compare(
        super::ArtifactActivity::Delete,
        baseline::ArtifactActivity::Delete,
    );
    compare(
        super::ArtifactActivity::Inspect,
        baseline::ArtifactActivity::Inspect,
    );
    compare(
        super::ArtifactActivity::GrantsRead,
        baseline::ArtifactActivity::GrantsRead,
    );
    compare(
        super::ArtifactActivity::Release,
        baseline::ArtifactActivity::Release,
    );
    compare(
        super::ArtifactActivity::AccessRequestCreate,
        baseline::ArtifactActivity::AccessRequestCreate,
    );
    compare(
        super::ArtifactActivity::AccessRequestRead,
        baseline::ArtifactActivity::AccessRequestRead,
    );
    compare(
        super::ArtifactActivity::AccessRequestDecide,
        baseline::ArtifactActivity::AccessRequestDecide,
    );
    compare(
        super::ArtifactActivity::AccessRequestCancel,
        baseline::ArtifactActivity::AccessRequestCancel,
    );
    compare(
        super::ArtifactActivity::ReadCapabilityIssue,
        baseline::ArtifactActivity::ReadCapabilityIssue,
    );
    compare(
        super::ArtifactActivity::ReadCapabilityRevoke,
        baseline::ArtifactActivity::ReadCapabilityRevoke,
    );
    compare(
        super::ArtifactActivity::WriteCapabilityIssue,
        baseline::ArtifactActivity::WriteCapabilityIssue,
    );
    compare(
        super::ArtifactActivity::WriteCapabilityRedeem,
        baseline::ArtifactActivity::WriteCapabilityRedeem,
    );
    compare(
        super::ArtifactActivity::ShareRedeem,
        baseline::ArtifactActivity::ShareRedeem,
    );
    assert_eq!(
        schemars::schema_for!(super::ArtifactActivity),
        schemars::schema_for!(baseline::ArtifactActivity)
    );
    compare(
        super::LiveViewActivity::Issue,
        baseline::LiveViewActivity::Issue,
    );
    compare(
        super::LiveViewActivity::Renew,
        baseline::LiveViewActivity::Renew,
    );
    compare(
        super::LiveViewActivity::Close,
        baseline::LiveViewActivity::Close,
    );
    compare(
        super::LiveViewActivity::Expire,
        baseline::LiveViewActivity::Expire,
    );
    compare(
        super::LiveViewActivity::Revoke,
        baseline::LiveViewActivity::Revoke,
    );
    assert_eq!(
        schemars::schema_for!(super::LiveViewActivity),
        schemars::schema_for!(baseline::LiveViewActivity)
    );
    compare(
        super::ComputerActivity::Create,
        baseline::ComputerActivity::Create,
    );
    compare(
        super::ComputerActivity::Start,
        baseline::ComputerActivity::Start,
    );
    compare(
        super::ComputerActivity::Stop,
        baseline::ComputerActivity::Stop,
    );
    compare(
        super::ComputerActivity::Delete,
        baseline::ComputerActivity::Delete,
    );
    compare(
        super::ComputerActivity::Attach,
        baseline::ComputerActivity::Attach,
    );
    compare(
        super::ComputerActivity::Grant,
        baseline::ComputerActivity::Grant,
    );
    compare(
        super::ComputerActivity::Renew,
        baseline::ComputerActivity::Renew,
    );
    compare(
        super::ComputerActivity::Revoke,
        baseline::ComputerActivity::Revoke,
    );
    compare(
        super::ComputerActivity::Command,
        baseline::ComputerActivity::Command,
    );
    compare(
        super::ComputerActivity::FileTransfer,
        baseline::ComputerActivity::FileTransfer,
    );
    compare(
        super::ComputerActivity::Maintain,
        baseline::ComputerActivity::Maintain,
    );
    compare(
        super::ComputerActivity::Close,
        baseline::ComputerActivity::Close,
    );
    compare(
        super::ComputerActivity::ObserveRestart,
        baseline::ComputerActivity::ObserveRestart,
    );
    assert_eq!(
        schemars::schema_for!(super::ComputerActivity),
        schemars::schema_for!(baseline::ComputerActivity)
    );
    compare(
        super::ComputerAuditStage::Reserved,
        baseline::ComputerAuditStage::Reserved,
    );
    compare(
        super::ComputerAuditStage::Queued,
        baseline::ComputerAuditStage::Queued,
    );
    compare(
        super::ComputerAuditStage::Dispatched,
        baseline::ComputerAuditStage::Dispatched,
    );
    compare(
        super::ComputerAuditStage::Observed,
        baseline::ComputerAuditStage::Observed,
    );
    compare(
        super::ComputerAuditStage::Settled,
        baseline::ComputerAuditStage::Settled,
    );
    compare(
        super::ComputerAuditStage::Aborted,
        baseline::ComputerAuditStage::Aborted,
    );
    compare(
        super::ComputerAuditStage::GrantIssued,
        baseline::ComputerAuditStage::GrantIssued,
    );
    compare(
        super::ComputerAuditStage::GrantRenewed,
        baseline::ComputerAuditStage::GrantRenewed,
    );
    compare(
        super::ComputerAuditStage::GrantRevoked,
        baseline::ComputerAuditStage::GrantRevoked,
    );
    compare(
        super::ComputerAuditStage::Attached,
        baseline::ComputerAuditStage::Attached,
    );
    compare(
        super::ComputerAuditStage::Closed,
        baseline::ComputerAuditStage::Closed,
    );
    compare(
        super::ComputerAuditStage::Terminated,
        baseline::ComputerAuditStage::Terminated,
    );
    compare(
        super::ComputerAuditStage::ContainmentRequested,
        baseline::ComputerAuditStage::ContainmentRequested,
    );
    compare(
        super::ComputerAuditStage::StopDispatched,
        baseline::ComputerAuditStage::StopDispatched,
    );
    compare(
        super::ComputerAuditStage::MaintenanceProgress,
        baseline::ComputerAuditStage::MaintenanceProgress,
    );
    compare(
        super::ComputerAuditStage::Resumed,
        baseline::ComputerAuditStage::Resumed,
    );
    compare(
        super::ComputerAuditStage::RecoveryRequired,
        baseline::ComputerAuditStage::RecoveryRequired,
    );
    assert_eq!(
        schemars::schema_for!(super::ComputerAuditStage),
        schemars::schema_for!(baseline::ComputerAuditStage)
    );
    compare(super::TaskActivity::Update, baseline::TaskActivity::Update);
    compare(super::TaskActivity::Cancel, baseline::TaskActivity::Cancel);
    assert_eq!(
        schemars::schema_for!(super::TaskActivity),
        schemars::schema_for!(baseline::TaskActivity)
    );
    compare(
        super::RecordingActivity::StreamOpen,
        baseline::RecordingActivity::StreamOpen,
    );
    compare(
        super::RecordingActivity::StreamStatus,
        baseline::RecordingActivity::StreamStatus,
    );
    compare(
        super::RecordingActivity::AppendDenied,
        baseline::RecordingActivity::AppendDenied,
    );
    compare(
        super::RecordingActivity::BlueprintPublish,
        baseline::RecordingActivity::BlueprintPublish,
    );
    compare(
        super::RecordingActivity::Finish,
        baseline::RecordingActivity::Finish,
    );
    compare(
        super::RecordingActivity::LayerPublish,
        baseline::RecordingActivity::LayerPublish,
    );
    compare(
        super::RecordingActivity::PlaybackGrant,
        baseline::RecordingActivity::PlaybackGrant,
    );
    assert_eq!(
        schemars::schema_for!(super::RecordingActivity),
        schemars::schema_for!(baseline::RecordingActivity)
    );
    compare(
        super::DictationEnd::Completed,
        baseline::DictationEnd::Completed,
    );
    compare(
        super::DictationEnd::Cancelled,
        baseline::DictationEnd::Cancelled,
    );
    compare(
        super::DictationEnd::TimedOut,
        baseline::DictationEnd::TimedOut,
    );
    compare(
        super::DictationEnd::Disconnected,
        baseline::DictationEnd::Disconnected,
    );
    compare(super::DictationEnd::Failed, baseline::DictationEnd::Failed);
    assert_eq!(
        schemars::schema_for!(super::DictationEnd),
        schemars::schema_for!(baseline::DictationEnd)
    );
    compare(
        super::KnowledgeReadStatus::Read,
        baseline::KnowledgeReadStatus::Read,
    );
    compare(
        super::KnowledgeReadStatus::NotModified,
        baseline::KnowledgeReadStatus::NotModified,
    );
    compare(
        super::KnowledgeReadStatus::Missing,
        baseline::KnowledgeReadStatus::Missing,
    );
    compare(
        super::KnowledgeReadStatus::Denied,
        baseline::KnowledgeReadStatus::Denied,
    );
    compare(
        super::KnowledgeReadStatus::Failed,
        baseline::KnowledgeReadStatus::Failed,
    );
    assert_eq!(
        schemars::schema_for!(super::KnowledgeReadStatus),
        schemars::schema_for!(baseline::KnowledgeReadStatus)
    );
}

/// Observe Serde's nonhuman unit-variant identity and ordinal without another codec.
struct UnitVariantProbe;
type ProbeError = serde::de::value::Error;
type ProbeToken = (&'static str, u32, &'static str);
impl serde::Serializer for UnitVariantProbe {
    type Ok = ProbeToken;
    type Error = ProbeError;
    type SerializeSeq = serde::ser::Impossible<ProbeToken, ProbeError>;
    type SerializeTuple = serde::ser::Impossible<ProbeToken, ProbeError>;
    type SerializeTupleStruct = serde::ser::Impossible<ProbeToken, ProbeError>;
    type SerializeTupleVariant = serde::ser::Impossible<ProbeToken, ProbeError>;
    type SerializeMap = serde::ser::Impossible<ProbeToken, ProbeError>;
    type SerializeStruct = serde::ser::Impossible<ProbeToken, ProbeError>;
    type SerializeStructVariant = serde::ser::Impossible<ProbeToken, ProbeError>;
    fn is_human_readable(&self) -> bool {
        false
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Ok((name, index, variant))
    }
    fn serialize_bool(self, _: bool) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_i8(self, _: i8) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_i16(self, _: i16) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_i32(self, _: i32) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_i64(self, _: i64) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_i128(self, _: i128) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_u8(self, _: u8) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_u16(self, _: u16) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_u32(self, _: u32) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_u64(self, _: u64) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_u128(self, _: u128) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_f32(self, _: f32) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_f64(self, _: f64) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_char(self, _: char) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_str(self, _: &str) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_some<T: serde::Serialize + ?Sized>(self, _: &T) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_newtype_struct<T: serde::Serialize + ?Sized>(
        self,
        _: &'static str,
        _: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
    fn serialize_newtype_variant<T: serde::Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        _: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Err(serde::ser::Error::custom("expected unit variant"))
    }
}
struct Ordinal(u32);
impl<'de> serde::de::EnumAccess<'de> for Ordinal {
    type Error = ProbeError;
    type Variant = Self;
    fn variant_seed<V: serde::de::DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self), Self::Error> {
        let value =
            seed.deserialize(serde::de::value::U32Deserializer::<ProbeError>::new(self.0))?;
        Ok((value, self))
    }
}
impl<'de> serde::de::VariantAccess<'de> for Ordinal {
    type Error = ProbeError;
    fn unit_variant(self) -> Result<(), Self::Error> {
        Ok(())
    }
    fn newtype_variant_seed<T: serde::de::DeserializeSeed<'de>>(
        self,
        _: T,
    ) -> Result<T::Value, Self::Error> {
        Err(serde::de::Error::custom("expected unit variant"))
    }
    fn tuple_variant<V: serde::de::Visitor<'de>>(
        self,
        _: usize,
        _: V,
    ) -> Result<V::Value, Self::Error> {
        Err(serde::de::Error::custom("expected unit variant"))
    }
    fn struct_variant<V: serde::de::Visitor<'de>>(
        self,
        _: &'static [&'static str],
        _: V,
    ) -> Result<V::Value, Self::Error> {
        Err(serde::de::Error::custom("expected unit variant"))
    }
}
