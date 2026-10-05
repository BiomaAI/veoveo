use std::path::{Component, Path};
use std::str::FromStr as _;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};
use uuid::Uuid;

use crate::{
    RecordingDatasetId, RecordingDatasetRecord, RecordingId, RecordingLayerId, RecordingLayerKind,
    RecordingLayerRecord, RecordingLayerState, RecordingRepository, RecordingRetentionMode,
    RecordingState, RecordingStoreError,
};
use veoveo_platform_store::{ArtifactId, PlatformIdentity, TenantId};

#[path = "recording_catalog/access.rs"]
mod access;
#[path = "recording_catalog/grants.rs"]
mod grants;
#[path = "recording_catalog/projections.rs"]
mod projections;
pub use access::RecordingAccessScope;
pub use grants::{RecordingReadGrantDraft, RecordingReadGrantRequest};
pub use projections::{RecordingProjectionReceiptDraft, RecordingProjectionRequest};

const MAX_DATASET_KEY_BYTES: usize = 128;
const MAX_DISPLAY_LABEL_BYTES: usize = 256;
const MAX_LAYER_NAME_BYTES: usize = 256;
const MAX_LAYER_LIMIT: u32 = 10_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordingCatalogCleanup {
    pub projection_receipts: usize,
    pub read_grants: usize,
}

#[derive(Clone, Debug)]
pub struct RecordingDatasetDraft {
    pub identity: PlatformIdentity,
    pub dataset_key: String,
    pub display_label: String,
    pub default_blueprint_artifact_id: Option<ArtifactId>,
    pub retention_mode: RecordingRetentionMode,
    pub retention_expires_at: Option<DateTime<Utc>>,
}

