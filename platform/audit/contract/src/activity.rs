use crate::KnowledgeReadObservation;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{ResourceUri, Sha256Digest};

macro_rules! vocabulary {
 ($name:ident {$($variant:ident),+ $(,)?})=>{
  #[derive(Debug,Clone,Copy,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize,JsonSchema)]
  #[serde(rename_all="snake_case")]
  pub enum $name {$($variant),+}
 };
}
vocabulary!(AuditClass {
    ApiActivity,
    Authentication,
    AccountChange,
    ArtifactActivity,
    LiveViewAccess,
    ComputerActivity
});
vocabulary!(AuditOutcome {
    Allowed,
    Denied,
    Succeeded,
    Failed
});
vocabulary!(AuditReason {
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
    TokenNotYetValid
});
vocabulary!(AuditReadMethod {
    ResourceRead,
    PromptGet,
    Completion,
    Subscription,
    Status,
    Usage,
    AuditView,
    AuditExport
});
vocabulary!(DiscoveryKind {
    Tools,
    Resources,
    ResourceTemplates,
    Prompts
});
vocabulary!(ToolResultKind {
    Complete,
    ErrorResult,
    InputRequired,
    TaskCreated,
    OtherResponse,
    ProtocolError
});
vocabulary!(AuthenticationActivity {
    Issue,
    Refresh,
    DuplicateRefresh,
    Revoke,
    Replay,
    CredentialDenial,
    Login,
    Logout
});

vocabulary!(AdministrativeOperation {
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
    ServerProxy,
    TaskCancel
});
vocabulary!(AdministrativeAccess { Read, Write });
vocabulary!(AdminOperationFailure {
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
});

vocabulary!(AccountActivity {
    Create,
    Update,
    Delete,
    MembershipChange,
    Grant,
    Revoke
});
vocabulary!(ArtifactActivity {
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
    ShareRedeem
});
vocabulary!(LiveViewActivity {
    Issue,
    Renew,
    Close,
    Expire,
    Revoke
});
vocabulary!(ComputerActivity {
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
    ObserveRestart
});
vocabulary!(ComputerAuditStage {
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
    RecoveryRequired
});
vocabulary!(TaskActivity { Update, Cancel });
vocabulary!(RecordingActivity {
    StreamOpen,
    StreamStatus,
    AppendDenied,
    BlueprintPublish,
    Finish,
    LayerPublish,
    PlaybackGrant
});
vocabulary!(DictationEnd {
    Completed,
    Cancelled,
    TimedOut,
    Disconnected,
    Failed
});
vocabulary!(KnowledgeReadStatus {
    Read,
    NotModified,
    Missing,
    Denied,
    Failed
});

