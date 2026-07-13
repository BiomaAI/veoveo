use std::collections::BTreeMap;
use std::path::{Component, Path};
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue, Uuid as SurrealUuid};
use uuid::Uuid;

use crate::identity::PLATFORM_ID_NAMESPACE;
use crate::{
    ArtifactId, OpenObject, OutboxDraft, PlatformIdentity, PlatformStore, RecordingId,
    RecordingRecord, RecordingState, SegmentId, SegmentRecord, SegmentState, StoreError, TaskId,
    TenantId,
};

const EVENT_SCHEMA_VERSION: i64 = 1;
const MAX_RECORDING_LIMIT: u32 = 500;
const MAX_SEGMENT_LIMIT: u32 = 10_000;

#[derive(Clone, Debug)]
pub struct RecordingDraft {
    pub identity: PlatformIdentity,
    pub dataset: String,
    pub application_id: String,
    pub recording_key: String,
    pub classification: String,
    pub labels: Vec<String>,
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub started_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct SegmentDraft {
    pub identity: PlatformIdentity,
    pub recording_id: RecordingId,
    pub segment_key: String,
    pub ordinal: i64,
    pub relative_path: String,
    pub start_time: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct SegmentSealBinding {
    pub segment_id: SegmentId,
    pub artifact_id: ArtifactId,
}

#[derive(Clone, Debug)]
pub struct RecordingSeal {
    pub identity: PlatformIdentity,
    pub recording_id: RecordingId,
    pub task_id: Option<TaskId>,
    pub manifest_artifact_id: ArtifactId,
    pub segments: Vec<SegmentSealBinding>,
    pub ended_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct RecordingContent {
    tenant: RecordId,
    owner: RecordId,
    dataset: String,
    application_id: String,
    recording_key: String,
    state: RecordingState,
    classification: String,
    labels: Vec<String>,
    metadata: OpenObject,
    manifest_artifact: Option<RecordId>,
    seal_task: Option<RecordId>,
    failure_reason: Option<String>,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    revision: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct SegmentContent {
    tenant: RecordId,
    recording: RecordId,
    segment_key: String,
    ordinal: i64,
    relative_path: String,
    artifact: Option<RecordId>,
    state: SegmentState,
    start_time: Option<DateTime<Utc>>,
    end_time: Option<DateTime<Utc>>,
    byte_len: i64,
    message_count: i64,
    sha256: Option<String>,
    failure_reason: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    revision: i64,
}

impl PlatformStore {
    pub async fn create_recording(
        &self,
        mut draft: RecordingDraft,
    ) -> Result<RecordingRecord, StoreError> {
        validate_name("dataset", &draft.dataset, 128)?;
        validate_name("application_id", &draft.application_id, 512)?;
        validate_name("recording_key", &draft.recording_key, 512)?;
        validate_name("classification", &draft.classification, 256)?;
        normalize_labels(&mut draft.labels)?;

        if let Some(existing) = self
            .recording_by_key(
                draft.identity.tenant_id,
                &draft.application_id,
                &draft.recording_key,
            )
            .await?
        {
            validate_existing_recording(&existing, &draft)?;
            return Ok(existing);
        }

        let id = RecordingId::new();
        let now = Utc::now();
        let content = RecordingContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            dataset: draft.dataset.clone(),
            application_id: draft.application_id.clone(),
            recording_key: draft.recording_key.clone(),
            state: RecordingState::Open,
            classification: draft.classification.clone(),
            labels: draft.labels.clone(),
            metadata: OpenObject::new(draft.metadata.clone()),
            manifest_artifact: None,
            seal_task: None,
            failure_reason: None,
            started_at: draft.started_at,
            ended_at: None,
            created_at: now,
            updated_at: now,
            revision: 0,
        };
        let outbox = recording_event(
            &draft.identity,
            id,
            "recording.created",
            RecordingState::Open,
        );
        let result = self
            .db
            .query("BEGIN TRANSACTION; CREATE ONLY $recording CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("recording", id.record_id()))
            .bind(("content", content.clone()))
            .bind(("outbox", outbox))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if let Some(existing) = self
                .recording_by_key(
                    draft.identity.tenant_id,
                    &content.application_id,
                    &content.recording_key,
                )
                .await?
            {
                validate_existing_recording(&existing, &draft)?;
                return Ok(existing);
            }
            return Err(error.into());
        }
        self.recording(draft.identity.tenant_id, id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording creation readback",
            })
    }

    pub async fn recording(
        &self,
        tenant_id: TenantId,
        recording_id: RecordingId,
    ) -> Result<Option<RecordingRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $recording WHERE tenant = $tenant;")
            .bind(("recording", recording_id.record_id()))
            .bind(("tenant", tenant_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn recording_by_key(
        &self,
        tenant_id: TenantId,
        application_id: &str,
        recording_key: &str,
    ) -> Result<Option<RecordingRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM recording WHERE tenant = $tenant AND application_id = $application_id AND recording_key = $recording_key LIMIT 1;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("application_id", application_id.to_owned()))
            .bind(("recording_key", recording_key.to_owned()))
            .await?
            .check()?;
        let records: Vec<RecordingRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }

    pub async fn list_recordings(
        &self,
        tenant_id: TenantId,
        limit: u32,
    ) -> Result<Vec<RecordingRecord>, StoreError> {
        if limit == 0 || limit > MAX_RECORDING_LIMIT {
            return Err(StoreError::InvalidRecordingField {
                field: "limit",
                reason: "must be in 1..=500",
            });
        }
        let mut response = self
            .db
            .query("SELECT * FROM recording WHERE tenant = $tenant ORDER BY started_at DESC LIMIT $limit;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("limit", i64::from(limit)))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn open_segment(&self, draft: SegmentDraft) -> Result<SegmentRecord, StoreError> {
        validate_name("segment_key", &draft.segment_key, 512)?;
        validate_relative_path(&draft.relative_path)?;
        if draft.ordinal < 0 {
            return Err(StoreError::InvalidRecordingField {
                field: "ordinal",
                reason: "must be non-negative",
            });
        }
        let recording = self
            .recording(draft.identity.tenant_id, draft.recording_id)
            .await?
            .ok_or_else(|| StoreError::RecordingNotFound(draft.recording_id.to_string()))?;
        if recording.state != RecordingState::Open {
            return Err(StoreError::RecordingStateConflict {
                recording_id: draft.recording_id.to_string(),
                state: recording_state_name(recording.state).to_owned(),
                target: "open segment",
            });
        }
        if let Some(existing) = self
            .segment_by_key(
                draft.identity.tenant_id,
                draft.recording_id,
                &draft.segment_key,
            )
            .await?
        {
            validate_existing_segment(&existing, &draft)?;
            return Ok(existing);
        }

        let id = SegmentId::new();
        let now = Utc::now();
        let content = SegmentContent {
            tenant: draft.identity.tenant_id.record_id(),
            recording: draft.recording_id.record_id(),
            segment_key: draft.segment_key.clone(),
            ordinal: draft.ordinal,
            relative_path: draft.relative_path.clone(),
            artifact: None,
            state: SegmentState::Writing,
            start_time: draft.start_time,
            end_time: None,
            byte_len: 0,
            message_count: 0,
            sha256: None,
            failure_reason: None,
            created_at: now,
            updated_at: now,
            revision: 0,
        };
        let edge = recording_segment_edge(draft.recording_id, id);
        let outbox = segment_event(
            &draft.identity,
            draft.recording_id,
            id,
            "recording.segment_opened",
            SegmentState::Writing,
        );
        let result = self
            .db
            .query("BEGIN TRANSACTION; CREATE ONLY $segment CONTENT $content RETURN NONE; RELATE ONLY $recording->$edge->$segment CONTENT { ordinal: $ordinal, created_at: time::now() } RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("segment", id.record_id()))
            .bind(("content", content.clone()))
            .bind(("recording", draft.recording_id.record_id()))
            .bind(("edge", edge))
            .bind(("ordinal", draft.ordinal))
            .bind(("outbox", outbox))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if let Some(existing) = self
                .segment_by_key(
                    draft.identity.tenant_id,
                    draft.recording_id,
                    &content.segment_key,
                )
                .await?
            {
                validate_existing_segment(&existing, &draft)?;
                return Ok(existing);
            }
            return Err(error.into());
        }
        self.segment(draft.identity.tenant_id, id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "segment creation readback",
            })
    }

    pub async fn freeze_segment(
        &self,
        identity: &PlatformIdentity,
        segment_id: SegmentId,
        byte_len: i64,
        message_count: i64,
        sha256: &str,
        end_time: Option<DateTime<Utc>>,
    ) -> Result<SegmentRecord, StoreError> {
        if byte_len < 0 || message_count < 0 {
            return Err(StoreError::InvalidRecordingField {
                field: "segment metrics",
                reason: "must be non-negative",
            });
        }
        validate_sha256(sha256)?;
        let existing = self.segment(identity.tenant_id, segment_id).await?.ok_or(
            StoreError::MissingRecord {
                operation: "segment freeze",
            },
        )?;
        if existing.state == SegmentState::Frozen
            && existing.byte_len == byte_len
            && existing.message_count == message_count
            && existing.sha256.as_deref() == Some(sha256)
        {
            return Ok(existing);
        }
        if existing.state != SegmentState::Writing {
            return Err(StoreError::SegmentConflict {
                segment_id: segment_id.to_string(),
            });
        }
        let outbox = segment_event(
            identity,
            recording_id_from_record(&existing.recording)?,
            segment_id,
            "recording.segment_frozen",
            SegmentState::Frozen,
        );
        self
            .db
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $segment); IF $current.revision != $revision OR $current.state != 'writing' { THROW 'segment_revision_conflict'; }; UPDATE ONLY $segment SET state = 'frozen', byte_len = $byte_len, message_count = $message_count, sha256 = $sha256, end_time = $end_time, failure_reason = NONE, updated_at = time::now(), revision += 1 RETURN AFTER; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("segment", segment_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("byte_len", byte_len))
            .bind(("message_count", message_count))
            .bind(("sha256", sha256.to_owned()))
            .bind(("end_time", end_time))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        self.segment(identity.tenant_id, segment_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "segment freeze readback",
            })
    }

