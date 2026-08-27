use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    ArtifactId, InvocationAuthorityRecord, OpenObject, OutboxDraft, PlatformIdentity,
    PlatformStore, RecordingDatasetId, RecordingId, RecordingLayerState, RecordingRecord,
    RecordingState, StoreError, TaskId, TenantId, deterministic_principal_id,
    deterministic_work_context_id,
};

const EVENT_SCHEMA_VERSION: i64 = 1;
const MAX_RECORDING_LIMIT: u32 = 500;
const MAX_RECORDING_LAYER_LIMIT: u32 = 10_000;

#[derive(Clone, Debug)]
pub struct RecordingDraft {
    pub identity: PlatformIdentity,
    pub authority: InvocationAuthorityRecord,
    pub dataset_id: RecordingDatasetId,
    pub application_id: String,
    pub recording_key: String,
    pub classification: String,
    pub labels: Vec<String>,
    pub metadata: BTreeMap<String, serde_json::Value>,
    pub started_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct RecordingSeal {
    pub identity: PlatformIdentity,
    pub recording_id: RecordingId,
    pub task_id: Option<TaskId>,
    pub manifest_artifact_id: ArtifactId,
    pub sealed_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct RecordingContent {
    tenant: RecordId,
    owner: RecordId,
    work_context: RecordId,
    initiator: Option<RecordId>,
    invocation_mode: crate::InvocationMode,
    delegation_id: Option<String>,
    policy_revision: String,
    authority: InvocationAuthorityRecord,
    dataset: RecordId,
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
    last_data_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    sealed_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    revision: i64,
}

impl PlatformStore {
    pub async fn create_recording(
        &self,
        mut draft: RecordingDraft,
    ) -> Result<RecordingRecord, StoreError> {
        validate_name("application_id", &draft.application_id, 512)?;
        validate_name("recording_key", &draft.recording_key, 512)?;
        validate_name("classification", &draft.classification, 256)?;
        normalize_labels(&mut draft.labels)?;

        self.recording_dataset(draft.identity.tenant_id, draft.dataset_id)
            .await?
            .ok_or(StoreError::RecordingDatasetConflict {
                dataset_id: draft.dataset_id.to_string(),
            })?;

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
        let work_context = deterministic_work_context_id(
            &draft.identity.tenant_key,
            &draft.authority.context_key,
        )?;
        let initiator = draft
            .authority
            .initiator_key
            .as_deref()
            .map(|principal| deterministic_principal_id(&draft.identity.tenant_key, principal))
            .transpose()?
            .map(|principal| principal.record_id());
        let content = RecordingContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            work_context: work_context.record_id(),
            initiator,
            invocation_mode: draft.authority.invocation_mode,
            delegation_id: draft.authority.delegation_id.clone(),
            policy_revision: draft.authority.policy_revision.clone(),
            authority: draft.authority.clone(),
            dataset: draft.dataset_id.record_id(),
            application_id: draft.application_id.clone(),
            recording_key: draft.recording_key.clone(),
            state: RecordingState::Live,
            classification: draft.classification.clone(),
            labels: draft.labels.clone(),
            metadata: OpenObject::new(draft.metadata.clone()),
            manifest_artifact: None,
            seal_task: None,
            failure_reason: None,
            started_at: draft.started_at,
            last_data_at: draft.started_at,
            ended_at: None,
            sealed_at: None,
            created_at: now,
            updated_at: now,
            revision: 0,
        };
        let outbox = recording_event(
            &draft.identity,
            id,
            "recording.created",
            RecordingState::Live,
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

    pub async fn finish_recording(
        &self,
        identity: &PlatformIdentity,
        recording_id: RecordingId,
        ended_at: DateTime<Utc>,
    ) -> Result<RecordingRecord, StoreError> {
        let existing = self
            .recording(identity.tenant_id, recording_id)
            .await?
            .ok_or_else(|| StoreError::RecordingNotFound(recording_id.to_string()))?;
        if existing.state == RecordingState::Ready {
            return Ok(existing);
        }
        if existing.state != RecordingState::Live {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: recording_state_name(existing.state).to_owned(),
                target: "ready",
            });
        }
        let layers = self
            .recording_layers(identity.tenant_id, recording_id, MAX_RECORDING_LAYER_LIMIT)
            .await?;
        if layers.is_empty()
            || layers
                .iter()
                .any(|layer| layer.state != RecordingLayerState::Committed)
        {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: "contains non-committed layers".to_owned(),
                target: "ready",
            });
        }
        let ended_at = ended_at.max(existing.last_data_at);
        let outbox = recording_event(
            identity,
            recording_id,
            "recording.ready",
            RecordingState::Ready,
        );
        self.db
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $recording); IF $current.revision != $revision OR $current.state != 'live' { THROW 'recording_revision_conflict'; }; UPDATE ONLY $recording SET state = 'ready', ended_at = $ended_at, failure_reason = NONE, updated_at = time::now(), revision += 1 RETURN AFTER; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("recording", recording_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("ended_at", ended_at))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        self.recording(identity.tenant_id, recording_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "finish recording readback",
            })
    }

    pub async fn interrupt_recording(
        &self,
        identity: &PlatformIdentity,
        recording_id: RecordingId,
        ended_at: DateTime<Utc>,
        reason: &str,
    ) -> Result<RecordingRecord, StoreError> {
        validate_name("failure_reason", reason, 2_048)?;
        let existing = self
            .recording(identity.tenant_id, recording_id)
            .await?
            .ok_or_else(|| StoreError::RecordingNotFound(recording_id.to_string()))?;
        if existing.state == RecordingState::Interrupted {
            return Ok(existing);
        }
        if existing.state != RecordingState::Live {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: recording_state_name(existing.state).to_owned(),
                target: "interrupted",
            });
        }
        let layers = self
            .recording_layers(identity.tenant_id, recording_id, MAX_RECORDING_LAYER_LIMIT)
            .await?;
        if layers
            .iter()
            .any(|layer| layer.state != RecordingLayerState::Committed)
        {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: "contains non-committed layers".to_owned(),
                target: "interrupted",
            });
        }
        let ended_at = ended_at.max(existing.last_data_at);
        let outbox = recording_event(
            identity,
            recording_id,
            "recording.interrupted",
            RecordingState::Interrupted,
        );
        self.db
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $recording); IF $current.revision != $revision OR $current.state != 'live' { THROW 'recording_revision_conflict'; }; UPDATE ONLY $recording SET state = 'interrupted', ended_at = $ended_at, failure_reason = $reason, updated_at = time::now(), revision += 1 RETURN AFTER; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("recording", recording_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("ended_at", ended_at))
            .bind(("reason", reason.to_owned()))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        self.recording(identity.tenant_id, recording_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "interrupt recording readback",
            })
    }

    pub(crate) async fn resume_recording(
        &self,
        identity: &PlatformIdentity,
        recording_id: RecordingId,
    ) -> Result<RecordingRecord, StoreError> {
        let existing = self
            .recording(identity.tenant_id, recording_id)
            .await?
            .ok_or_else(|| StoreError::RecordingNotFound(recording_id.to_string()))?;
        if existing.state == RecordingState::Live {
            return Ok(existing);
        }
        if !matches!(
            existing.state,
            RecordingState::Ready | RecordingState::Interrupted
        ) {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: recording_state_name(existing.state).to_owned(),
                target: "live",
            });
        }
        let outbox = recording_event(
            identity,
            recording_id,
            "recording.resumed",
            RecordingState::Live,
        );
        self.db
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $recording); IF $current.revision != $revision OR $current.state NOT IN ['ready', 'interrupted'] { THROW 'recording_revision_conflict'; }; UPDATE ONLY $recording SET state = 'live', ended_at = NONE, sealed_at = NONE, failure_reason = NONE, updated_at = time::now(), revision += 1 RETURN AFTER; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("recording", recording_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        self.recording(identity.tenant_id, recording_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "resume recording readback",
            })
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
        if !matches!(
            existing.state,
            RecordingState::Ready | RecordingState::Interrupted
        ) {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: recording_state_name(existing.state).to_owned(),
                target: "sealing",
            });
        }
        let layers = self
            .recording_layers(identity.tenant_id, recording_id, MAX_RECORDING_LAYER_LIMIT)
            .await?;
        if layers.is_empty()
            || layers
                .iter()
                .any(|layer| layer.state != RecordingLayerState::Committed)
        {
            return Err(StoreError::RecordingStateConflict {
                recording_id: recording_id.to_string(),
                state: "contains non-committed layers".to_owned(),
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
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $recording); IF $current.revision != $revision OR $current.state NOT IN ['ready', 'interrupted'] { THROW 'recording_revision_conflict'; }; UPDATE ONLY $recording SET state = 'sealing', seal_task = $task, failure_reason = NONE, updated_at = time::now(), revision += 1 RETURN AFTER; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
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
        let layers = self
            .recording_layers(
                seal.identity.tenant_id,
                seal.recording_id,
                MAX_RECORDING_LAYER_LIMIT,
            )
            .await?;
        if layers.is_empty()
            || layers
                .iter()
                .any(|layer| layer.state != RecordingLayerState::Committed)
        {
            return Err(StoreError::RecordingStateConflict {
                recording_id: seal.recording_id.to_string(),
                state: "contains non-committed layers".to_owned(),
                target: "sealed",
            });
        }
        let outbox = recording_event(
            &seal.identity,
            seal.recording_id,
            "recording.sealed",
            RecordingState::Sealed,
        );
        self.db
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $recording); IF $current.revision != $revision OR $current.state != 'sealing' OR $current.manifest_artifact != $manifest { THROW 'recording_revision_conflict'; }; UPDATE ONLY $recording SET state = 'sealed', sealed_at = $sealed_at, failure_reason = NONE, updated_at = time::now(), revision += 1 RETURN NONE; CREATE outbox_event CONTENT $outbox RETURN NONE; COMMIT TRANSACTION;")
            .bind(("recording", seal.recording_id.record_id()))
            .bind(("revision", existing.revision))
            .bind(("manifest", seal.manifest_artifact_id.record_id()))
            .bind(("sealed_at", seal.sealed_at))
            .bind(("outbox", outbox))
            .await?
            .check()?;
        self.recording(seal.identity.tenant_id, seal.recording_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "complete recording seal readback",
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
        RecordingState::Live => "live",
        RecordingState::Ready => "ready",
        RecordingState::Sealing => "sealing",
        RecordingState::Sealed => "sealed",
        RecordingState::Interrupted => "interrupted",
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

fn validate_existing_recording(
    existing: &RecordingRecord,
    draft: &RecordingDraft,
) -> Result<(), StoreError> {
    if existing.tenant != draft.identity.tenant_id.record_id()
        || existing.owner != draft.identity.principal_id.record_id()
        || existing.work_context
            != deterministic_work_context_id(
                &draft.identity.tenant_key,
                &draft.authority.context_key,
            )?
            .record_id()
        || existing.initiator
            != draft
                .authority
                .initiator_key
                .as_deref()
                .map(|principal| deterministic_principal_id(&draft.identity.tenant_key, principal))
                .transpose()?
                .map(|principal| principal.record_id())
        || existing.invocation_mode != draft.authority.invocation_mode
        || existing.delegation_id != draft.authority.delegation_id
        || existing.policy_revision != draft.authority.policy_revision
        || existing.authority != draft.authority
        || existing.dataset != draft.dataset_id.record_id()
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

#[cfg(test)]
mod tests {
    use super::*;

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
