use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{
    PlatformIdentity, PlatformStore, RecordingId, RecordingIngestBatchId,
    RecordingIngestBatchRecord, RecordingIngestBatchState, RecordingIngestStreamId,
    RecordingIngestStreamRecord, RecordingIngestStreamState, StoreError, TenantId,
};

const MAX_TEXT_LENGTH: usize = 512;

#[derive(Clone, Debug)]
pub struct RecordingIngestStreamDraft {
    pub identity: PlatformIdentity,
    pub recording_id: RecordingId,
    pub producer_id: String,
    pub oauth_client_id: String,
    pub source_stream_id: String,
    pub application_id: String,
    pub recording_key: String,
    pub dataset: String,
    pub maximum_concurrent_streams: u32,
}

#[derive(Clone, Debug)]
pub struct RecordingIngestBatchDraft {
    pub identity: PlatformIdentity,
    pub stream_id: RecordingIngestStreamId,
    pub sequence: u64,
    pub payload_format: String,
    pub sha256: String,
    pub relative_path: String,
    pub byte_len: u64,
    pub message_count: u64,
    pub producer_id: String,
    pub maximum_batches_per_minute: u32,
    pub maximum_bytes_per_day: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecordingIngestAppendOutcome {
    pub stream: RecordingIngestStreamRecord,
    pub batch: RecordingIngestBatchRecord,
    pub duplicate: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct RecordingIngestStreamContent {
    tenant: RecordId,
    owner: RecordId,
    recording: RecordId,
    producer_id: String,
    oauth_client_id: String,
    source_stream_id: String,
    application_id: String,
    recording_key: String,
    dataset: String,
    state: RecordingIngestStreamState,
    next_sequence: i64,
    materialized_through_sequence: Option<i64>,
    byte_len: i64,
    message_count: i64,
    failure_reason: Option<String>,
    opened_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
    revision: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct RecordingIngestBatchContent {
    tenant: RecordId,
    stream: RecordId,
    producer_id: String,
    sequence: i64,
    payload_format: String,
    sha256: String,
    relative_path: String,
    byte_len: i64,
    message_count: i64,
    state: RecordingIngestBatchState,
    created_at: DateTime<Utc>,
    materialized_at: Option<DateTime<Utc>>,
}

impl PlatformStore {
    pub async fn open_recording_ingest_stream(
        &self,
        draft: RecordingIngestStreamDraft,
    ) -> Result<RecordingIngestStreamRecord, StoreError> {
        validate_stream_draft(&draft)?;
        if let Some(existing) = self
            .recording_ingest_stream_by_source(
                draft.identity.tenant_id,
                &draft.producer_id,
                &draft.source_stream_id,
            )
            .await?
        {
            validate_existing_stream(&existing, &draft)?;
            return Ok(existing);
        }

        let stream_id = RecordingIngestStreamId::new();
        let now = Utc::now();
        let content = RecordingIngestStreamContent {
            tenant: draft.identity.tenant_id.record_id(),
            owner: draft.identity.principal_id.record_id(),
            recording: draft.recording_id.record_id(),
            producer_id: draft.producer_id.clone(),
            oauth_client_id: draft.oauth_client_id.clone(),
            source_stream_id: draft.source_stream_id.clone(),
            application_id: draft.application_id.clone(),
            recording_key: draft.recording_key.clone(),
            dataset: draft.dataset.clone(),
            state: RecordingIngestStreamState::Open,
            next_sequence: 1,
            materialized_through_sequence: None,
            byte_len: 0,
            message_count: 0,
            failure_reason: None,
            opened_at: now,
            finished_at: None,
            updated_at: now,
            revision: 0,
        };
        let created = self
            .db
            .query("BEGIN TRANSACTION; LET $open_streams = (SELECT VALUE id FROM recording_ingest_stream WHERE tenant = $tenant AND producer_id = $producer_id AND state = 'open'); IF array::len($open_streams) >= $maximum_concurrent_streams { THROW 'recording_ingest_concurrent_stream_quota'; }; CREATE ONLY $stream CONTENT $content RETURN NONE; COMMIT TRANSACTION;")
            .bind(("stream", stream_id.record_id()))
            .bind(("content", content))
            .bind(("tenant", draft.identity.tenant_id.record_id()))
            .bind(("producer_id", draft.producer_id.clone()))
            .bind((
                "maximum_concurrent_streams",
                i64::from(draft.maximum_concurrent_streams),
            ))
            .await
            .and_then(|response| response.check());
        if let Err(error) = created {
            if let Some(existing) = self
                .recording_ingest_stream_by_source(
                    draft.identity.tenant_id,
                    &draft.producer_id,
                    &draft.source_stream_id,
                )
                .await?
            {
                validate_existing_stream(&existing, &draft)?;
                return Ok(existing);
            }
            return Err(classify_database_error(error));
        }
        self.recording_ingest_stream(draft.identity.tenant_id, stream_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording ingest stream creation readback",
            })
    }

    pub async fn recording_ingest_stream(
        &self,
        tenant_id: TenantId,
        stream_id: RecordingIngestStreamId,
    ) -> Result<Option<RecordingIngestStreamRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $stream WHERE tenant = $tenant;")
            .bind(("stream", stream_id.record_id()))
            .bind(("tenant", tenant_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn recording_ingest_stream_by_source(
        &self,
        tenant_id: TenantId,
        producer_id: &str,
        source_stream_id: &str,
    ) -> Result<Option<RecordingIngestStreamRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM recording_ingest_stream WHERE tenant = $tenant AND producer_id = $producer_id AND source_stream_id = $source_stream_id LIMIT 1;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("producer_id", producer_id.to_owned()))
            .bind(("source_stream_id", source_stream_id.to_owned()))
            .await?
            .check()?;
        let streams: Vec<RecordingIngestStreamRecord> = response.take(0)?;
        Ok(streams.into_iter().next())
    }

    pub async fn commit_recording_ingest_batch(
        &self,
        draft: RecordingIngestBatchDraft,
    ) -> Result<RecordingIngestAppendOutcome, StoreError> {
        validate_batch_draft(&draft)?;
        let stream = self
            .recording_ingest_stream(draft.identity.tenant_id, draft.stream_id)
            .await?
            .ok_or_else(|| {
                StoreError::RecordingIngestStreamNotFound(draft.stream_id.to_string())
            })?;
        classify_sequence(&stream, &draft, self).await?;
        if draft.sequence < u64::try_from(stream.next_sequence).unwrap_or_default() {
            return duplicate_outcome(stream, &draft, self).await;
        }

        let batch_id = RecordingIngestBatchId::new();
        let now = Utc::now();
        let sequence = checked_i64("sequence", draft.sequence)?;
        let byte_len = checked_i64("byte_len", draft.byte_len)?;
        let message_count = checked_i64("message_count", draft.message_count)?;
        let content = RecordingIngestBatchContent {
            tenant: draft.identity.tenant_id.record_id(),
            stream: draft.stream_id.record_id(),
            producer_id: draft.producer_id.clone(),
            sequence,
            payload_format: draft.payload_format.clone(),
            sha256: draft.sha256.clone(),
            relative_path: draft.relative_path.clone(),
            byte_len,
            message_count,
            state: RecordingIngestBatchState::Durable,
            created_at: now,
            materialized_at: None,
        };
        let committed = self
            .db
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $stream); IF $current.state != 'open' OR $current.revision != $revision OR $current.next_sequence != $sequence { THROW 'recording_ingest_checkpoint_conflict'; }; LET $minute_batches = (SELECT VALUE id FROM recording_ingest_batch WHERE tenant = $tenant AND producer_id = $producer_id AND created_at >= $minute_cutoff); IF array::len($minute_batches) >= $maximum_batches_per_minute { THROW 'recording_ingest_batches_per_minute_quota'; }; LET $day_bytes = (SELECT VALUE byte_len FROM recording_ingest_batch WHERE tenant = $tenant AND producer_id = $producer_id AND created_at >= $day_cutoff); IF math::sum($day_bytes) + $byte_len > $maximum_bytes_per_day { THROW 'recording_ingest_bytes_per_day_quota'; }; CREATE ONLY $batch CONTENT $content RETURN NONE; UPDATE ONLY $stream SET next_sequence += 1, byte_len += $byte_len, message_count += $message_count, updated_at = $now, revision += 1 RETURN NONE; COMMIT TRANSACTION;")
            .bind(("stream", draft.stream_id.record_id()))
            .bind(("revision", stream.revision))
            .bind(("sequence", sequence))
            .bind(("batch", batch_id.record_id()))
            .bind(("content", content))
            .bind(("byte_len", byte_len))
            .bind(("message_count", message_count))
            .bind(("now", now))
            .bind(("tenant", draft.identity.tenant_id.record_id()))
            .bind(("producer_id", draft.producer_id.clone()))
            .bind(("minute_cutoff", now - chrono::TimeDelta::minutes(1)))
            .bind(("day_cutoff", now - chrono::TimeDelta::days(1)))
            .bind((
                "maximum_batches_per_minute",
                i64::from(draft.maximum_batches_per_minute),
            ))
            .bind((
                "maximum_bytes_per_day",
                checked_i64("maximum_bytes_per_day", draft.maximum_bytes_per_day)?,
            ))
            .await
            .and_then(|response| response.check());
        if let Err(error) = committed {
            let current = self
                .recording_ingest_stream(draft.identity.tenant_id, draft.stream_id)
                .await?
                .ok_or_else(|| {
                    StoreError::RecordingIngestStreamNotFound(draft.stream_id.to_string())
                })?;
            if draft.sequence < u64::try_from(current.next_sequence).unwrap_or_default() {
                return duplicate_outcome(current, &draft, self).await;
            }
            classify_sequence(&current, &draft, self).await?;
            return Err(classify_database_error(error));
        }
        let stream = self
            .recording_ingest_stream(draft.identity.tenant_id, draft.stream_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording ingest stream checkpoint readback",
            })?;
        let batch = self
            .recording_ingest_batch(draft.identity.tenant_id, draft.stream_id, draft.sequence)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording ingest batch creation readback",
            })?;
        Ok(RecordingIngestAppendOutcome {
            stream,
            batch,
            duplicate: false,
        })
    }

    pub async fn recording_ingest_batch(
        &self,
        tenant_id: TenantId,
        stream_id: RecordingIngestStreamId,
        sequence: u64,
    ) -> Result<Option<RecordingIngestBatchRecord>, StoreError> {
        let sequence = checked_i64("sequence", sequence)?;
        let mut response = self
            .db
            .query("SELECT * FROM recording_ingest_batch WHERE tenant = $tenant AND stream = $stream AND sequence = $sequence LIMIT 1;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("stream", stream_id.record_id()))
            .bind(("sequence", sequence))
            .await?
            .check()?;
        let batches: Vec<RecordingIngestBatchRecord> = response.take(0)?;
        Ok(batches.into_iter().next())
    }

    pub async fn durable_recording_ingest_batches(
        &self,
        tenant_id: TenantId,
        stream_id: RecordingIngestStreamId,
        limit: u32,
    ) -> Result<Vec<RecordingIngestBatchRecord>, StoreError> {
        if limit == 0 || limit > 10_000 {
            return Err(StoreError::InvalidRecordingIngestField {
                field: "limit",
                reason: "must be in 1..=10000",
            });
        }
        let mut response = self
            .db
            .query("SELECT * FROM recording_ingest_batch WHERE tenant = $tenant AND stream = $stream AND state = 'durable' ORDER BY sequence ASC LIMIT $limit;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("stream", stream_id.record_id()))
            .bind(("limit", i64::from(limit)))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    pub async fn mark_recording_ingest_materialized(
        &self,
        tenant_id: TenantId,
        stream_id: RecordingIngestStreamId,
        through_sequence: u64,
    ) -> Result<RecordingIngestStreamRecord, StoreError> {
        let through = checked_i64("through_sequence", through_sequence)?;
        let stream = self
            .recording_ingest_stream(tenant_id, stream_id)
            .await?
            .ok_or_else(|| StoreError::RecordingIngestStreamNotFound(stream_id.to_string()))?;
        if through >= stream.next_sequence {
            return Err(StoreError::InvalidRecordingIngestField {
                field: "through_sequence",
                reason: "must identify a durable batch",
            });
        }
        self.db
            .query("BEGIN TRANSACTION; UPDATE recording_ingest_batch SET state = 'materialized', materialized_at = $now WHERE tenant = $tenant AND stream = $stream AND sequence <= $through AND state = 'durable' RETURN NONE; UPDATE ONLY $stream SET materialized_through_sequence = $through, updated_at = $now, revision += 1 WHERE materialized_through_sequence = NONE OR materialized_through_sequence < $through RETURN NONE; COMMIT TRANSACTION;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("stream", stream_id.record_id()))
            .bind(("through", through))
            .bind(("now", Utc::now()))
            .await?
            .check()?;
        self.recording_ingest_stream(tenant_id, stream_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording ingest materialization readback",
            })
    }

    pub async fn finish_recording_ingest_stream(
        &self,
        tenant_id: TenantId,
        stream_id: RecordingIngestStreamId,
    ) -> Result<RecordingIngestStreamRecord, StoreError> {
        let stream = self
            .recording_ingest_stream(tenant_id, stream_id)
            .await?
            .ok_or_else(|| StoreError::RecordingIngestStreamNotFound(stream_id.to_string()))?;
        if stream.state == RecordingIngestStreamState::Finished {
            return Ok(stream);
        }
        if stream.state != RecordingIngestStreamState::Open {
            return Err(StoreError::RecordingIngestStreamStateConflict {
                stream_id: stream_id.to_string(),
                state: "failed".to_owned(),
            });
        }
        self.db
            .query("UPDATE ONLY $stream SET state = 'finished', finished_at = $now, updated_at = $now, revision += 1 WHERE tenant = $tenant AND state = 'open' AND revision = $revision RETURN NONE;")
            .bind(("stream", stream_id.record_id()))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("revision", stream.revision))
            .bind(("now", Utc::now()))
            .await?
            .check()?;
        self.recording_ingest_stream(tenant_id, stream_id)
            .await?
            .ok_or(StoreError::MissingRecord {
                operation: "recording ingest finish readback",
            })
    }
}

async fn classify_sequence(
    stream: &RecordingIngestStreamRecord,
    draft: &RecordingIngestBatchDraft,
    _store: &PlatformStore,
) -> Result<(), StoreError> {
    if stream.state != RecordingIngestStreamState::Open {
        return Err(StoreError::RecordingIngestStreamStateConflict {
            stream_id: draft.stream_id.to_string(),
            state: match stream.state {
                RecordingIngestStreamState::Open => "open",
                RecordingIngestStreamState::Finished => "finished",
                RecordingIngestStreamState::Failed => "failed",
            }
            .to_owned(),
        });
    }
    let expected = u64::try_from(stream.next_sequence).map_err(|_| {
        StoreError::InvalidRecordingIngestField {
            field: "next_sequence",
            reason: "must be non-negative",
        }
    })?;
    if draft.sequence > expected {
        return Err(StoreError::RecordingIngestSequenceGap {
            expected,
            actual: draft.sequence,
        });
    }
    Ok(())
}

async fn duplicate_outcome(
    stream: RecordingIngestStreamRecord,
    draft: &RecordingIngestBatchDraft,
    store: &PlatformStore,
) -> Result<RecordingIngestAppendOutcome, StoreError> {
    let batch = store
        .recording_ingest_batch(draft.identity.tenant_id, draft.stream_id, draft.sequence)
        .await?
        .ok_or(StoreError::RecordingIngestDigestConflict {
            sequence: draft.sequence,
        })?;
    if batch.sha256 != draft.sha256
        || batch.payload_format != draft.payload_format
        || batch.byte_len != checked_i64("byte_len", draft.byte_len)?
        || batch.message_count != checked_i64("message_count", draft.message_count)?
    {
        return Err(StoreError::RecordingIngestDigestConflict {
            sequence: draft.sequence,
        });
    }
    Ok(RecordingIngestAppendOutcome {
        stream,
        batch,
        duplicate: true,
    })
}

fn validate_stream_draft(draft: &RecordingIngestStreamDraft) -> Result<(), StoreError> {
    for (field, value) in [
        ("producer_id", draft.producer_id.as_str()),
        ("oauth_client_id", draft.oauth_client_id.as_str()),
        ("source_stream_id", draft.source_stream_id.as_str()),
        ("application_id", draft.application_id.as_str()),
        ("recording_key", draft.recording_key.as_str()),
        ("dataset", draft.dataset.as_str()),
    ] {
        validate_text(field, value)?;
    }
    if draft.maximum_concurrent_streams == 0 {
        return Err(StoreError::InvalidRecordingIngestField {
            field: "maximum_concurrent_streams",
            reason: "must be positive",
        });
    }
    Ok(())
}

fn validate_existing_stream(
    existing: &RecordingIngestStreamRecord,
    draft: &RecordingIngestStreamDraft,
) -> Result<(), StoreError> {
    if existing.tenant != draft.identity.tenant_id.record_id()
        || existing.owner != draft.identity.principal_id.record_id()
        || existing.recording != draft.recording_id.record_id()
        || existing.oauth_client_id != draft.oauth_client_id
        || existing.application_id != draft.application_id
        || existing.recording_key != draft.recording_key
        || existing.dataset != draft.dataset
    {
        return Err(StoreError::InvalidRecordingIngestField {
            field: "source_stream_id",
            reason: "was reused with different immutable stream identity",
        });
    }
    Ok(())
}

fn validate_batch_draft(draft: &RecordingIngestBatchDraft) -> Result<(), StoreError> {
    validate_text("payload_format", &draft.payload_format)?;
    validate_text("relative_path", &draft.relative_path)?;
    validate_text("producer_id", &draft.producer_id)?;
    if draft.relative_path.starts_with('/') || draft.relative_path.contains("..") {
        return Err(StoreError::InvalidRecordingIngestField {
            field: "relative_path",
            reason: "must be a normalized relative path",
        });
    }
    if draft.sha256.len() != 64 || !draft.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StoreError::InvalidRecordingIngestField {
            field: "sha256",
            reason: "must be 64 hexadecimal characters",
        });
    }
    if draft.byte_len == 0 || draft.message_count == 0 {
        return Err(StoreError::InvalidRecordingIngestField {
            field: "batch",
            reason: "byte_len and message_count must be positive",
        });
    }
    checked_i64("sequence", draft.sequence)?;
    checked_i64("byte_len", draft.byte_len)?;
    checked_i64("message_count", draft.message_count)?;
    if draft.maximum_batches_per_minute == 0 || draft.maximum_bytes_per_day == 0 {
        return Err(StoreError::InvalidRecordingIngestField {
            field: "producer_quotas",
            reason: "must be positive",
        });
    }
    checked_i64("maximum_bytes_per_day", draft.maximum_bytes_per_day)?;
    Ok(())
}