    pub async fn segment(
        &self,
        tenant_id: TenantId,
        segment_id: SegmentId,
    ) -> Result<Option<SegmentRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $segment WHERE tenant = $tenant;")
            .bind(("segment", segment_id.record_id()))
            .bind(("tenant", tenant_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn segment_by_key(
        &self,
        tenant_id: TenantId,
        recording_id: RecordingId,
        segment_key: &str,
    ) -> Result<Option<SegmentRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM segment WHERE tenant = $tenant AND recording = $recording AND segment_key = $segment_key LIMIT 1;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("recording", recording_id.record_id()))
            .bind(("segment_key", segment_key.to_owned()))
            .await?
            .check()?;
        let records: Vec<SegmentRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }

    pub async fn recording_segments(
        &self,
        tenant_id: TenantId,
        recording_id: RecordingId,
        limit: u32,
    ) -> Result<Vec<SegmentRecord>, StoreError> {
        if limit == 0 || limit > MAX_SEGMENT_LIMIT {
            return Err(StoreError::InvalidRecordingField {
                field: "limit",
                reason: "must be in 1..=10000",
            });
        }
        let mut response = self
            .db
            .query("SELECT * FROM segment WHERE tenant = $tenant AND recording = $recording ORDER BY ordinal ASC LIMIT $limit;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("recording", recording_id.record_id()))
            .bind(("limit", i64::from(limit)))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn begin_recording_seal(
        &self,
        identity: &PlatformIdentity,
        recording_id: RecordingId,
        task_id: Option<TaskId>,
    ) -> Result<RecordingRecord, StoreError> {
        let existing = self
            .recording(identity.tenant_id, recording_id)
            .await?
            .ok_or_else(|| StoreError::RecordingNotFound(recording_id.to_string()))?;
        if existing.state == RecordingState::Sealing
            && existing.seal_task == task_id.map(TaskId::record_id)
        {
            return Ok(existing);
        }
        if existing.state != RecordingState::Open {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: recording_state_name(existing.state).to_owned(),
                target: "sealing",
            });
        }
        let segments = self
            .recording_segments(identity.tenant_id, recording_id, MAX_SEGMENT_LIMIT)
            .await?;
        if segments.is_empty()
            || segments
                .iter()
                .any(|segment| segment.state != SegmentState::Frozen)
        {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: "contains non-frozen segments".to_owned(),
                target: "sealing",
            });
        }
        let outbox = recording_event(
            identity,
            recording_id,
            "recording.sealing",
            RecordingState::Sealing,
        );
        self
            .db
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $recording); IF $current.revision != $revision OR $current.state != 'open' { THROW 'recording_revision_conflict'; }; UPDATE ONLY $recording SET state = 'sealing', seal_task = $task, failure_reason = NONE, updated_at = time::now(), revision += 1 RETURN AFTER; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("recording", recording_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("task", task_id.map(TaskId::record_id)))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        self.recording(identity.tenant_id, recording_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "begin recording seal readback",
            })
    }

    pub async fn complete_recording_seal(
        &self,
        seal: RecordingSeal,
    ) -> Result<RecordingRecord, StoreError> {
        let existing = self
            .recording(seal.identity.tenant_id, seal.recording_id)
            .await?
            .ok_or_else(|| StoreError::RecordingNotFound(seal.recording_id.to_string()))?;
        if existing.state == RecordingState::Sealed
            && existing.manifest_artifact == Some(seal.manifest_artifact_id.record_id())
        {
            return Ok(existing);
        }
        if existing.state != RecordingState::Sealing
            || existing.seal_task != seal.task_id.map(TaskId::record_id)
            || existing.manifest_artifact != Some(seal.manifest_artifact_id.record_id())
        {
            return Err(StoreError::RecordingStateConflict {
                recording_id: seal.recording_id.to_string(),
                state: recording_state_name(existing.state).to_owned(),
                target: "sealed",
            });
        }
        let segments = self
            .recording_segments(
                seal.identity.tenant_id,
                seal.recording_id,
                MAX_SEGMENT_LIMIT,
            )
            .await?;
        if segments.len() != seal.segments.len() {
            return Err(StoreError::RecordingStateConflict {
                recording_id: seal.recording_id.to_string(),
                state: "segment binding count mismatch".to_owned(),
                target: "sealed",
            });
        }
        let by_id: BTreeMap<_, _> = seal
            .segments
            .iter()
            .map(|binding| (binding.segment_id, binding.artifact_id))
            .collect();
        for segment in &segments {
            let segment_id = segment_id_from_record(&segment.id)?;
            if segment.state != SegmentState::Frozen
                || by_id.get(&segment_id).copied().map(ArtifactId::record_id) != segment.artifact
            {
                return Err(StoreError::SegmentConflict {
                    segment_id: segment_id.to_string(),
                });
            }
        }

        let mut sql = String::from(
            "BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $recording); IF $current.revision != $revision OR $current.state != 'sealing' { THROW 'recording_revision_conflict'; };",
        );
        for index in 0..seal.segments.len() {
            sql.push_str(&format!(
                " UPDATE ONLY $segment_{index} SET state = 'sealed', artifact = $artifact_{index}, updated_at = time::now(), revision += 1 RETURN NONE;"
            ));
        }
        sql.push_str(" UPDATE ONLY $recording SET state = 'sealed', manifest_artifact = $manifest, ended_at = $ended_at, failure_reason = NONE, updated_at = time::now(), revision += 1 RETURN AFTER; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;");
        let outbox = recording_event(
            &seal.identity,
            seal.recording_id,
            "recording.sealed",
            RecordingState::Sealed,
        );
        let mut query = self
            .db
            .query(sql)
            .bind(("recording", seal.recording_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("manifest", seal.manifest_artifact_id.record_id()))
            .bind(("ended_at", seal.ended_at))
            .bind(("outbox", outbox));
        for (index, binding) in seal.segments.iter().enumerate() {
            query = query
                .bind((format!("segment_{index}"), binding.segment_id.record_id()))
                .bind((format!("artifact_{index}"), binding.artifact_id.record_id()));
        }
        query.await?.check()?;
        self.recording(seal.identity.tenant_id, seal.recording_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "complete recording seal readback",
            })
    }

    pub async fn stage_segment_artifact(
        &self,
        identity: &PlatformIdentity,
        recording_id: RecordingId,
        segment_id: SegmentId,
        artifact_id: ArtifactId,
    ) -> Result<SegmentRecord, StoreError> {
        let artifact =
            self.artifact_aggregate(artifact_id)
                .await?
                .ok_or(StoreError::MissingRecord {
                    operation: "stage segment artifact occurrence",
                })?;
        if artifact.occurrence.tenant != identity.tenant_id.record_id() {
            return Err(StoreError::SegmentConflict {
                segment_id: segment_id.to_string(),
            });
        }
        let recording = self
            .recording(identity.tenant_id, recording_id)
            .await?
            .ok_or_else(|| StoreError::RecordingNotFound(recording_id.to_string()))?;
        if recording.state != RecordingState::Sealing {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: recording_state_name(recording.state).to_owned(),
                target: "stage segment artifact",
            });
        }
        let segment = self.segment(identity.tenant_id, segment_id).await?.ok_or(
            StoreError::MissingRecord {
                operation: "stage segment artifact",
            },
        )?;
        if segment.recording != recording_id.record_id() || segment.state != SegmentState::Frozen {
            return Err(StoreError::SegmentConflict {
                segment_id: segment_id.to_string(),
            });
        }
        if segment.artifact == Some(artifact_id.record_id()) {
            return Ok(segment);
        }
        if segment.artifact.is_some() {
            return Err(StoreError::SegmentConflict {
                segment_id: segment_id.to_string(),
            });
        }
        let outbox = segment_event(
            identity,
            recording_id,
            segment_id,
            "recording.segment_artifact_staged",
            SegmentState::Frozen,
        );
        self.db
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $segment); IF $current.revision != $revision OR $current.state != 'frozen' OR $current.artifact != NONE { THROW 'segment_revision_conflict'; }; UPDATE ONLY $segment SET artifact = $artifact, updated_at = time::now(), revision += 1 RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("segment", segment_id.record_id()))
            .bind(("revision", segment.revision))
            .bind(("artifact", artifact_id.record_id()))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        self.segment(identity.tenant_id, segment_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "stage segment artifact readback",
            })
    }

    pub async fn stage_recording_manifest(
        &self,
        identity: &PlatformIdentity,
        recording_id: RecordingId,
        artifact_id: ArtifactId,
    ) -> Result<RecordingRecord, StoreError> {
        let artifact =
            self.artifact_aggregate(artifact_id)
                .await?
                .ok_or(StoreError::MissingRecord {
                    operation: "stage recording manifest occurrence",
                })?;
        if artifact.occurrence.tenant != identity.tenant_id.record_id() {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: "manifest artifact belongs to another tenant".to_owned(),
                target: "stage recording manifest",
            });
        }
        let recording = self
            .recording(identity.tenant_id, recording_id)
            .await?
            .ok_or_else(|| StoreError::RecordingNotFound(recording_id.to_string()))?;
        if recording.state != RecordingState::Sealing {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: recording_state_name(recording.state).to_owned(),
                target: "stage recording manifest",
            });
        }
        if recording.manifest_artifact == Some(artifact_id.record_id()) {
            return Ok(recording);
        }
        if recording.manifest_artifact.is_some() {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: "different manifest already staged".to_owned(),
                target: "stage recording manifest",
            });
        }
        let outbox = recording_event(
            identity,
            recording_id,
            "recording.manifest_staged",
            RecordingState::Sealing,
        );
        self.db
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $recording); IF $current.revision != $revision OR $current.state != 'sealing' OR $current.manifest_artifact != NONE { THROW 'recording_revision_conflict'; }; UPDATE ONLY $recording SET manifest_artifact = $artifact, updated_at = time::now(), revision += 1 RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("recording", recording_id.record_id()))
            .bind(("revision", recording.revision))
            .bind(("artifact", artifact_id.record_id()))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        self.recording(identity.tenant_id, recording_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "stage recording manifest readback",
            })
    }

    pub async fn fail_recording_seal(
        &self,
        identity: &PlatformIdentity,
        recording_id: RecordingId,
        reason: &str,
    ) -> Result<RecordingRecord, StoreError> {
        validate_name("failure_reason", reason, 2_048)?;
        let existing = self
            .recording(identity.tenant_id, recording_id)
            .await?
            .ok_or_else(|| StoreError::RecordingNotFound(recording_id.to_string()))?;
        if existing.state != RecordingState::Sealing {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: recording_state_name(existing.state).to_owned(),
                target: "failed",
            });
        }
        let outbox = recording_event(
            identity,
            recording_id,
            "recording.seal_failed",
            RecordingState::Failed,
        );
        self
            .db
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $recording); IF $current.revision != $revision OR $current.state != 'sealing' { THROW 'recording_revision_conflict'; }; UPDATE ONLY $recording SET state = 'failed', failure_reason = $reason, updated_at = time::now(), revision += 1 RETURN AFTER; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("recording", recording_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("reason", reason.to_owned()))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        self.recording(identity.tenant_id, recording_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "fail recording seal readback",
            })
    }
}