impl RecordingDatasetDraft {
    pub fn installation_default(
        identity: PlatformIdentity,
        dataset_key: impl Into<String>,
    ) -> Self {
        let dataset_key = dataset_key.into();
        Self {
            identity,
            display_label: dataset_key.clone(),
            dataset_key,
            default_blueprint_artifact_id: None,
            retention_mode: RecordingRetentionMode::InstallationDefault,
            retention_expires_at: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecordingLayerDraft {
    pub identity: PlatformIdentity,
    pub recording_id: RecordingId,
    pub layer_name: String,
    pub kind: RecordingLayerKind,
    pub ordinal: Option<i64>,
    pub staging_path: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
}

impl RecordingLayerDraft {
    pub fn capture(
        identity: PlatformIdentity,
        recording_id: RecordingId,
        ordinal: i64,
        staging_path: String,
        start_time: Option<DateTime<Utc>>,
    ) -> Result<Self, RecordingStoreError> {
        let layer_name = capture_layer_name(ordinal)?;
        Ok(Self {
            identity,
            recording_id,
            layer_name,
            kind: RecordingLayerKind::Capture,
            ordinal: Some(ordinal),
            staging_path: Some(staging_path),
            start_time,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct RecordingDatasetContent {
    tenant: RecordId,
    dataset_key: String,
    display_label: String,
    default_blueprint_artifact: Option<RecordId>,
    retention_mode: RecordingRetentionMode,
    retention_expires_at: Option<DateTime<Utc>>,
    revision: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct RecordingLayerContent {
    tenant: RecordId,
    recording: RecordId,
    layer_name: String,
    kind: RecordingLayerKind,
    ordinal: Option<i64>,
    staging_path: Option<String>,
    artifact: Option<RecordId>,
    state: RecordingLayerState,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
    byte_len: i64,
    message_count: i64,
    sha256: Option<String>,
    rrd_version: Option<String>,
    schema_digest: Option<String>,
    failure_reason: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    revision: i64,
}

#[derive(Clone, Debug, Deserialize, SurrealValue)]
struct RecordingCatalogCleanupContent {
    projection_receipts: i64,
    read_grants: i64,
}

impl RecordingRepository {
    pub async fn ensure_recording_dataset(
        &self,
        draft: RecordingDatasetDraft,
    ) -> Result<RecordingDatasetRecord, RecordingStoreError> {
        validate_text("dataset_key", &draft.dataset_key, MAX_DATASET_KEY_BYTES)?;
        validate_text(
            "dataset display_label",
            &draft.display_label,
            MAX_DISPLAY_LABEL_BYTES,
        )?;
        validate_retention(draft.retention_mode, draft.retention_expires_at)?;
        if let Some(artifact_id) = draft.default_blueprint_artifact_id {
            let artifact = self.store.artifact_aggregate(artifact_id).await?.ok_or(
                RecordingStoreError::MissingRecord {
                    operation: "recording dataset default Blueprint artifact",
                },
            )?;
            if artifact.occurrence.tenant != draft.identity.tenant_id.record_id() {
                return Err(RecordingStoreError::RecordingDatasetConflict {
                    dataset_id: draft.dataset_key,
                });
            }
        }
        if let Some(existing) = self
            .recording_dataset_by_key(draft.identity.tenant_id, &draft.dataset_key)
            .await?
        {
            validate_existing_dataset(&existing, &draft)?;
            return Ok(existing);
        }

        let id = RecordingDatasetId::new();
        let now = Utc::now();
        let content = RecordingDatasetContent {
            tenant: draft.identity.tenant_id.record_id(),
            dataset_key: draft.dataset_key.clone(),
            display_label: draft.display_label.clone(),
            default_blueprint_artifact: draft
                .default_blueprint_artifact_id
                .map(ArtifactId::record_id),
            retention_mode: draft.retention_mode,
            retention_expires_at: draft.retention_expires_at,
            revision: 0,
            created_at: now,
            updated_at: now,
        };

        let result = self
            .client()
            .query(include_str!(
                "queries/recording_catalog/ensure_recording_dataset.surql"
            ))
            .bind(("dataset", id.record_id()))
            .bind(("content", content))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if let Some(existing) = self
                .recording_dataset_by_key(draft.identity.tenant_id, &draft.dataset_key)
                .await?
            {
                validate_existing_dataset(&existing, &draft)?;
                return Ok(existing);
            }
            return Err(error.into());
        }
        self.recording_dataset(draft.identity.tenant_id, id)
            .await?
            .ok_or(RecordingStoreError::MissingRecord {
                operation: "recording dataset creation readback",
            })
    }

    pub async fn recording_dataset(
        &self,
        tenant_id: TenantId,
        dataset_id: RecordingDatasetId,
    ) -> Result<Option<RecordingDatasetRecord>, RecordingStoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "queries/recording_catalog/recording_dataset.surql"
            ))
            .bind(("dataset", dataset_id.record_id()))
            .bind(("tenant", tenant_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn recording_dataset_by_key(
        &self,
        tenant_id: TenantId,
        dataset_key: &str,
    ) -> Result<Option<RecordingDatasetRecord>, RecordingStoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "queries/recording_catalog/recording_dataset_by_key.surql"
            ))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("dataset_key", dataset_key.to_owned()))
            .await?
            .check()?;
        let records: Vec<RecordingDatasetRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }

    pub async fn open_recording_layer(
        &self,
        draft: RecordingLayerDraft,
    ) -> Result<RecordingLayerRecord, RecordingStoreError> {
        validate_layer_draft(&draft)?;
        let mut recording = self
            .recording(draft.identity.tenant_id, draft.recording_id)
            .await?
            .ok_or_else(|| {
                RecordingStoreError::RecordingNotFound(draft.recording_id.to_string())
            })?;
        if draft.kind == RecordingLayerKind::Capture
            && matches!(
                recording.state,
                RecordingState::Ready | RecordingState::Interrupted
            )
        {
            recording = self
                .resume_recording(&draft.identity, draft.recording_id)
                .await?;
        }
        let allowed = match draft.kind {
            RecordingLayerKind::Capture => recording.state == RecordingState::Live,
            RecordingLayerKind::Properties => recording.state == RecordingState::Sealing,
            RecordingLayerKind::Derived => matches!(
                recording.state,
                RecordingState::Ready | RecordingState::Sealed | RecordingState::Interrupted
            ),
        };
        if !allowed {
            return Err(RecordingStoreError::RecordingStateConflict {
                recording_id: draft.recording_id.to_string(),
                state: format!("{:?}", recording.state).to_lowercase(),
                target: "open recording layer",
            });
        }
        if let Some(existing) = self
            .recording_layer_by_name(
                draft.identity.tenant_id,
                draft.recording_id,
                &draft.layer_name,
            )
            .await?
        {
            validate_existing_layer(&existing, &draft)?;
            return Ok(existing);
        }
        if let Some(path) = draft.staging_path.as_deref()
            && let Some(existing) = self
                .recording_layer_by_staging_path(draft.identity.tenant_id, path)
                .await?
        {
            validate_existing_layer(&existing, &draft)?;
            return Ok(existing);
        }

        let id = RecordingLayerId::new();
        let now = Utc::now();
        let content = RecordingLayerContent {
            tenant: draft.identity.tenant_id.record_id(),
            recording: draft.recording_id.record_id(),
            layer_name: draft.layer_name.clone(),
            kind: draft.kind,
            ordinal: draft.ordinal,
            staging_path: draft.staging_path.clone(),
            artifact: None,
            state: RecordingLayerState::Writing,
            start_time: draft.start_time,
            end_time: None,
            byte_len: 0,
            message_count: 0,
            sha256: None,
            rrd_version: None,
            schema_digest: None,
            failure_reason: None,
            created_at: now,
            updated_at: now,
            revision: 0,
        };

        let result = self
            .client()
            .query(include_str!(
                "queries/recording_catalog/open_recording_layer.surql"
            ))
            .bind(("layer", id.record_id()))
            .bind(("content", content))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if let Some(existing) = self
                .recording_layer_by_name(
                    draft.identity.tenant_id,
                    draft.recording_id,
                    &draft.layer_name,
                )
                .await?
            {
                validate_existing_layer(&existing, &draft)?;
                return Ok(existing);
            }
            return Err(error.into());
        }
        self.recording_layer(draft.identity.tenant_id, id)
            .await?
            .ok_or(RecordingStoreError::MissingRecord {
                operation: "recording layer creation readback",
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn stage_recording_layer(
        &self,
        identity: &PlatformIdentity,
        layer_id: RecordingLayerId,
        byte_len: i64,
        message_count: i64,
        sha256: &str,
        rrd_version: Option<&str>,
        schema_digest: Option<&str>,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<RecordingLayerRecord, RecordingStoreError> {
        if byte_len < 0 || message_count < 0 {
            return Err(RecordingStoreError::InvalidRecordingField {
                field: "recording layer metrics",
                reason: "must be non-negative",
            });
        }
        validate_sha256("sha256", sha256)?;
        if let Some(schema_digest) = schema_digest {
            validate_sha256("schema_digest", schema_digest)?;
        }
        if let Some(rrd_version) = rrd_version {
            validate_text("rrd_version", rrd_version, 64)?;
        }
        let existing = self
            .recording_layer(identity.tenant_id, layer_id)
            .await?
            .ok_or(RecordingStoreError::MissingRecord {
                operation: "recording layer stage",
            })?;
        if existing.state == RecordingLayerState::Staged
            && existing.byte_len == byte_len
            && existing.message_count == message_count
            && existing.sha256.as_deref() == Some(sha256)
            && existing.rrd_version.as_deref() == rrd_version
            && existing.schema_digest.as_deref() == schema_digest
        {
            return Ok(existing);
        }
        if existing.state != RecordingLayerState::Writing {
            return Err(RecordingStoreError::RecordingLayerConflict {
                layer_id: layer_id.to_string(),
            });
        }
        let recording_id = recording_id_from_record(&existing.recording)?;

        self.client()
            .query(include_str!(
                "queries/recording_catalog/stage_recording_layer.surql"
            ))
            .bind(("layer", layer_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("byte_len", byte_len))
            .bind(("message_count", message_count))
            .bind(("sha256", sha256.to_owned()))
            .bind(("rrd_version", rrd_version.map(str::to_owned)))
            .bind(("schema_digest", schema_digest.map(str::to_owned)))
            .bind(("end_time", end_time))
            .bind(("recording", recording_id.record_id()))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("activity_at", end_time.unwrap_or_else(Utc::now)))
            .await?
            .check()?;
        self.recording_layer(identity.tenant_id, layer_id)
            .await?
            .ok_or(RecordingStoreError::MissingRecord {
                operation: "recording layer stage readback",
            })
    }

    pub async fn commit_recording_layer(
        &self,
        identity: &PlatformIdentity,
        layer_id: RecordingLayerId,
        artifact_id: ArtifactId,
    ) -> Result<RecordingLayerRecord, RecordingStoreError> {
        let existing = self
            .recording_layer(identity.tenant_id, layer_id)
            .await?
            .ok_or(RecordingStoreError::MissingRecord {
                operation: "recording layer commit",
            })?;
        if existing.state == RecordingLayerState::Committed
            && existing.artifact == Some(artifact_id.record_id())
        {
            return Ok(existing);
        }
        if existing.state != RecordingLayerState::Staged || existing.artifact.is_some() {
            return Err(RecordingStoreError::RecordingLayerConflict {
                layer_id: layer_id.to_string(),
            });
        }
        let artifact = self.store.artifact_aggregate(artifact_id).await?.ok_or(
            RecordingStoreError::MissingRecord {
                operation: "recording layer Artifact occurrence",
            },
        )?;
        if artifact.occurrence.tenant != identity.tenant_id.record_id()
            || artifact.blob.byte_len != existing.byte_len
            || existing.sha256.as_deref() != Some(artifact.blob.sha256.as_str())
        {
            return Err(RecordingStoreError::RecordingLayerConflict {
                layer_id: layer_id.to_string(),
            });
        }
        let recording_id = recording_id_from_record(&existing.recording)?;
        let recording = self
            .recording(identity.tenant_id, recording_id)
            .await?
            .ok_or_else(|| RecordingStoreError::RecordingNotFound(recording_id.to_string()))?;

        self.client()
            .query(include_str!(
                "queries/recording_catalog/commit_recording_layer.surql"
            ))
            .bind(("layer", layer_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("artifact", artifact_id.record_id()))
            .bind(("dataset", recording.dataset))
            .await?
            .check()?;
        self.recording_layer(identity.tenant_id, layer_id)
            .await?
            .ok_or(RecordingStoreError::MissingRecord {
                operation: "recording layer commit readback",
            })
    }

    pub async fn fail_recording_layer(
        &self,
        identity: &PlatformIdentity,
        layer_id: RecordingLayerId,
        reason: &str,
    ) -> Result<RecordingLayerRecord, RecordingStoreError> {
        validate_text("recording layer failure_reason", reason, 2_048)?;
        let existing = self
            .recording_layer(identity.tenant_id, layer_id)
            .await?
            .ok_or(RecordingStoreError::MissingRecord {
                operation: "recording layer failure",
            })?;
        if existing.state == RecordingLayerState::Failed
            && existing.failure_reason.as_deref() == Some(reason)
        {
            return Ok(existing);
        }
        if matches!(
            existing.state,
            RecordingLayerState::Committed | RecordingLayerState::Failed
        ) {
            return Err(RecordingStoreError::RecordingLayerConflict {
                layer_id: layer_id.to_string(),
            });
        }
        recording_id_from_record(&existing.recording)?;

        self.client()
            .query(include_str!(
                "queries/recording_catalog/fail_recording_layer.surql"
            ))
            .bind(("layer", layer_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("reason", reason.to_owned()))
            .await?
            .check()?;
        self.recording_layer(identity.tenant_id, layer_id)
            .await?
            .ok_or(RecordingStoreError::MissingRecord {
                operation: "recording layer failure readback",
            })
    }

    pub async fn recording_layer(
        &self,
        tenant_id: TenantId,
        layer_id: RecordingLayerId,
    ) -> Result<Option<RecordingLayerRecord>, RecordingStoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "queries/recording_catalog/recording_layer.surql"
            ))
            .bind(("layer", layer_id.record_id()))
            .bind(("tenant", tenant_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn recording_layer_by_name(
        &self,
        tenant_id: TenantId,
        recording_id: RecordingId,
        layer_name: &str,
    ) -> Result<Option<RecordingLayerRecord>, RecordingStoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "queries/recording_catalog/recording_layer_by_name.surql"
            ))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("recording", recording_id.record_id()))
            .bind(("layer_name", layer_name.to_owned()))
            .await?
            .check()?;
        let records: Vec<RecordingLayerRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }

    pub async fn recording_layer_by_staging_path(
        &self,
        tenant_id: TenantId,
        staging_path: &str,
    ) -> Result<Option<RecordingLayerRecord>, RecordingStoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "queries/recording_catalog/recording_layer_by_staging_path.surql"
            ))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("staging_path", staging_path.to_owned()))
            .await?
            .check()?;
        let records: Vec<RecordingLayerRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }

