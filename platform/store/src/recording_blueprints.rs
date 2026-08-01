use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    ArtifactId, PlatformIdentity, PlatformStore, RecordingBlueprintId, RecordingBlueprintRecord,
    RecordingId, RecordingIngestStreamId, RecordingState, StoreError,
};

const MAX_TEXT_LENGTH: usize = 512;

#[derive(Clone, Debug)]
pub struct RecordingBlueprintDraft {
    pub identity: PlatformIdentity,
    pub recording_id: RecordingId,
    pub stream_id: Option<RecordingIngestStreamId>,
    pub work_context: RecordId,
    pub producer_id: String,
    pub application_id: String,
    pub blueprint_id: String,
    pub revision: u64,
    pub relative_path: String,
    pub sha256: String,
    pub byte_len: u64,
    pub message_count: u64,
    pub maximum_revisions: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordingBlueprintOutcome {
    pub blueprint: RecordingBlueprintRecord,
    pub duplicate: bool,
}

#[derive(Clone, Debug)]
pub struct RecordingBlueprintCommit {
    pub draft: RecordingBlueprintDraft,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct RecordingBlueprintContent {
    tenant: RecordId,
    recording: RecordId,
    stream: Option<RecordId>,
    artifact: Option<RecordId>,
    owner: RecordId,
    work_context: RecordId,
    producer_id: String,
    application_id: String,
    blueprint_id: String,
    revision: i64,
    relative_path: String,
    sha256: String,
    byte_len: i64,
    message_count: i64,
    created_at: DateTime<Utc>,
}

impl PlatformStore {
    pub async fn commit_recording_blueprint(
        &self,
        commit: RecordingBlueprintCommit,
    ) -> Result<RecordingBlueprintOutcome, StoreError> {
        validate_draft(&commit.draft)?;
        if let Some(existing) = self
            .recording_blueprint_revision(
                commit.draft.identity.tenant_id,
                commit.draft.recording_id,
                commit.draft.revision,
            )
            .await?
        {
            return duplicate_outcome(existing, &commit.draft);
        }

        let current = self
            .current_recording_blueprint(commit.draft.identity.tenant_id, commit.draft.recording_id)
            .await?;
        let expected = current
            .as_ref()
            .map(|blueprint| {
                u64::try_from(blueprint.revision)
                    .unwrap_or(u64::MAX)
                    .saturating_add(1)
            })
            .unwrap_or(1);
        if commit.draft.revision != expected {
            return Err(StoreError::RecordingBlueprintRevisionGap {
                expected,
                actual: commit.draft.revision,
            });
        }
        if commit.draft.revision > u64::from(commit.draft.maximum_revisions) {
            return Err(StoreError::RecordingIngestQuotaExceeded {
                quota: crate::RecordingIngestQuota::MaximumBlueprintRevisions,
            });
        }

        let blueprint_id = RecordingBlueprintId::new();
        let content = RecordingBlueprintContent {
            tenant: commit.draft.identity.tenant_id.record_id(),
            recording: commit.draft.recording_id.record_id(),
            stream: commit.draft.stream_id.map(|stream| stream.record_id()),
            artifact: None,
            owner: commit.draft.identity.principal_id.record_id(),
            work_context: commit.draft.work_context.clone(),
            producer_id: commit.draft.producer_id.clone(),
            application_id: commit.draft.application_id.clone(),
            blueprint_id: commit.draft.blueprint_id.clone(),
            revision: to_i64("revision", commit.draft.revision)?,
            relative_path: commit.draft.relative_path.clone(),
            sha256: commit.draft.sha256.clone(),
            byte_len: to_i64("byte_len", commit.draft.byte_len)?,
            message_count: to_i64("message_count", commit.draft.message_count)?,
            created_at: commit.created_at,
        };
        let created = self
            .db
            .query("CREATE ONLY $blueprint CONTENT $content RETURN NONE;")
            .bind(("blueprint", blueprint_id.record_id()))
            .bind(("content", content))
            .await
            .and_then(|response| response.check());
        if let Err(error) = created {
            if let Some(existing) = self
                .recording_blueprint_revision(
                    commit.draft.identity.tenant_id,
                    commit.draft.recording_id,
                    commit.draft.revision,
                )
                .await?
            {
                return duplicate_outcome(existing, &commit.draft);
            }
            return Err(error.into());
        }
        let blueprint = self
            .recording_blueprint_revision(
                commit.draft.identity.tenant_id,
                commit.draft.recording_id,
                commit.draft.revision,
            )
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording blueprint creation readback",
            })?;
        Ok(RecordingBlueprintOutcome {
            blueprint,
            duplicate: false,
        })
    }

