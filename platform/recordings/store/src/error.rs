use thiserror::Error;
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RecordingIngestQuota {
    MaximumStreamBytes,
    MaximumConcurrentStreams,
    MaximumBatchesPerMinute,
    MaximumBytesPerDay,
    MaximumBlueprintBytes,
    MaximumBlueprintMessages,
    MaximumBlueprintRevisions,
}

impl std::fmt::Display for RecordingIngestQuota {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::MaximumStreamBytes => "maximum_stream_bytes",
            Self::MaximumConcurrentStreams => "maximum_concurrent_streams",
            Self::MaximumBatchesPerMinute => "maximum_batches_per_minute",
            Self::MaximumBytesPerDay => "maximum_bytes_per_day",
            Self::MaximumBlueprintBytes => "maximum_blueprint_bytes",
            Self::MaximumBlueprintMessages => "maximum_blueprint_messages",
            Self::MaximumBlueprintRevisions => "maximum_blueprint_revisions",
        })
    }
}

#[derive(Debug, Error)]
pub enum RecordingStoreError {
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
    #[error("recording dataset `{dataset_id}` conflicts with its durable identity")]
    RecordingDatasetConflict { dataset_id: String },
    #[error("recording layer `{layer_id}` conflicts with its durable identity")]
    RecordingLayerConflict { layer_id: String },
    #[error("recording read grant `{grant_id}` conflicts with its durable authority")]
    RecordingReadGrantConflict { grant_id: String },
    #[error("recording projection `{projection_id}` conflicts with its durable request")]
    RecordingProjectionConflict { projection_id: String },
    #[error("recording projection idempotency key conflicts with its authority or request")]
    RecordingProjectionRequestConflict,
    #[error("invalid recording ingest field {field}: {reason}")]
    InvalidRecordingIngestField {
        field: &'static str,
        reason: &'static str,
    },
    #[error("recording ingest stream `{0}` was not found")]
    RecordingIngestStreamNotFound(String),
    #[error("recording ingest stream `{stream_id}` is {state}")]
    RecordingIngestStreamStateConflict { stream_id: String, state: String },
    #[error("recording ingest stream `{0}` exceeded its open-stream retention window")]
    RecordingIngestStreamExpired(String),
    #[error("recording ingest stream expected sequence {expected}, received {actual}")]
    RecordingIngestSequenceGap { expected: u64, actual: u64 },
    #[error("recording ingest sequence {sequence} conflicts with its durable digest")]
    RecordingIngestDigestConflict { sequence: u64 },
    #[error("recording ingest checkpoint changed concurrently")]
    RecordingIngestCheckpointConflict,
    #[error(
        "recording Blueprint revision {revision} conflicts with its durable digest or identity"
    )]
    RecordingBlueprintRevisionConflict { revision: u64 },
    #[error("recording Blueprint expected revision {expected}, received {actual}")]
    RecordingBlueprintRevisionGap { expected: u64, actual: u64 },
    #[error("recording ingest producer exceeded the {quota} quota")]
    RecordingIngestQuotaExceeded { quota: RecordingIngestQuota },
    #[error("SurrealDB operation failed: {0}")]
    Database(#[from] surrealdb::Error),
    #[error(transparent)]
    Kernel(#[from] veoveo_platform_store::StoreError),
    #[error("SurrealDB returned no record for {operation}")]
    MissingRecord { operation: &'static str },
}