    pub async fn recording_layers(
        &self,
        tenant_id: TenantId,
        recording_id: RecordingId,
        limit: u32,
    ) -> Result<Vec<RecordingLayerRecord>, RecordingStoreError> {
        if limit == 0 || limit > MAX_LAYER_LIMIT {
            return Err(RecordingStoreError::InvalidRecordingField {
                field: "limit",
                reason: "must be in 1..=10000",
            });
        }
        let mut response = self
            .client()
            .query(include_str!(
                "queries/recording_catalog/recording_layers.surql"
            ))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("recording", recording_id.record_id()))
            .bind(("limit", i64::from(limit)))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn pending_recording_layers(
        &self,
        tenant_id: TenantId,
        limit: u32,
    ) -> Result<Vec<RecordingLayerRecord>, RecordingStoreError> {
        if limit == 0 || limit > MAX_LAYER_LIMIT {
            return Err(RecordingStoreError::InvalidRecordingField {
                field: "limit",
                reason: "must be in 1..=10000",
            });
        }
        let mut response = self
            .client()
            .query(include_str!(
                "queries/recording_catalog/pending_recording_layers.surql"
            ))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("limit", i64::from(limit)))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    /// Return installation-wide mutable layers for bounded Recording Hub startup recovery.
    ///
    /// Tenant-scoped catalog callers use [`Self::pending_recording_layers`].
    pub async fn pending_recording_layers_for_recovery(
        &self,
        limit: u32,
    ) -> Result<Vec<RecordingLayerRecord>, RecordingStoreError> {
        if limit == 0 || limit > MAX_LAYER_LIMIT {
            return Err(RecordingStoreError::InvalidRecordingField {
                field: "limit",
                reason: "must be in 1..=10000",
            });
        }
        let mut response = self
            .client()
            .query(include_str!(
                "queries/recording_catalog/pending_recording_layers_for_recovery.surql"
            ))
            .bind(("limit", i64::from(limit)))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn cleanup_expired_recording_catalog_authority(
        &self,
        now: DateTime<Utc>,
    ) -> Result<RecordingCatalogCleanup, RecordingStoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "queries/recording_catalog/cleanup_expired_recording_catalog_authority.surql"
            ))
            .bind(("now", now))
            .await?
            .check()?;
        // BEGIN and the two LET statements occupy response slots 0..=2.
        let cleanup: Option<RecordingCatalogCleanupContent> = response.take(3)?;
        let cleanup = cleanup.ok_or(RecordingStoreError::MissingRecord {
            operation: "recording catalog cleanup result",
        })?;
        Ok(RecordingCatalogCleanup {
            projection_receipts: usize::try_from(cleanup.projection_receipts).map_err(|_| {
                RecordingStoreError::MissingRecord {
                    operation: "recording projection cleanup count conversion",
                }
            })?,
            read_grants: usize::try_from(cleanup.read_grants).map_err(|_| {
                RecordingStoreError::MissingRecord {
                    operation: "recording grant cleanup count conversion",
                }
            })?,
        })
    }
}

