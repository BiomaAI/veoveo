use thiserror::Error;

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum StoreConfigError {
    #[error("SurrealDB endpoint must use ws or wss, got {0}")]
    UnsupportedEndpointScheme(String),
    #[error("SurrealDB endpoint must include a host")]
    MissingEndpointHost,
    #[error("SurrealDB endpoint must not include credentials, query parameters, or a fragment")]
    UnsafeEndpoint,
    #[error("invalid SurrealDB endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("{field} must be 1-64 ASCII letters, digits, underscores, or hyphens")]
    InvalidName { field: &'static str },
    #[error("SurrealDB username must not be empty")]
    EmptyUsername,
    #[error("SurrealDB password must not be empty")]
    EmptyPassword,
    #[error("VEOVEO_SURREAL_AUTH_LEVEL must be root, namespace, or database, got {0}")]
    InvalidAuthLevel(String),
    #[error("{field} must be greater than zero")]
    ZeroValue { field: &'static str },
    #[error("max WebSocket write buffer must be larger than the write buffer")]
    InvalidWriteBuffer,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(
        "database uses the mixed schema catalog; create a fresh installation with selected module lanes before starting this runtime"
    )]
    FreshInstallationRequired,

    #[error(transparent)]
    AuditTarget(#[from] veoveo_audit_contract::AuditTargetError),
    #[error("knowledge persistence rejected the operation: {0}")]
    Knowledge(&'static str),
    #[error("native changefeed LIVE connection exceeded 15 seconds")]
    ChangefeedConnectionTimeout,
    #[error(transparent)]
    AuditValidation(#[from] veoveo_audit_contract::AuditValidationError),
    #[error("audit partition access denied")]
    AuditAccessDenied,
    #[error("audit row identity or document is inconsistent")]
    AuditIntegrity,
    #[error("audit batch exceeds the limit for this operation")]
    AuditBatchLimit,
    #[error("another replica holds the audit maintenance lease")]
    AuditLeaseBusy,
    #[error("audit maintenance lease ownership was lost")]
    AuditLeaseLost,
    #[error("audit change-feed recovery exceeded its six-day safety window")]
    AuditChangefeedGap,
    #[error("audit retention conditions changed before commit")]
    AuditRetentionNotAdmitted,
    #[error(
        "audit destination permanently rejected this delivery; configure a corrected destination before resuming export"
    )]
    AuditExportRejected,
    #[error("the active gateway control-plane pointer and revision do not agree")]
    InvalidGatewayControlRevision,
    #[error("artifact upload rejected: {0:?}")]
    ArtifactUpload(crate::ArtifactUploadRejection),
    #[error("artifact digest is already registered with different immutable content metadata")]
    ArtifactBlobIntegrityConflict,
    #[error(transparent)]
    Config(#[from] StoreConfigError),
    #[error("SurrealDB operation failed: {0}")]
    Database(#[from] surrealdb::Error),
    #[error("{operation} requires root-scoped SurrealDB credentials")]
    RootCredentialsRequired { operation: &'static str },
    #[error("SurrealDB administration failed during {operation}; details are redacted")]
    AdministrationFailed { operation: &'static str },
    #[error("changefeed limit must be in 1..={max}")]
    InvalidChangefeedLimit { max: u32 },
    #[error("changefeed entry could not be decoded: {reason}")]
    InvalidChangefeedEntry { reason: &'static str },
    #[error("change-feed consumer identity must contain 1–256 ASCII identifier characters")]
    InvalidChangefeedConsumer,
    #[error("SurrealDB returned no record for {operation}")]
    MissingRecord { operation: &'static str },
    #[error("invalid platform identity field {field}: {reason}")]
    InvalidIdentityField {
        field: &'static str,
        reason: &'static str,
    },
    #[error("existing {entity} identity conflicts with canonical key {key}")]
    IdentityConflict { entity: &'static str, key: String },
    #[error("invalid domain usage field {field}: {reason}")]
    InvalidUsageField {
        field: &'static str,
        reason: &'static str,
    },
    #[error("task `{0}` was not found")]
    TaskNotFound(String),
    #[error("task `{task_id}` does not belong to MCP server `{server}`")]
    TaskServerMismatch { task_id: String, server: String },
    #[error("invalid gateway task route: {reason}")]
    InvalidGatewayTaskRoute { reason: String },
    #[error("artifact write capability redemption was denied")]
    ArtifactWriteDenied,
    #[error("artifact write idempotency key `{key}` was reused for a different request")]
    ArtifactWriteConflict { key: String },
    #[error("invalid artifact access request field {field}: {reason}")]
    InvalidArtifactAccessRequest {
        field: &'static str,
        reason: &'static str,
    },
    #[error("artifact access request `{0}` conflicts with its current state")]
    ArtifactAccessRequestConflict(String),
    #[error("invalid gateway refresh-token transition: {reason}")]
    InvalidGatewayRefreshTransition { reason: &'static str },
}