fn validate_name(field: &'static str, value: &str, max_bytes: usize) -> Result<(), StoreError> {
    if value.trim().is_empty() {
        return Err(StoreError::InvalidRecordingField {
            field,
            reason: "must not be empty",
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

fn recording_state_name(state: RecordingState) -> &'static str {
    match state {
        RecordingState::Open => "open",
        RecordingState::Sealing => "sealing",
        RecordingState::Sealed => "sealed",
        RecordingState::Failed => "failed",
    }
}

fn normalize_labels(labels: &mut Vec<String>) -> Result<(), StoreError> {
    for label in labels.iter() {
        validate_name("label", label, 256)?;
    }
    labels.sort();
    labels.dedup();
    if labels.len() > 128 {
        return Err(StoreError::InvalidRecordingField {
            field: "labels",
            reason: "must contain at most 128 values",
        });
    }
    Ok(())
}

fn validate_relative_path(value: &str) -> Result<(), StoreError> {
    validate_name("relative_path", value, 4_096)?;
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
            field: "relative_path",
            reason: "must be a normalized relative .rrd path",
        });
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), StoreError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StoreError::InvalidRecordingField {
            field: "sha256",
            reason: "must be 64 hexadecimal characters",
        });
    }
    Ok(())
}

fn validate_existing_recording(
    existing: &RecordingRecord,
    draft: &RecordingDraft,
) -> Result<(), StoreError> {
    if existing.tenant != draft.identity.tenant_id.record_id()
        || existing.owner != draft.identity.principal_id.record_id()
        || existing.dataset != draft.dataset
        || existing.application_id != draft.application_id
        || existing.recording_key != draft.recording_key
        || existing.classification != draft.classification
        || existing.labels != draft.labels
    {
        return Err(StoreError::IdentityConflict {
            entity: "recording",
            key: draft.recording_key.clone(),
        });
    }
    Ok(())
}