pub fn capture_layer_name(ordinal: i64) -> Result<String, RecordingStoreError> {
    if ordinal < 0 {
        return Err(RecordingStoreError::InvalidRecordingField {
            field: "capture ordinal",
            reason: "must be non-negative",
        });
    }
    Ok(format!("capture-{ordinal:020}"))
}

fn validate_layer_draft(draft: &RecordingLayerDraft) -> Result<(), RecordingStoreError> {
    validate_text("layer_name", &draft.layer_name, MAX_LAYER_NAME_BYTES)?;
    match draft.kind {
        RecordingLayerKind::Capture => {
            let ordinal = draft
                .ordinal
                .ok_or(RecordingStoreError::InvalidRecordingField {
                    field: "capture ordinal",
                    reason: "is required",
                })?;
            if draft.layer_name != capture_layer_name(ordinal)? {
                return Err(RecordingStoreError::InvalidRecordingField {
                    field: "layer_name",
                    reason: "must equal the canonical capture ordinal name",
                });
            }
            let path = draft.staging_path.as_deref().ok_or(
                RecordingStoreError::InvalidRecordingField {
                    field: "staging_path",
                    reason: "is required for a capture layer",
                },
            )?;
            validate_relative_rrd_path(path)?;
        }
        RecordingLayerKind::Properties => {
            if draft.layer_name != "properties" || draft.ordinal.is_some() {
                return Err(RecordingStoreError::InvalidRecordingField {
                    field: "properties layer",
                    reason: "must use name properties without an ordinal",
                });
            }
        }
        RecordingLayerKind::Derived => {
            if !draft.layer_name.starts_with("derived-") || draft.ordinal.is_some() {
                return Err(RecordingStoreError::InvalidRecordingField {
                    field: "derived layer",
                    reason: "must use a derived- name without an ordinal",
                });
            }
        }
    }
    if let Some(path) = draft.staging_path.as_deref() {
        validate_relative_rrd_path(path)?;
    }
    Ok(())
}

