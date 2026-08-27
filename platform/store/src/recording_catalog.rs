use std::collections::BTreeMap;
use std::path::{Component, Path};
use std::str::FromStr as _;

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};
use uuid::Uuid;

use crate::{
    ArtifactId, InvocationAuthorityRecord, OpenObject, OutboxDraft, PlatformIdentity,
    PlatformStore, RecordingDatasetId, RecordingDatasetRecord, RecordingId, RecordingLayerId,
    RecordingLayerKind, RecordingLayerRecord, RecordingLayerState, RecordingProjectionReceiptId,
    RecordingProjectionReceiptRecord, RecordingProjectionState, RecordingReadGrantClass,
    RecordingReadGrantId, RecordingReadGrantRecord, RecordingRetentionMode, RecordingState,
    StoreError, TenantId, deterministic_work_context_id,
};

const EVENT_SCHEMA_VERSION: i64 = 1;
const MAX_DATASET_KEY_BYTES: usize = 128;
const MAX_DISPLAY_LABEL_BYTES: usize = 256;
const MAX_LAYER_NAME_BYTES: usize = 256;
const MAX_LAYER_LIMIT: u32 = 10_000;
const MAX_GRANT_RECORDINGS: usize = 500;
const MAX_GRANT_TTL: TimeDelta = TimeDelta::hours(1);

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
    ) -> Result<Self, StoreError> {
        let ordinal = Some(ordinal);
        let layer_name = capture_layer_name(ordinal.expect("capture ordinal exists"))?;
        Ok(Self {
            identity,
            recording_id,
            layer_name,
            kind: RecordingLayerKind::Capture,
            ordinal,
            staging_path: Some(staging_path),
            start_time,
        })
    }
}

