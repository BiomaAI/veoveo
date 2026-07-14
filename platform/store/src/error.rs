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
    #[error("schema migration requires root-scoped SurrealDB credentials")]
    MigrationRequiresRootCredentials,
    #[error("{field} must be greater than zero")]
    ZeroValue { field: &'static str },
    #[error("max WebSocket write buffer must be larger than the write buffer")]
    InvalidWriteBuffer,
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum MigrationError {
    #[error("migration catalog is empty")]
    EmptyCatalog,
    #[error("migration versions must be contiguous from 0; expected {expected}, found {actual}")]
    NonContiguous { expected: u32, actual: u32 },
    #[error("migration {version} has an empty name or SQL body")]
    EmptyMigration { version: u32 },
    #[error("database has unknown migration version {version}")]
    DatabaseAhead { version: u32 },
    #[error("migration history has a gap before version {version}")]
    HistoryGap { version: u32 },
    #[error("migration {version} differs from the compiled catalog")]
    Drift { version: u32 },
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error(transparent)]
    Config(#[from] StoreConfigError),
    #[error(transparent)]
    Migration(#[from] MigrationError),
    #[error("SurrealDB operation failed: {0}")]
    Database(#[from] surrealdb::Error),
    #[error("{operation} requires root-scoped SurrealDB credentials")]
    RootCredentialsRequired { operation: &'static str },
    #[error("SurrealDB administration failed during {operation}; details are redacted")]
    AdministrationFailed { operation: &'static str },
    #[error("changefeed limit must be in 1..={max}")]
    InvalidChangefeedLimit { max: u32 },
    #[error("outbox page limit must be in 1..={max}")]
    InvalidOutboxLimit { max: u32 },
    #[error("SurrealDB returned no record for {operation}")]
    MissingRecord { operation: &'static str },
    #[error("invalid platform identity field {field}: {reason}")]
    InvalidIdentityField {
        field: &'static str,
        reason: &'static str,
    },
    #[error("existing {entity} identity conflicts with canonical key {key}")]
    IdentityConflict { entity: &'static str, key: String },
    #[error("invalid recording field {field}: {reason}")]
    InvalidRecordingField {
        field: &'static str,
        reason: &'static str,
    },
    #[error("recording `{0}` was not found")]
    RecordingNotFound(String),
    #[error("recording `{recording_id}` cannot transition from {state} to {target}")]
    RecordingStateConflict {
        recording_id: String,
        state: String,
        target: &'static str,
    },
    #[error("segment `{segment_id}` conflicts with its existing immutable identity")]
    SegmentConflict { segment_id: String },
    #[error("invalid domain usage field {field}: {reason}")]
    InvalidUsageField {
        field: &'static str,
        reason: &'static str,
    },
    #[error("invalid coordinate field {field}: {reason}")]
    InvalidCoordinateField {
        field: &'static str,
        reason: &'static str,
    },
    #[error("coordinate frame `{0}` already exists in this tenant")]
    CoordinateFrameConflict(String),
    #[error("coordinate operation `{0}` conflicts with its durable provenance")]
    CoordinateOperationConflict(String),
    #[error("invalid map field {field}: {reason}")]
    InvalidMapField {
        field: &'static str,
        reason: &'static str,
    },
    #[error("map {entity} `{key}` conflicts with the current durable record")]
    MapRecordConflict { entity: &'static str, key: String },
    #[error("invalid time field {field}: {reason}")]
    InvalidTimeField {
        field: &'static str,
        reason: &'static str,
    },
    #[error("time {entity} `{key}` conflicts with the current durable record")]
    TimeRecordConflict { entity: &'static str, key: String },
    #[error("task `{0}` was not found")]
    TaskNotFound(String),
    #[error("task `{task_id}` does not belong to MCP server `{server}`")]
    TaskServerMismatch { task_id: String, server: String },
    #[error("artifact write capability redemption was denied")]
    ArtifactWriteDenied,
    #[error("artifact write idempotency key `{key}` was reused for a different request")]
    ArtifactWriteConflict { key: String },
    #[error("invalid gateway refresh-token transition: {reason}")]
    InvalidGatewayRefreshTransition { reason: &'static str },
}