fn validate_existing_dataset(
    existing: &RecordingDatasetRecord,
    draft: &RecordingDatasetDraft,
) -> Result<(), RecordingStoreError> {
    if existing.tenant != draft.identity.tenant_id.record_id()
        || existing.dataset_key != draft.dataset_key
        || existing.display_label != draft.display_label
        || existing.default_blueprint_artifact
            != draft
                .default_blueprint_artifact_id
                .map(ArtifactId::record_id)
        || existing.retention_mode != draft.retention_mode
        || existing.retention_expires_at != draft.retention_expires_at
    {
        return Err(RecordingStoreError::RecordingDatasetConflict {
            dataset_id: dataset_id_from_record(&existing.id)?.to_string(),
        });
    }
    Ok(())
}

fn validate_existing_layer(
    existing: &RecordingLayerRecord,
    draft: &RecordingLayerDraft,
) -> Result<(), RecordingStoreError> {
    if existing.tenant != draft.identity.tenant_id.record_id()
        || existing.recording != draft.recording_id.record_id()
        || existing.layer_name != draft.layer_name
        || existing.kind != draft.kind
        || existing.ordinal != draft.ordinal
        || (existing.state != RecordingLayerState::Committed
            && existing.staging_path != draft.staging_path)
    {
        return Err(RecordingStoreError::RecordingLayerConflict {
            layer_id: layer_id_from_record(&existing.id)?.to_string(),
        });
    }
    Ok(())
}