#[derive(Clone, Debug)]
pub struct RecordingReadGrantDraft {
    pub identity: PlatformIdentity,
    pub authority: InvocationAuthorityRecord,
    pub dataset_id: RecordingDatasetId,
    pub grant_class: RecordingReadGrantClass,
    pub recording_ids: Vec<RecordingId>,
    pub catalog_revision: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct RecordingProjectionReceiptDraft {
    pub identity: PlatformIdentity,
    pub grant_id: RecordingReadGrantId,
    pub caller_idempotency_key: String,
    pub manifest_digest: String,
    pub query_digest: String,
    pub expires_at: DateTime<Utc>,
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

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct RecordingReadGrantContent {
    tenant: RecordId,
    dataset: RecordId,
    grant_class: RecordingReadGrantClass,
    recordings: Vec<RecordId>,
    admitted_set_digest: String,
    actor: RecordId,
    work_context: RecordId,
    policy_revision: String,
    catalog_revision: String,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct RecordingProjectionReceiptContent {
    tenant: RecordId,
    grant: RecordId,
    dataset: RecordId,
    recordings: Vec<RecordId>,
    actor: RecordId,
    work_context: RecordId,
    policy_revision: String,
    catalog_revision: String,
    caller_idempotency_key: String,
    manifest_digest: String,
    query_digest: String,
    state: RecordingProjectionState,
    result_byte_len: Option<i64>,
    result_sha256: Option<String>,
    failure_reason: Option<String>,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, SurrealValue)]
struct RecordingCatalogCleanupContent {
    projection_receipts: i64,
    read_grants: i64,
}

impl PlatformStore {
    pub async fn ensure_recording_dataset(
        &self,
        draft: RecordingDatasetDraft,
    ) -> Result<RecordingDatasetRecord, StoreError> {
        validate_text("dataset_key", &draft.dataset_key, MAX_DATASET_KEY_BYTES)?;
        validate_text(
            "dataset display_label",
            &draft.display_label,
            MAX_DISPLAY_LABEL_BYTES,
        )?;
        validate_retention(draft.retention_mode, draft.retention_expires_at)?;
        if let Some(artifact_id) = draft.default_blueprint_artifact_id {
            let artifact =
                self.artifact_aggregate(artifact_id)
                    .await?
                    .ok_or(StoreError::MissingRecord {
                        operation: "recording dataset default Blueprint artifact",
                    })?;
            if artifact.occurrence.tenant != draft.identity.tenant_id.record_id() {
                return Err(StoreError::RecordingDatasetConflict {
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
        let outbox = catalog_event(
            &draft.identity,
            "recording_dataset",
            id.to_string(),
            "recording.dataset_created",
            BTreeMap::from([(
                "dataset_key".to_owned(),
                serde_json::json!(draft.dataset_key),
            )]),
        );
        let result = self
            .db
            .query("BEGIN TRANSACTION; CREATE ONLY $dataset CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("dataset", id.record_id()))
            .bind(("content", content))
            .bind(("outbox", outbox))
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
            .ok_or(StoreError::MissingRecord {
                operation: "recording dataset creation readback",
            })
    }

    pub async fn recording_dataset(
        &self,
        tenant_id: TenantId,
        dataset_id: RecordingDatasetId,
    ) -> Result<Option<RecordingDatasetRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $dataset WHERE tenant = $tenant;")
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
    ) -> Result<Option<RecordingDatasetRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM recording_dataset WHERE tenant = $tenant AND dataset_key = $dataset_key LIMIT 1;")
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
    ) -> Result<RecordingLayerRecord, StoreError> {
        validate_layer_draft(&draft)?;
        let mut recording = self
            .recording(draft.identity.tenant_id, draft.recording_id)
            .await?
            .ok_or_else(|| StoreError::RecordingNotFound(draft.recording_id.to_string()))?;
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
            return Err(StoreError::RecordingStateConflict {
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
        let outbox = layer_event(
            &draft.identity,
            draft.recording_id,
            id,
            "recording.layer_opened",
            RecordingLayerState::Writing,
        );
        let result = self
            .db
            .query("BEGIN TRANSACTION; CREATE ONLY $layer CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("layer", id.record_id()))
            .bind(("content", content))
            .bind(("outbox", outbox))
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
            .ok_or(StoreError::MissingRecord {
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
    ) -> Result<RecordingLayerRecord, StoreError> {
        if byte_len < 0 || message_count < 0 {
            return Err(StoreError::InvalidRecordingField {
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
            .ok_or(StoreError::MissingRecord {
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
            return Err(StoreError::RecordingLayerConflict {
                layer_id: layer_id.to_string(),
            });
        }
        let recording_id = recording_id_from_record(&existing.recording)?;
        let outbox = layer_event(
            identity,
            recording_id,
            layer_id,
            "recording.layer_staged",
            RecordingLayerState::Staged,
        );
        self.db
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $layer); IF $current.revision != $revision OR $current.state != 'writing' { THROW 'recording_layer_revision_conflict'; }; UPDATE ONLY $layer SET state = 'staged', byte_len = $byte_len, message_count = $message_count, sha256 = $sha256, rrd_version = $rrd_version, schema_digest = $schema_digest, end_time = $end_time, failure_reason = NONE, updated_at = time::now(), revision += 1 RETURN NONE; UPDATE ONLY $recording SET last_data_at = $activity_at, updated_at = time::now(), revision += 1 WHERE tenant = $tenant AND state = 'live' RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
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
            .bind(("outbox", outbox))
            .await?
            .check()?;
        self.recording_layer(identity.tenant_id, layer_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording layer stage readback",
            })
    }

    pub async fn commit_recording_layer(
        &self,
        identity: &PlatformIdentity,
        layer_id: RecordingLayerId,
        artifact_id: ArtifactId,
    ) -> Result<RecordingLayerRecord, StoreError> {
        let existing = self
            .recording_layer(identity.tenant_id, layer_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording layer commit",
            })?;
        if existing.state == RecordingLayerState::Committed
            && existing.artifact == Some(artifact_id.record_id())
        {
            return Ok(existing);
        }
        if existing.state != RecordingLayerState::Staged || existing.artifact.is_some() {
            return Err(StoreError::RecordingLayerConflict {
                layer_id: layer_id.to_string(),
            });
        }
        let artifact =
            self.artifact_aggregate(artifact_id)
                .await?
                .ok_or(StoreError::MissingRecord {
                    operation: "recording layer Artifact occurrence",
                })?;
        if artifact.occurrence.tenant != identity.tenant_id.record_id()
            || artifact.blob.byte_len != existing.byte_len
            || existing.sha256.as_deref() != Some(artifact.blob.sha256.as_str())
        {
            return Err(StoreError::RecordingLayerConflict {
                layer_id: layer_id.to_string(),
            });
        }
        let recording_id = recording_id_from_record(&existing.recording)?;
        let recording = self
            .recording(identity.tenant_id, recording_id)
            .await?
            .ok_or_else(|| StoreError::RecordingNotFound(recording_id.to_string()))?;
        let outbox = layer_event(
            identity,
            recording_id,
            layer_id,
            "recording.layer_committed",
            RecordingLayerState::Committed,
        );
        self.db
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $layer); IF $current.revision != $revision OR $current.state != 'staged' OR $current.artifact != NONE { THROW 'recording_layer_revision_conflict'; }; UPDATE ONLY $layer SET state = 'committed', artifact = $artifact, staging_path = NONE, failure_reason = NONE, updated_at = time::now(), revision += 1 RETURN NONE; UPDATE ONLY $dataset SET revision += 1, updated_at = time::now() RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("layer", layer_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("artifact", artifact_id.record_id()))
            .bind(("dataset", recording.dataset))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        self.recording_layer(identity.tenant_id, layer_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording layer commit readback",
            })
    }

    pub async fn fail_recording_layer(
        &self,
        identity: &PlatformIdentity,
        layer_id: RecordingLayerId,
        reason: &str,
    ) -> Result<RecordingLayerRecord, StoreError> {
        validate_text("recording layer failure_reason", reason, 2_048)?;
        let existing = self
            .recording_layer(identity.tenant_id, layer_id)
            .await?
            .ok_or(StoreError::MissingRecord {
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
            return Err(StoreError::RecordingLayerConflict {
                layer_id: layer_id.to_string(),
            });
        }
        let recording_id = recording_id_from_record(&existing.recording)?;
        let outbox = layer_event(
            identity,
            recording_id,
            layer_id,
            "recording.layer_failed",
            RecordingLayerState::Failed,
        );
        self.db
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $layer); IF $current.revision != $revision OR $current.state IN ['committed', 'failed'] { THROW 'recording_layer_revision_conflict'; }; UPDATE ONLY $layer SET state = 'failed', failure_reason = $reason, staging_path = NONE, updated_at = time::now(), revision += 1 RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("layer", layer_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("reason", reason.to_owned()))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        self.recording_layer(identity.tenant_id, layer_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording layer failure readback",
            })
    }

    pub async fn recording_layer(
        &self,
        tenant_id: TenantId,
        layer_id: RecordingLayerId,
    ) -> Result<Option<RecordingLayerRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $layer WHERE tenant = $tenant;")
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
    ) -> Result<Option<RecordingLayerRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM recording_layer WHERE tenant = $tenant AND recording = $recording AND layer_name = $layer_name LIMIT 1;")
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
    ) -> Result<Option<RecordingLayerRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM recording_layer WHERE tenant = $tenant AND staging_path = $staging_path LIMIT 1;")
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
    ) -> Result<Vec<RecordingLayerRecord>, StoreError> {
        if limit == 0 || limit > MAX_LAYER_LIMIT {
            return Err(StoreError::InvalidRecordingField {
                field: "limit",
                reason: "must be in 1..=10000",
            });
        }
        let mut response = self
            .db
            .query("SELECT * FROM recording_layer WHERE tenant = $tenant AND recording = $recording ORDER BY ordinal ASC, layer_name ASC LIMIT $limit;")
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
    ) -> Result<Vec<RecordingLayerRecord>, StoreError> {
        if limit == 0 || limit > MAX_LAYER_LIMIT {
            return Err(StoreError::InvalidRecordingField {
                field: "limit",
                reason: "must be in 1..=10000",
            });
        }
        let mut response = self
            .db
            .query("SELECT * FROM recording_layer WHERE tenant = $tenant AND state IN ['writing', 'staged'] ORDER BY updated_at ASC LIMIT $limit;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("limit", i64::from(limit)))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn create_recording_read_grant(
        &self,
        mut draft: RecordingReadGrantDraft,
    ) -> Result<RecordingReadGrantRecord, StoreError> {
        validate_text("catalog_revision", &draft.catalog_revision, 128)?;
        let now = Utc::now();
        if draft.expires_at <= now || draft.expires_at > now + MAX_GRANT_TTL {
            return Err(StoreError::InvalidRecordingField {
                field: "grant expires_at",
                reason: "must be within the next hour",
            });
        }
        draft.recording_ids.sort_unstable();
        draft.recording_ids.dedup();
        if draft.recording_ids.is_empty() || draft.recording_ids.len() > MAX_GRANT_RECORDINGS {
            return Err(StoreError::InvalidRecordingField {
                field: "grant recordings",
                reason: "must contain 1..=500 recording UUIDs",
            });
        }
        let dataset = self
            .recording_dataset(draft.identity.tenant_id, draft.dataset_id)
            .await?
            .ok_or(StoreError::RecordingDatasetConflict {
                dataset_id: draft.dataset_id.to_string(),
            })?;
        for recording_id in &draft.recording_ids {
            let recording = self
                .recording(draft.identity.tenant_id, *recording_id)
                .await?
                .ok_or_else(|| StoreError::RecordingNotFound(recording_id.to_string()))?;
            if recording.dataset != dataset.id {
                return Err(StoreError::RecordingReadGrantConflict {
                    grant_id: "new".to_owned(),
                });
            }
        }
        let admitted_set_digest =
            admitted_set_digest(draft.dataset_id, draft.grant_class, &draft.recording_ids);
        let id = RecordingReadGrantId::new();
        let work_context = deterministic_work_context_id(
            &draft.identity.tenant_key,
            &draft.authority.context_key,
        )?;
        let content = RecordingReadGrantContent {
            tenant: draft.identity.tenant_id.record_id(),
            dataset: draft.dataset_id.record_id(),
            grant_class: draft.grant_class,
            recordings: draft
                .recording_ids
                .iter()
                .copied()
                .map(RecordingId::record_id)
                .collect(),
            admitted_set_digest,
            actor: draft.identity.principal_id.record_id(),
            work_context: work_context.record_id(),
            policy_revision: draft.authority.policy_revision,
            catalog_revision: draft.catalog_revision,
            expires_at: draft.expires_at,
            created_at: now,
        };
        self.db
            .query("CREATE ONLY $grant CONTENT $content RETURN NONE;")
            .bind(("grant", id.record_id()))
            .bind(("content", content))
            .await?
            .check()?;
        self.recording_read_grant(draft.identity.tenant_id, id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording read grant creation readback",
            })
    }

    pub async fn recording_read_grant(
        &self,
        tenant_id: TenantId,
        grant_id: RecordingReadGrantId,
    ) -> Result<Option<RecordingReadGrantRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $grant WHERE tenant = $tenant AND expires_at > time::now();")
            .bind(("grant", grant_id.record_id()))
            .bind(("tenant", tenant_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    /// Resolve an unexpired grant by its cryptographically unguessable UUID.
    ///
    /// This service-only lookup exists for Redap bearer redemption, where the
    /// verified token subject is the durable grant ID and no tenant selector is
    /// supplied by the client. Callers must still validate the grant class and
    /// route against the returned record.
    pub async fn recording_read_grant_by_id(
        &self,
        grant_id: RecordingReadGrantId,
    ) -> Result<Option<RecordingReadGrantRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $grant WHERE expires_at > time::now();")
            .bind(("grant", grant_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn reserve_recording_projection(
        &self,
        draft: RecordingProjectionReceiptDraft,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        validate_text(
            "projection idempotency key",
            &draft.caller_idempotency_key,
            128,
        )?;
        validate_sha256("manifest_digest", &draft.manifest_digest)?;
        validate_sha256("query_digest", &draft.query_digest)?;
        let grant = self
            .recording_read_grant(draft.identity.tenant_id, draft.grant_id)
            .await?
            .ok_or(StoreError::RecordingReadGrantConflict {
                grant_id: draft.grant_id.to_string(),
            })?;
        if grant.actor != draft.identity.principal_id.record_id()
            || grant.grant_class != RecordingReadGrantClass::AppProjection
            || draft.expires_at > grant.expires_at
            || draft.expires_at <= Utc::now()
        {
            return Err(StoreError::RecordingReadGrantConflict {
                grant_id: draft.grant_id.to_string(),
            });
        }
        if let Some(existing) = self
            .recording_projection_by_idempotency_key(
                draft.identity.tenant_id,
                draft.identity.principal_id,
                &draft.caller_idempotency_key,
            )
            .await?
        {
            if existing.manifest_digest == draft.manifest_digest
                && existing.query_digest == draft.query_digest
            {
                return Ok(existing);
            }
            return Err(StoreError::RecordingProjectionConflict {
                projection_id: projection_id_from_record(&existing.id)?.to_string(),
            });
        }
        let id = RecordingProjectionReceiptId::new();
        let now = Utc::now();
        let content = RecordingProjectionReceiptContent {
            tenant: draft.identity.tenant_id.record_id(),
            grant: draft.grant_id.record_id(),
            dataset: grant.dataset,
            recordings: grant.recordings,
            actor: grant.actor,
            work_context: grant.work_context,
            policy_revision: grant.policy_revision,
            catalog_revision: grant.catalog_revision,
            caller_idempotency_key: draft.caller_idempotency_key.clone(),
            manifest_digest: draft.manifest_digest.clone(),
            query_digest: draft.query_digest.clone(),
            state: RecordingProjectionState::Reserved,
            result_byte_len: None,
            result_sha256: None,
            failure_reason: None,
            expires_at: draft.expires_at,
            created_at: now,
            updated_at: now,
        };
        let result = self
            .db
            .query("CREATE ONLY $projection CONTENT $content RETURN NONE;")
            .bind(("projection", id.record_id()))
            .bind(("content", content))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if let Some(existing) = self
                .recording_projection_by_idempotency_key(
                    draft.identity.tenant_id,
                    draft.identity.principal_id,
                    &draft.caller_idempotency_key,
                )
                .await?
            {
                if existing.manifest_digest == draft.manifest_digest
                    && existing.query_digest == draft.query_digest
                {
                    return Ok(existing);
                }
                return Err(StoreError::RecordingProjectionConflict {
                    projection_id: projection_id_from_record(&existing.id)?.to_string(),
                });
            }
            return Err(error.into());
        }
        self.recording_projection_receipt(draft.identity.tenant_id, id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording projection reservation readback",
            })
    }

    pub async fn recording_projection_receipt(
        &self,
        tenant_id: TenantId,
        projection_id: RecordingProjectionReceiptId,
    ) -> Result<Option<RecordingProjectionReceiptRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $projection WHERE tenant = $tenant AND expires_at > time::now();")
            .bind(("projection", projection_id.record_id()))
            .bind(("tenant", tenant_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn begin_recording_projection(
        &self,
        identity: &PlatformIdentity,
        projection_id: RecordingProjectionReceiptId,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        let existing = self
            .recording_projection_receipt(identity.tenant_id, projection_id)
            .await?
            .ok_or(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            })?;
        ensure_projection_actor(&existing, identity, projection_id)?;
        if existing.state == RecordingProjectionState::Materializing {
            return Ok(existing);
        }
        if existing.state != RecordingProjectionState::Reserved {
            return Err(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            });
        }
        self.db
            .query("LET $current = (SELECT * FROM ONLY $projection); IF $current.state != 'reserved' { THROW 'recording_projection_state_conflict'; }; UPDATE ONLY $projection SET state = 'materializing', updated_at = time::now() RETURN NONE;")
            .bind(("projection", projection_id.record_id()))
            .await?
            .check()?;
        self.recording_projection_receipt(identity.tenant_id, projection_id)
            .await?
            .ok_or(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            })
    }

    pub async fn complete_recording_projection(
        &self,
        identity: &PlatformIdentity,
        projection_id: RecordingProjectionReceiptId,
        result_byte_len: i64,
        result_sha256: &str,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        if result_byte_len < 0 {
            return Err(StoreError::InvalidRecordingField {
                field: "projection result_byte_len",
                reason: "must be non-negative",
            });
        }
        validate_sha256("projection result_sha256", result_sha256)?;
        let existing = self
            .recording_projection_receipt(identity.tenant_id, projection_id)
            .await?
            .ok_or(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            })?;
        ensure_projection_actor(&existing, identity, projection_id)?;
        if existing.state == RecordingProjectionState::Ready
            && existing.result_byte_len == Some(result_byte_len)
            && existing.result_sha256.as_deref() == Some(result_sha256)
        {
            return Ok(existing);
        }
        if existing.state != RecordingProjectionState::Materializing {
            return Err(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            });
        }
        self.db
            .query("LET $current = (SELECT * FROM ONLY $projection); IF $current.state != 'materializing' { THROW 'recording_projection_state_conflict'; }; UPDATE ONLY $projection SET state = 'ready', result_byte_len = $byte_len, result_sha256 = $sha256, failure_reason = NONE, updated_at = time::now() RETURN NONE;")
            .bind(("projection", projection_id.record_id()))
            .bind(("byte_len", result_byte_len))
            .bind(("sha256", result_sha256.to_owned()))
            .await?
            .check()?;
        self.recording_projection_receipt(identity.tenant_id, projection_id)
            .await?
            .ok_or(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            })
    }

    pub async fn fail_recording_projection(
        &self,
        identity: &PlatformIdentity,
        projection_id: RecordingProjectionReceiptId,
        reason: &str,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        self.finish_recording_projection(
            identity,
            projection_id,
            RecordingProjectionState::Failed,
            reason,
        )
        .await
    }

    pub async fn cancel_recording_projection(
        &self,
        identity: &PlatformIdentity,
        projection_id: RecordingProjectionReceiptId,
        reason: &str,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        self.finish_recording_projection(
            identity,
            projection_id,
            RecordingProjectionState::Cancelled,
            reason,
        )
        .await
    }

    pub async fn cleanup_expired_recording_catalog_authority(
        &self,
        now: DateTime<Utc>,
    ) -> Result<RecordingCatalogCleanup, StoreError> {
        let mut response = self
            .db
            .query("BEGIN TRANSACTION; LET $expired_projections = (DELETE recording_projection_receipt WHERE expires_at <= $now RETURN BEFORE); LET $expired_grants = (DELETE recording_read_grant WHERE expires_at <= $now RETURN BEFORE); RETURN { projection_receipts: array::len($expired_projections), read_grants: array::len($expired_grants) }; COMMIT TRANSACTION;")
            .bind(("now", now))
            .await?
            .check()?;
        // BEGIN and the two LET statements occupy response slots 0..=2.
        let cleanup: Option<RecordingCatalogCleanupContent> = response.take(3)?;
        let cleanup = cleanup.ok_or(StoreError::MissingRecord {
            operation: "recording catalog cleanup result",
        })?;
        Ok(RecordingCatalogCleanup {
            projection_receipts: usize::try_from(cleanup.projection_receipts).map_err(|_| {
                StoreError::MissingRecord {
                    operation: "recording projection cleanup count conversion",
                }
            })?,
            read_grants: usize::try_from(cleanup.read_grants).map_err(|_| {
                StoreError::MissingRecord {
                    operation: "recording grant cleanup count conversion",
                }
            })?,
        })
    }

    async fn finish_recording_projection(
        &self,
        identity: &PlatformIdentity,
        projection_id: RecordingProjectionReceiptId,
        target: RecordingProjectionState,
        reason: &str,
    ) -> Result<RecordingProjectionReceiptRecord, StoreError> {
        validate_text("projection failure_reason", reason, 2_048)?;
        if !matches!(
            target,
            RecordingProjectionState::Failed | RecordingProjectionState::Cancelled
        ) {
            return Err(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            });
        }
        let existing = self
            .recording_projection_receipt(identity.tenant_id, projection_id)
            .await?
            .ok_or(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            })?;
        ensure_projection_actor(&existing, identity, projection_id)?;
        if existing.state == target && existing.failure_reason.as_deref() == Some(reason) {
            return Ok(existing);
        }
        if !matches!(
            existing.state,
            RecordingProjectionState::Reserved | RecordingProjectionState::Materializing
        ) {
            return Err(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            });
        }
        self.db
            .query("LET $current = (SELECT * FROM ONLY $projection); IF $current.state NOT IN ['reserved', 'materializing'] { THROW 'recording_projection_state_conflict'; }; UPDATE ONLY $projection SET state = $state, failure_reason = $reason, updated_at = time::now() RETURN NONE;")
            .bind(("projection", projection_id.record_id()))
            .bind(("state", target))
            .bind(("reason", reason.to_owned()))
            .await?
            .check()?;
        self.recording_projection_receipt(identity.tenant_id, projection_id)
            .await?
            .ok_or(StoreError::RecordingProjectionConflict {
                projection_id: projection_id.to_string(),
            })
    }