/// Closed payloads contain identifiers, measurements and reviewed enums only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuditDetail {
    AdminAdmission {
        operation: AdministrativeOperation,
        access: AdministrativeAccess,
    },
    AdminCompletion {
        operation: AdministrativeOperation,
        access: AdministrativeAccess,
        failure: Option<AdminOperationFailure>,
    },
    Read {
        method: AuditReadMethod,
    },
    Discovery {
        collection: DiscoveryKind,
        visible: u32,
        denied: u32,
        visible_digest: Sha256Digest,
    },
    Task {
        activity: TaskActivity,
    },
    ToolAdmission,
    ToolCompletion {
        result: ToolResultKind,
        duration_ms: u64,
        error_code: Option<i32>,
    },
    Authentication {
        activity: AuthenticationActivity,
        method: veoveo_types::AuthMethod,
        reason: veoveo_types::AuthReasonCode,
    },
    AccountChange {
        activity: AccountActivity,
    },
    Artifact {
        activity: ArtifactActivity,
        requested: Option<veoveo_types::AccessLevel>,
        subject: Option<veoveo_types::AccessSubject>,
        release_state: Option<veoveo_artifact_contract::ArtifactReleaseState>,
        related: Option<ResourceUri>,
        bytes: Option<u64>,
        window_start: Option<chrono::DateTime<chrono::Utc>>,
    },
    LiveView {
        activity: LiveViewActivity,
    },
    Computer {
        activity: ComputerActivity,
        stage: ComputerAuditStage,
        task: Option<veoveo_types::TaskId>,
    },
    Recording {
        activity: RecordingActivity,
    },
    RecordingCatalogGrant {
        recordings: u32,
        selection_digest: Sha256Digest,
    },
    DictationOpen,
    DictationDenial,
    DictationSummary {
        end: DictationEnd,
        chunks: u64,
        duration_ms: u64,
    },
    KnowledgeRead {
        member: ResourceUri,
        observation: Option<Box<KnowledgeReadObservation>>,
        status: KnowledgeReadStatus,
    },
    IndexingWindow {
        reads: u64,
        denials: u64,
        members_digest: Sha256Digest,
    },
}
impl AuditDetail {
    pub fn class(&self) -> AuditClass {
        match self {
            Self::Authentication { .. } => AuditClass::Authentication,
            Self::AccountChange { .. } => AuditClass::AccountChange,
            Self::Artifact { .. } => AuditClass::ArtifactActivity,
            Self::LiveView { .. } => AuditClass::LiveViewAccess,
            Self::Computer { .. } => AuditClass::ComputerActivity,
            _ => AuditClass::ApiActivity,
        }
    }
    pub fn activity(&self) -> &'static str {
        match self {
            Self::AdminAdmission { .. } => "admin_admission",
            Self::AdminCompletion { .. } => "admin_completion",
            Self::Read { method } => match method {
                AuditReadMethod::ResourceRead => "resource_read",
                AuditReadMethod::PromptGet => "prompt_get",
                AuditReadMethod::Completion => "completion",
                AuditReadMethod::Subscription => "subscription",
                AuditReadMethod::Status => "status",
                AuditReadMethod::Usage => "usage",
                AuditReadMethod::AuditView => "audit_view",
                AuditReadMethod::AuditExport => "audit_export",
            },
            Self::Task { activity } => match activity {
                TaskActivity::Update => "task_update",
                TaskActivity::Cancel => "task_cancel",
            },
            Self::Discovery { .. } => "discovery",
            Self::ToolAdmission => "tool_admission",
            Self::ToolCompletion { .. } => "tool_completion",
            Self::Authentication { activity, .. } => match activity {
                AuthenticationActivity::Issue => "token_issue",
                AuthenticationActivity::Refresh => "token_refresh",
                AuthenticationActivity::DuplicateRefresh => "token_redelivery",
                AuthenticationActivity::Revoke => "token_revoke",
                AuthenticationActivity::Replay => "token_replay",
                AuthenticationActivity::CredentialDenial => "credential_denial",
                AuthenticationActivity::Login => "login",
                AuthenticationActivity::Logout => "logout",
            },
            Self::AccountChange { activity } => match activity {
                AccountActivity::Create => "account_create",
                AccountActivity::Update => "account_update",
                AccountActivity::Delete => "account_delete",
                AccountActivity::MembershipChange => "membership_change",
                AccountActivity::Grant => "account_grant",
                AccountActivity::Revoke => "account_revoke",
            },
            Self::Artifact { activity, .. } => match activity {
                ArtifactActivity::Publish => "artifact_publish",
                ArtifactActivity::Download => "artifact_download",
                ArtifactActivity::Grant => "artifact_grant",
                ArtifactActivity::Revoke => "artifact_revoke",
                ArtifactActivity::Share => "artifact_share",
                ArtifactActivity::Unshare => "artifact_unshare",
                ArtifactActivity::Delete => "artifact_delete",
                ArtifactActivity::Inspect => "artifact_inspect",
                ArtifactActivity::GrantsRead => "artifact_grants_read",
                ArtifactActivity::Release => "artifact_release",
                ArtifactActivity::AccessRequestCreate => "artifact_access_request_create",
                ArtifactActivity::AccessRequestRead => "artifact_access_request_read",
                ArtifactActivity::AccessRequestDecide => "artifact_access_request_decide",
                ArtifactActivity::AccessRequestCancel => "artifact_access_request_cancel",
                ArtifactActivity::ReadCapabilityIssue => "artifact_read_capability_issue",
                ArtifactActivity::ReadCapabilityRevoke => "artifact_read_capability_revoke",
                ArtifactActivity::WriteCapabilityIssue => "artifact_write_capability_issue",
                ArtifactActivity::WriteCapabilityRedeem => "artifact_write_capability_redeem",
                ArtifactActivity::ShareRedeem => "artifact_share_redeem",
            },
            Self::LiveView { activity } => match activity {
                LiveViewActivity::Issue => "live_view_issue",
                LiveViewActivity::Renew => "live_view_renew",
                LiveViewActivity::Close => "live_view_close",
                LiveViewActivity::Expire => "live_view_expire",
                LiveViewActivity::Revoke => "live_view_revoke",
            },
            Self::Computer { activity, .. } => match activity {
                ComputerActivity::Create => "computer_create",
                ComputerActivity::Start => "computer_start",
                ComputerActivity::Stop => "computer_stop",
                ComputerActivity::Delete => "computer_delete",
                ComputerActivity::Attach => "computer_attach",
                ComputerActivity::Grant => "computer_grant",
                ComputerActivity::Renew => "computer_renew",
                ComputerActivity::Revoke => "computer_revoke",
                ComputerActivity::Command => "computer_command",
                ComputerActivity::FileTransfer => "computer_file_transfer",
                ComputerActivity::Maintain => "computer_maintain",
                ComputerActivity::Close => "computer_close",
                ComputerActivity::ObserveRestart => "computer_restart_observed",
            },
            Self::Recording { activity } => match activity {
                RecordingActivity::StreamOpen => "recording_stream_open",
                RecordingActivity::StreamStatus => "recording_stream_status",
                RecordingActivity::AppendDenied => "recording_append_denied",
                RecordingActivity::BlueprintPublish => "recording_blueprint_publish",
                RecordingActivity::Finish => "recording_finish",
                RecordingActivity::LayerPublish => "recording_layer_publish",
                RecordingActivity::PlaybackGrant => "recording_playback_grant",
            },
            Self::RecordingCatalogGrant { .. } => "recording_catalog_grant",
            Self::DictationOpen => "dictation_open",
            Self::DictationDenial => "dictation_denial",
            Self::DictationSummary { .. } => "dictation_summary",
            Self::KnowledgeRead { .. } => "knowledge_read",
            Self::IndexingWindow { .. } => "indexing_window",
        }
    }
}