fn validate_retention(
    mode: RecordingRetentionMode,
    expires_at: Option<DateTime<Utc>>,
) -> Result<(), RecordingStoreError> {
    let valid = match mode {
        RecordingRetentionMode::RetainUntil => expires_at.is_some_and(|value| value > Utc::now()),
        RecordingRetentionMode::InstallationDefault | RecordingRetentionMode::RetainForever => {
            expires_at.is_none()
        }
    };
    if !valid {
        return Err(RecordingStoreError::InvalidRecordingField {
            field: "recording dataset retention",
            reason: "mode and expiry are inconsistent",
        });
    }
    Ok(())
}

fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), RecordingStoreError> {
    if value.trim().is_empty() || value.trim() != value {
        return Err(RecordingStoreError::InvalidRecordingField {
            field,
            reason: "must be nonempty and trimmed",
        });
    }
    if value.len() > max_bytes {
        return Err(RecordingStoreError::InvalidRecordingField {
            field,
            reason: "exceeds maximum encoded length",
        });
    }
    if value.chars().any(char::is_control) {
        return Err(RecordingStoreError::InvalidRecordingField {
            field,
            reason: "must not contain control characters",
        });
    }
    Ok(())
}

fn validate_relative_rrd_path(value: &str) -> Result<(), RecordingStoreError> {
    validate_text("staging_path", value, 4_096)?;
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir
                    | Component::CurDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        })
        || path.extension().and_then(|value| value.to_str()) != Some("rrd")
    {
        return Err(RecordingStoreError::InvalidRecordingField {
            field: "staging_path",
            reason: "must be a normalized relative .rrd path",
        });
    }
    Ok(())
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), RecordingStoreError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RecordingStoreError::InvalidRecordingField {
            field,
            reason: "must be 64 hexadecimal characters",
        });
    }
    Ok(())
}