fn validate_existing_segment(
    existing: &SegmentRecord,
    draft: &SegmentDraft,
) -> Result<(), StoreError> {
    if existing.tenant != draft.identity.tenant_id.record_id()
        || existing.recording != draft.recording_id.record_id()
        || existing.segment_key != draft.segment_key
        || existing.ordinal != draft.ordinal
        || existing.relative_path != draft.relative_path
    {
        return Err(StoreError::SegmentConflict {
            segment_id: segment_id_from_record(&existing.id)?.to_string(),
        });
    }
    Ok(())
}

fn recording_event(
    identity: &PlatformIdentity,
    recording_id: RecordingId,
    event_type: &str,
    state: RecordingState,
) -> OutboxDraft {
    OutboxDraft::now(
        Some(identity.tenant_id.record_id()),
        "recording",
        recording_id.to_string(),
        event_type,
        EVENT_SCHEMA_VERSION,
        OpenObject::new(BTreeMap::from([
            ("recording_id".to_owned(), serde_json::json!(recording_id)),
            ("state".to_owned(), serde_json::json!(state)),
        ])),
    )
}

fn segment_event(
    identity: &PlatformIdentity,
    recording_id: RecordingId,
    segment_id: SegmentId,
    event_type: &str,
    state: SegmentState,
) -> OutboxDraft {
    OutboxDraft::now(
        Some(identity.tenant_id.record_id()),
        "recording_segment",
        segment_id.to_string(),
        event_type,
        EVENT_SCHEMA_VERSION,
        OpenObject::new(BTreeMap::from([
            ("recording_id".to_owned(), serde_json::json!(recording_id)),
            ("segment_id".to_owned(), serde_json::json!(segment_id)),
            ("state".to_owned(), serde_json::json!(state)),
        ])),
    )
}