    pub async fn current_recording_blueprint(
        &self,
        tenant_id: crate::TenantId,
        recording_id: RecordingId,
    ) -> Result<Option<RecordingBlueprintRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM recording_blueprint WHERE tenant = $tenant AND recording = $recording ORDER BY revision DESC LIMIT 1;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("recording", recording_id.record_id()))
            .await?
            .check()?;
        let rows: Vec<RecordingBlueprintRecord> = response.take(0)?;
        Ok(rows.into_iter().next())
    }

    pub async fn recording_blueprint_revision(
        &self,
        tenant_id: crate::TenantId,
        recording_id: RecordingId,
        revision: u64,
    ) -> Result<Option<RecordingBlueprintRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM recording_blueprint WHERE tenant = $tenant AND recording = $recording AND revision = $revision LIMIT 1;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("recording", recording_id.record_id()))
            .bind(("revision", to_i64("revision", revision)?))
            .await?
            .check()?;
        let rows: Vec<RecordingBlueprintRecord> = response.take(0)?;
        Ok(rows.into_iter().next())
    }

    pub async fn stage_recording_blueprint_artifact(
        &self,
        identity: &PlatformIdentity,
        recording_id: RecordingId,
        revision: u64,
        artifact_id: ArtifactId,
    ) -> Result<RecordingBlueprintRecord, StoreError> {
        let artifact =
            self.artifact_aggregate(artifact_id)
                .await?
                .ok_or(StoreError::MissingRecord {
                    operation: "stage recording Blueprint artifact occurrence",
                })?;
        if artifact.occurrence.tenant != identity.tenant_id.record_id() {
            return Err(StoreError::InvalidRecordingIngestField {
                field: "blueprint_artifact",
                reason: "artifact belongs to another tenant",
            });
        }
        let recording = self
            .recording(identity.tenant_id, recording_id)
            .await?
            .ok_or_else(|| StoreError::RecordingNotFound(recording_id.to_string()))?;
        if recording.state != RecordingState::Sealing {
            return Err(StoreError::InvalidRecordingIngestField {
                field: "blueprint_artifact",
                reason: "recording must be sealing",
            });
        }
        let blueprint = self
            .recording_blueprint_revision(identity.tenant_id, recording_id, revision)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "stage recording Blueprint artifact",
            })?;
        if blueprint.artifact == Some(artifact_id.record_id()) {
            return Ok(blueprint);
        }
        if blueprint.artifact.is_some() {
            return Err(StoreError::RecordingBlueprintRevisionConflict { revision });
        }
        self.db
            .query("UPDATE ONLY $blueprint SET artifact = $artifact WHERE artifact = NONE RETURN NONE;")
            .bind(("blueprint", blueprint.id.clone()))
            .bind(("artifact", artifact_id.record_id()))
            .await?
            .check()?;
        let staged = self
            .recording_blueprint_revision(identity.tenant_id, recording_id, revision)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "stage recording Blueprint artifact readback",
            })?;
        if staged.artifact != Some(artifact_id.record_id()) {
            return Err(StoreError::RecordingBlueprintRevisionConflict { revision });
        }
        Ok(staged)
    }
}

fn duplicate_outcome(
    existing: RecordingBlueprintRecord,
    draft: &RecordingBlueprintDraft,
) -> Result<RecordingBlueprintOutcome, StoreError> {
    if existing.sha256 != draft.sha256
        || existing.byte_len != to_i64("byte_len", draft.byte_len)?
        || existing.message_count != to_i64("message_count", draft.message_count)?
        || existing.blueprint_id != draft.blueprint_id
    {
        return Err(StoreError::RecordingBlueprintRevisionConflict {
            revision: draft.revision,
        });
    }
    Ok(RecordingBlueprintOutcome {
        blueprint: existing,
        duplicate: true,
    })
}

fn validate_draft(draft: &RecordingBlueprintDraft) -> Result<(), StoreError> {
    for (field, value) in [
        ("producer_id", draft.producer_id.as_str()),
        ("application_id", draft.application_id.as_str()),
        ("blueprint_id", draft.blueprint_id.as_str()),
        ("relative_path", draft.relative_path.as_str()),
    ] {
        if value.trim().is_empty()
            || value.len() > MAX_TEXT_LENGTH
            || value.chars().any(char::is_control)
        {
            return Err(StoreError::InvalidRecordingIngestField {
                field,
                reason: "value is empty, oversized, or contains control characters",
            });
        }
    }
    if draft.revision == 0
        || draft.byte_len == 0
        || draft.message_count == 0
        || draft.maximum_revisions == 0
        || draft.sha256.len() != 64
        || !draft.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(StoreError::InvalidRecordingIngestField {
            field: "blueprint",
            reason: "revision, sizes, digest, and revision budget must be valid",
        });
    }
    Ok(())
}

fn to_i64(field: &'static str, value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::InvalidRecordingIngestField {
        field,
        reason: "value exceeds signed storage range",
    })
}