fn classify_database_error(error: surrealdb::Error) -> StoreError {
    let message = error.to_string();
    if message.contains("recording_ingest_concurrent_stream_quota") {
        StoreError::RecordingIngestQuotaExceeded {
            quota: "maximum_concurrent_streams",
        }
    } else if message.contains("recording_ingest_batches_per_minute_quota") {
        StoreError::RecordingIngestQuotaExceeded {
            quota: "maximum_batches_per_minute",
        }
    } else if message.contains("recording_ingest_bytes_per_day_quota") {
        StoreError::RecordingIngestQuotaExceeded {
            quota: "maximum_bytes_per_day",
        }
    } else if message.contains("recording_ingest_checkpoint_conflict") {
        StoreError::RecordingIngestCheckpointConflict
    } else {
        StoreError::Database(error)
    }
}

fn validate_text(field: &'static str, value: &str) -> Result<(), StoreError> {
    if value.trim().is_empty()
        || value.len() > MAX_TEXT_LENGTH
        || value.chars().any(char::is_control)
    {
        return Err(StoreError::InvalidRecordingIngestField {
            field,
            reason: "must be non-empty, bounded text without control characters",
        });
    }
    Ok(())
}

fn checked_i64(field: &'static str, value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::InvalidRecordingIngestField {
        field,
        reason: "exceeds the persistence range",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_validation_rejects_traversal_and_bad_digests() {
        let identity = PlatformIdentity {
            tenant_id: TenantId::new(),
            principal_id: crate::PrincipalId::new(),
            tenant_key: "tenant-a".to_owned(),
            principal_key: "producer-a".to_owned(),
        };
        let mut draft = RecordingIngestBatchDraft {
            identity,
            stream_id: RecordingIngestStreamId::new(),
            sequence: 0,
            payload_format: "rrd_0_34_1".to_owned(),
            sha256: "a".repeat(64),
            relative_path: "journal/stream/00000000000000000000.rrd".to_owned(),
            byte_len: 1,
            message_count: 1,
            producer_id: "producer-a".to_owned(),
            maximum_batches_per_minute: 60,
            maximum_bytes_per_day: 1_000_000,
        };
        assert!(validate_batch_draft(&draft).is_ok());
        draft.relative_path = "../outside".to_owned();
        assert!(validate_batch_draft(&draft).is_err());
        draft.relative_path = "journal/batch.rrd".to_owned();
        draft.sha256 = "not-a-digest".to_owned();
        assert!(validate_batch_draft(&draft).is_err());
    }
}