fn recording_id_from_record(record: &RecordId) -> Result<RecordingId, RecordingStoreError> {
    typed_uuid_from_record(record, RecordingId::TABLE).map(RecordingId::from_uuid)
}

fn dataset_id_from_record(record: &RecordId) -> Result<RecordingDatasetId, RecordingStoreError> {
    typed_uuid_from_record(record, RecordingDatasetId::TABLE).map(RecordingDatasetId::from_uuid)
}

fn layer_id_from_record(record: &RecordId) -> Result<RecordingLayerId, RecordingStoreError> {
    typed_uuid_from_record(record, RecordingLayerId::TABLE).map(RecordingLayerId::from_uuid)
}

fn typed_uuid_from_record(
    record: &RecordId,
    table: &'static str,
) -> Result<Uuid, RecordingStoreError> {
    if record.table.as_str() != table {
        return Err(RecordingStoreError::InvalidRecordingField {
            field: "record_id",
            reason: "has the wrong table",
        });
    }
    let raw = match &record.key {
        RecordIdKey::Uuid(value) => value.to_string(),
        RecordIdKey::String(value) => value.clone(),
        _ => {
            return Err(RecordingStoreError::InvalidRecordingField {
                field: "record_id",
                reason: "must use a UUID key",
            });
        }
    };
    let value = Uuid::from_str(&raw).map_err(|_| RecordingStoreError::InvalidRecordingField {
        field: "record_id",
        reason: "must use a UUID key",
    })?;
    if value.get_version_num() != 7 {
        return Err(RecordingStoreError::InvalidRecordingField {
            field: "record_id",
            reason: "must use UUIDv7",
        });
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    #[test]
    fn capture_names_are_fixed_width_and_ordered() {
        let first = capture_layer_name(2).unwrap();
        let second = capture_layer_name(10).unwrap();
        assert_eq!(first, "capture-00000000000000000002");
        assert!(first < second);
    }

    #[test]
    fn retention_contract_rejects_ambiguous_expiry() {
        assert!(
            validate_retention(RecordingRetentionMode::RetainForever, Some(Utc::now())).is_err()
        );
        assert!(
            validate_retention(
                RecordingRetentionMode::RetainUntil,
                Some(Utc::now() + TimeDelta::hours(1))
            )
            .is_ok()
        );
    }
}