    pub async fn recording_projection_by_idempotency_key(
        &self,
        tenant_id: TenantId,
        actor_id: crate::PrincipalId,
        idempotency_key: &str,
    ) -> Result<Option<RecordingProjectionReceiptRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM recording_projection_receipt WHERE tenant = $tenant AND actor = $actor AND caller_idempotency_key = $key AND expires_at > time::now() LIMIT 1;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("actor", actor_id.record_id()))
            .bind(("key", idempotency_key.to_owned()))
            .await?
            .check()?;
        let records: Vec<RecordingProjectionReceiptRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }
}

fn ensure_projection_actor(
    receipt: &RecordingProjectionReceiptRecord,
    identity: &PlatformIdentity,
    projection_id: RecordingProjectionReceiptId,
) -> Result<(), StoreError> {
    if receipt.actor != identity.principal_id.record_id() {
        return Err(StoreError::RecordingProjectionConflict {
            projection_id: projection_id.to_string(),
        });
    }
    Ok(())
}

pub fn capture_layer_name(ordinal: i64) -> Result<String, StoreError> {
    if ordinal < 0 {
        return Err(StoreError::InvalidRecordingField {
            field: "capture ordinal",
            reason: "must be non-negative",
        });
    }
    Ok(format!("capture-{ordinal:020}"))
}