fn recording_segment_edge(recording_id: RecordingId, segment_id: SegmentId) -> RecordId {
    let id = Uuid::new_v5(
        &PLATFORM_ID_NAMESPACE,
        format!("recording-segment:{recording_id}:{segment_id}").as_bytes(),
    );
    RecordId::new("recording_segment", SurrealUuid::from(id))
}

fn recording_id_from_record(record: &RecordId) -> Result<RecordingId, StoreError> {
    typed_uuid_from_record(record, RecordingId::TABLE).map(RecordingId::from_uuid)
}

fn segment_id_from_record(record: &RecordId) -> Result<SegmentId, StoreError> {
    typed_uuid_from_record(record, SegmentId::TABLE).map(SegmentId::from_uuid)
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
    fn relative_paths_are_confined_rrd_paths() {
        assert!(validate_relative_path("world/2026-07-09/recording.r1.rrd").is_ok());
        assert!(validate_relative_path("../recording.rrd").is_err());
        assert!(validate_relative_path("/recording.rrd").is_err());
        assert!(validate_relative_path("recording.json").is_err());
    }

    #[test]
    fn label_normalization_is_stable() {
        let mut labels = vec![
            "restricted".to_owned(),
            "operations".to_owned(),
            "restricted".to_owned(),
        ];
        normalize_labels(&mut labels).unwrap();
        assert_eq!(labels, ["operations", "restricted"]);
    }
}