fn validate_layer_draft(draft: &RecordingLayerDraft) -> Result<(), StoreError> {
    validate_text("layer_name", &draft.layer_name, MAX_LAYER_NAME_BYTES)?;
    match draft.kind {
        RecordingLayerKind::Capture => {
            let ordinal = draft.ordinal.ok_or(StoreError::InvalidRecordingField {
                field: "capture ordinal",
                reason: "is required",
            })?;
            if draft.layer_name != capture_layer_name(ordinal)? {
                return Err(StoreError::InvalidRecordingField {
                    field: "layer_name",
                    reason: "must equal the canonical capture ordinal name",
                });
            }
            let path = draft
                .staging_path
                .as_deref()
                .ok_or(StoreError::InvalidRecordingField {
                    field: "staging_path",
                    reason: "is required for a capture layer",
                })?;
            validate_relative_rrd_path(path)?;
        }
        RecordingLayerKind::Properties => {
            if draft.layer_name != "properties" || draft.ordinal.is_some() {
                return Err(StoreError::InvalidRecordingField {
                    field: "properties layer",
                    reason: "must use name properties without an ordinal",
                });
            }
        }
        RecordingLayerKind::Derived => {
            if !draft.layer_name.starts_with("derived-") || draft.ordinal.is_some() {
                return Err(StoreError::InvalidRecordingField {
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
) -> Result<(), StoreError> {
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
        return Err(StoreError::RecordingDatasetConflict {
            dataset_id: dataset_id_from_record(&existing.id)?.to_string(),
        });
    }
    Ok(())
}

fn validate_existing_layer(
    existing: &RecordingLayerRecord,
    draft: &RecordingLayerDraft,
) -> Result<(), StoreError> {
    if existing.tenant != draft.identity.tenant_id.record_id()
        || existing.recording != draft.recording_id.record_id()
        || existing.layer_name != draft.layer_name
        || existing.kind != draft.kind
        || existing.ordinal != draft.ordinal
        || (existing.state != RecordingLayerState::Committed
            && existing.staging_path != draft.staging_path)
    {
        return Err(StoreError::RecordingLayerConflict {
            layer_id: layer_id_from_record(&existing.id)?.to_string(),
        });
    }
    Ok(())
}

fn validate_retention(
    mode: RecordingRetentionMode,
    expires_at: Option<DateTime<Utc>>,
) -> Result<(), StoreError> {
    let valid = match mode {
        RecordingRetentionMode::RetainUntil => expires_at.is_some_and(|value| value > Utc::now()),
        RecordingRetentionMode::InstallationDefault | RecordingRetentionMode::RetainForever => {
            expires_at.is_none()
        }
    };
    if !valid {
        return Err(StoreError::InvalidRecordingField {
            field: "recording dataset retention",
            reason: "mode and expiry are inconsistent",
        });
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str, max_bytes: usize) -> Result<(), StoreError> {
    if value.trim().is_empty() || value.trim() != value {
        return Err(StoreError::InvalidRecordingField {
            field,
            reason: "must be nonempty and trimmed",
        });
    }
    if value.len() > max_bytes {
        return Err(StoreError::InvalidRecordingField {
            field,
            reason: "exceeds maximum encoded length",
        });
    }
    if value.chars().any(char::is_control) {
        return Err(StoreError::InvalidRecordingField {
            field,
            reason: "must not contain control characters",
        });
    }
    Ok(())
}

fn validate_relative_rrd_path(value: &str) -> Result<(), StoreError> {
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
        return Err(StoreError::InvalidRecordingField {
            field: "staging_path",
            reason: "must be a normalized relative .rrd path",
        });
    }
    Ok(())
}

fn validate_sha256(field: &'static str, value: &str) -> Result<(), StoreError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StoreError::InvalidRecordingField {
            field,
            reason: "must be 64 hexadecimal characters",
        });
    }
    Ok(())
}

fn admitted_set_digest(
    dataset_id: RecordingDatasetId,
    grant_class: RecordingReadGrantClass,
    recording_ids: &[RecordingId],
) -> String {
    let mut digest = Sha256::new();
    digest.update(dataset_id.as_uuid().as_bytes());
    digest.update(serde_json::to_vec(&grant_class).expect("closed grant class serializes"));
    for recording_id in recording_ids {
        digest.update(recording_id.as_uuid().as_bytes());
    }
    hex::encode(digest.finalize())
}

fn catalog_event(
    identity: &PlatformIdentity,
    aggregate_type: &str,
    aggregate_id: String,
    event_type: &str,
    payload: BTreeMap<String, serde_json::Value>,
) -> OutboxDraft {
    OutboxDraft::now(
        Some(identity.tenant_id.record_id()),
        aggregate_type,
        aggregate_id,
        event_type,
        EVENT_SCHEMA_VERSION,
        OpenObject::new(payload),
    )
}

fn layer_event(
    identity: &PlatformIdentity,
    recording_id: RecordingId,
    layer_id: RecordingLayerId,
    event_type: &str,
    state: RecordingLayerState,
) -> OutboxDraft {
    catalog_event(
        identity,
        "recording_layer",
        layer_id.to_string(),
        event_type,
        BTreeMap::from([
            ("recording_id".to_owned(), serde_json::json!(recording_id)),
            ("layer_id".to_owned(), serde_json::json!(layer_id)),
            ("state".to_owned(), serde_json::json!(state)),
        ]),
    )
}

fn recording_id_from_record(record: &RecordId) -> Result<RecordingId, StoreError> {
    typed_uuid_from_record(record, RecordingId::TABLE).map(RecordingId::from_uuid)
}

fn dataset_id_from_record(record: &RecordId) -> Result<RecordingDatasetId, StoreError> {
    typed_uuid_from_record(record, RecordingDatasetId::TABLE).map(RecordingDatasetId::from_uuid)
}

fn layer_id_from_record(record: &RecordId) -> Result<RecordingLayerId, StoreError> {
    typed_uuid_from_record(record, RecordingLayerId::TABLE).map(RecordingLayerId::from_uuid)
}

fn projection_id_from_record(
    record: &RecordId,
) -> Result<RecordingProjectionReceiptId, StoreError> {
    typed_uuid_from_record(record, RecordingProjectionReceiptId::TABLE)
        .map(RecordingProjectionReceiptId::from_uuid)
}

fn typed_uuid_from_record(record: &RecordId, table: &'static str) -> Result<Uuid, StoreError> {
    if record.table.as_str() != table {
        return Err(StoreError::InvalidRecordingField {
            field: "record_id",
            reason: "has the wrong table",
        });
    }
    let raw = match &record.key {
        RecordIdKey::Uuid(value) => value.to_string(),
        RecordIdKey::String(value) => value.clone(),
        _ => {
            return Err(StoreError::InvalidRecordingField {
                field: "record_id",
                reason: "must use a UUID key",
            });
        }
    };
    let value = Uuid::from_str(&raw).map_err(|_| StoreError::InvalidRecordingField {
        field: "record_id",
        reason: "must use a UUID key",
    })?;
    if value.get_version_num() != 7 {
        return Err(StoreError::InvalidRecordingField {
            field: "record_id",
            reason: "must use UUIDv7",
        });
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_names_are_fixed_width_and_ordered() {
        let first = capture_layer_name(2).unwrap();
        let second = capture_layer_name(10).unwrap();
        assert_eq!(first, "capture-00000000000000000002");
        assert!(first < second);
    }

    #[test]
    fn admitted_recording_digest_is_order_sensitive_only_before_normalization() {
        let dataset = RecordingDatasetId::new();
        let mut recordings = vec![RecordingId::new(), RecordingId::new()];
        recordings.sort_unstable();
        let first = admitted_set_digest(
            dataset,
            RecordingReadGrantClass::CatalogDataset,
            &recordings,
        );
        let second = admitted_set_digest(
            dataset,
            RecordingReadGrantClass::CatalogDataset,
            &recordings,
        );
        assert_eq!(first, second);
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
