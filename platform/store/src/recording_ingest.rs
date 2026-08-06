use chrono::{DateTime, TimeDelta, Timelike, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types as surrealdb_types;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use crate::{
    PlatformIdentity, PlatformStore, RecordingId, RecordingIngestBatchId,
    RecordingIngestBatchRecord, RecordingIngestBatchState, RecordingIngestQuota,
    RecordingIngestQuotaWindowId, RecordingIngestStreamId, RecordingIngestStreamRecord,
    RecordingIngestStreamState, StoreError, TenantId,
};

const MAX_TEXT_LENGTH: usize = 512;
const MAX_SUPERSEDED_STREAMS: i64 = 512;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingIngestQuotaCheckpoint {
    minute: RecordingIngestQuotaWindow,
    day: RecordingIngestQuotaWindow,
}

impl RecordingIngestQuotaCheckpoint {
    pub fn is_current(&self, at: DateTime<Utc>) -> bool {
        self.minute.contains(at) && self.day.contains(at)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordingIngestQuotaWindow {
    id: RecordingIngestQuotaWindowId,
    period: RecordingIngestQuotaPeriod,
    started_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
}

impl RecordingIngestQuotaWindow {
    fn new(
        tenant_id: TenantId,
        producer_id: &str,
        period: RecordingIngestQuotaPeriod,
        started_at: DateTime<Utc>,
        ends_at: DateTime<Utc>,
    ) -> Self {
        let key = format!(
            "{}|{}|{}|{}",
            tenant_id,
            producer_id,
            period.name(),
            started_at.to_rfc3339()
        );
        Self {
            id: RecordingIngestQuotaWindowId::from_uuid(Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                key.as_bytes(),
            )),
            period,
            started_at,
            ends_at,
        }
    }

    fn contains(&self, at: DateTime<Utc>) -> bool {
        self.started_at <= at && at < self.ends_at
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, SurrealValue)]
#[surreal(untagged)]
enum RecordingIngestQuotaPeriod {
    #[serde(rename = "minute")]
    #[surreal(value = "minute")]
    Minute,
    #[serde(rename = "day")]
    #[surreal(value = "day")]
    Day,
}

impl RecordingIngestQuotaPeriod {
    const fn name(self) -> &'static str {
        match self {
            Self::Minute => "minute",
            Self::Day => "day",
        }
    }
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

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct RecordingIngestQuotaWindowContent {
    tenant: RecordId,
    producer_id: String,
    period: RecordingIngestQuotaPeriod,
    window_started_at: DateTime<Utc>,
    window_ends_at: DateTime<Utc>,
    batch_count: i64,
    byte_len: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct RecordingIngestQuotaWindowRecord {
    id: RecordId,
    tenant: RecordId,
    producer_id: String,
    period: RecordingIngestQuotaPeriod,
    window_started_at: DateTime<Utc>,
    window_ends_at: DateTime<Utc>,
    batch_count: i64,
    byte_len: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, SurrealValue)]
struct RecordingIngestQuotaUsage {
    batch_count: i64,
    byte_len: Option<i64>,
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

    pub async fn superseded_recording_ingest_streams(
        &self,
        tenant_id: TenantId,
        producer_id: &str,
        application_id: &str,
        current_recording_key: &str,
    ) -> Result<Vec<RecordingIngestStreamRecord>, StoreError> {
        validate_text("producer_id", producer_id)?;
        validate_text("application_id", application_id)?;
        validate_text("recording_key", current_recording_key)?;
        let mut response = self
            .db
            .query("SELECT * FROM recording_ingest_stream WHERE tenant = $tenant AND producer_id = $producer_id AND application_id = $application_id AND recording_key != $recording_key AND recording IN (SELECT VALUE id FROM recording WHERE tenant = $tenant AND application_id = $application_id AND state = 'live') ORDER BY opened_at ASC LIMIT $limit;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("producer_id", producer_id.to_owned()))
            .bind(("application_id", application_id.to_owned()))
            .bind(("recording_key", current_recording_key.to_owned()))
            .bind(("limit", MAX_SUPERSEDED_STREAMS + 1))
            .await?
            .check()?;
        let streams: Vec<RecordingIngestStreamRecord> = response.take(0)?;
        if streams.len() > usize::try_from(MAX_SUPERSEDED_STREAMS).unwrap_or(usize::MAX) {
            return Err(StoreError::InvalidRecordingIngestField {
                field: "superseded_streams",
                reason: "exceeds the bounded reconciliation limit",
            });
        }
        Ok(streams)
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
        self.commit_recording_ingest_batch_at_checkpoint(stream, draft)
            .await
    }

    /// Commits one batch against a checkpoint already held by the serialized
    /// recording materializer.
    ///
    /// The database transaction still compares the checkpoint revision and
    /// sequence. A successful commit is reconstructed from the exact values
    /// written by that transaction instead of issuing redundant readbacks.
    pub async fn commit_recording_ingest_batch_at_checkpoint(
        &self,
        stream: RecordingIngestStreamRecord,
        draft: RecordingIngestBatchDraft,
    ) -> Result<RecordingIngestAppendOutcome, StoreError> {
        let quota = self
            .recording_ingest_quota_checkpoint(
                draft.identity.tenant_id,
                &draft.producer_id,
                Utc::now(),
            )
            .await?;
        self.commit_recording_ingest_batch_at_checkpoints(stream, quota, draft)
            .await
    }

    pub async fn recording_ingest_quota_checkpoint(
        &self,
        tenant_id: TenantId,
        producer_id: &str,
        at: DateTime<Utc>,
    ) -> Result<RecordingIngestQuotaCheckpoint, StoreError> {
        validate_text("producer_id", producer_id)?;
        let minute_start = at
            .with_second(0)
            .and_then(|value| value.with_nanosecond(0))
            .ok_or(StoreError::InvalidRecordingIngestField {
                field: "quota_time",
                reason: "could not derive a UTC minute boundary",
            })?;
        let day_start = at
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .map(|value| value.and_utc())
            .ok_or(StoreError::InvalidRecordingIngestField {
                field: "quota_time",
                reason: "could not derive a UTC day boundary",
            })?;
        let minute = RecordingIngestQuotaWindow::new(
            tenant_id,
            producer_id,
            RecordingIngestQuotaPeriod::Minute,
            minute_start,
            minute_start + TimeDelta::minutes(1),
        );
        let day = RecordingIngestQuotaWindow::new(
            tenant_id,
            producer_id,
            RecordingIngestQuotaPeriod::Day,
            day_start,
            day_start + TimeDelta::days(1),
        );
        self.ensure_recording_ingest_quota_window(tenant_id, producer_id, &minute)
            .await?;
        self.ensure_recording_ingest_quota_window(tenant_id, producer_id, &day)
            .await?;
        Ok(RecordingIngestQuotaCheckpoint { minute, day })
    }

    /// Commits one batch against stream and quota checkpoints already held by
    /// the serialized materializer. The quota records remain database-atomic,
    /// while ordinary appends avoid rediscovering their fixed UTC windows.
    pub async fn commit_recording_ingest_batch_at_checkpoints(
        &self,
        mut stream: RecordingIngestStreamRecord,
        quota: RecordingIngestQuotaCheckpoint,
        draft: RecordingIngestBatchDraft,
    ) -> Result<RecordingIngestAppendOutcome, StoreError> {
        validate_batch_draft(&draft)?;
        if stream.id != draft.stream_id.record_id()
            || stream.tenant != draft.identity.tenant_id.record_id()
        {
            return Err(StoreError::RecordingIngestStreamNotFound(
                draft.stream_id.to_string(),
            ));
        }
        classify_sequence(&stream, &draft, self).await?;
        if draft.sequence < u64::try_from(stream.next_sequence).unwrap_or_default() {
            return duplicate_outcome(stream, &draft, self).await;
        }

        let batch_id = RecordingIngestBatchId::new();
        let now = Utc::now();
        if !quota.is_current(now) {
            return Err(StoreError::InvalidRecordingIngestField {
                field: "quota_checkpoint",
                reason: "does not contain the batch acceptance time",
            });
        }
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
            .query("BEGIN TRANSACTION; LET $current = (SELECT * FROM ONLY $stream); IF $current.state != 'open' OR $current.revision != $revision OR $current.next_sequence != $sequence { THROW 'recording_ingest_checkpoint_conflict'; }; LET $minute = (UPDATE ONLY $minute_window SET batch_count += 1, byte_len += $byte_len, updated_at = $now WHERE tenant = $tenant AND producer_id = $producer_id AND period = 'minute' AND window_started_at = $minute_started_at AND window_ends_at = $minute_ends_at AND batch_count < $maximum_batches_per_minute RETURN AFTER); IF $minute = NONE { THROW 'recording_ingest_batches_per_minute_quota'; }; LET $day = (UPDATE ONLY $day_window SET batch_count += 1, byte_len += $byte_len, updated_at = $now WHERE tenant = $tenant AND producer_id = $producer_id AND period = 'day' AND window_started_at = $day_started_at AND window_ends_at = $day_ends_at AND byte_len + $byte_len <= $maximum_bytes_per_day RETURN AFTER); IF $day = NONE { THROW 'recording_ingest_bytes_per_day_quota'; }; CREATE ONLY $batch CONTENT $content RETURN NONE; UPDATE ONLY $stream SET next_sequence += 1, byte_len += $byte_len, message_count += $message_count, updated_at = $now, revision += 1 RETURN NONE; COMMIT TRANSACTION;")
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
            .bind(("minute_window", quota.minute.id.record_id()))
            .bind(("minute_started_at", quota.minute.started_at))
            .bind(("minute_ends_at", quota.minute.ends_at))
            .bind(("day_window", quota.day.id.record_id()))
            .bind(("day_started_at", quota.day.started_at))
            .bind(("day_ends_at", quota.day.ends_at))
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
            if let Some(quota) = self
                .recording_ingest_quota_rejection(&quota, &draft)
                .await?
            {
                return Err(StoreError::RecordingIngestQuotaExceeded { quota });
            }
            return Err(classify_database_error(error));
        }
        stream.next_sequence =
            stream
                .next_sequence
                .checked_add(1)
                .ok_or(StoreError::InvalidRecordingIngestField {
                    field: "next_sequence",
                    reason: "exceeds the persistence range",
                })?;
        stream.byte_len = stream.byte_len.checked_add(byte_len).ok_or(
            StoreError::InvalidRecordingIngestField {
                field: "byte_len",
                reason: "exceeds the persistence range",
            },
        )?;
        stream.message_count = stream.message_count.checked_add(message_count).ok_or(
            StoreError::InvalidRecordingIngestField {
                field: "message_count",
                reason: "exceeds the persistence range",
            },
        )?;
        stream.updated_at = now;
        stream.revision =
            stream
                .revision
                .checked_add(1)
                .ok_or(StoreError::InvalidRecordingIngestField {
                    field: "revision",
                    reason: "exceeds the persistence range",
                })?;
        let batch = RecordingIngestBatchRecord {
            id: batch_id.record_id(),
            tenant: draft.identity.tenant_id.record_id(),
            stream: draft.stream_id.record_id(),
            sequence,
            payload_format: draft.payload_format,
            sha256: draft.sha256,
            relative_path: draft.relative_path,
            byte_len,
            message_count,
            state: RecordingIngestBatchState::Durable,
            created_at: now,
            materialized_at: None,
        };
        Ok(RecordingIngestAppendOutcome {
            stream,
            batch,
            duplicate: false,
        })
    }

    async fn ensure_recording_ingest_quota_window(
        &self,
        tenant_id: TenantId,
        producer_id: &str,
        window: &RecordingIngestQuotaWindow,
    ) -> Result<(), StoreError> {
        if let Some(existing) = self.recording_ingest_quota_window(window.id).await? {
            return validate_quota_window(&existing, tenant_id, producer_id, window);
        }

        let mut response = self
            .db
            .query("SELECT count() AS batch_count, math::sum(byte_len) AS byte_len FROM recording_ingest_batch WHERE tenant = $tenant AND producer_id = $producer_id AND created_at >= $started_at AND created_at < $ends_at GROUP ALL;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("producer_id", producer_id.to_owned()))
            .bind(("started_at", window.started_at))
            .bind(("ends_at", window.ends_at))
            .await?
            .check()?;
        let usage = response
            .take::<Vec<RecordingIngestQuotaUsage>>(0)?
            .into_iter()
            .next();
        let now = Utc::now();
        let content = RecordingIngestQuotaWindowContent {
            tenant: tenant_id.record_id(),
            producer_id: producer_id.to_owned(),
            period: window.period,
            window_started_at: window.started_at,
            window_ends_at: window.ends_at,
            batch_count: usage.as_ref().map_or(0, |usage| usage.batch_count),
            byte_len: usage.and_then(|usage| usage.byte_len).unwrap_or_default(),
            created_at: now,
            updated_at: now,
        };
        let created = self
            .db
            .query("BEGIN TRANSACTION; CREATE ONLY $window CONTENT $content RETURN NONE; DELETE recording_ingest_quota_window WHERE tenant = $tenant AND producer_id = $producer_id AND window_ends_at <= $started_at RETURN NONE; COMMIT TRANSACTION;")
            .bind(("window", window.id.record_id()))
            .bind(("content", content))
            .bind(("tenant", tenant_id.record_id()))
            .bind(("producer_id", producer_id.to_owned()))
            .bind(("started_at", window.started_at))
            .await
            .and_then(|response| response.check());
        if let Err(error) = created {
            let existing = self
                .recording_ingest_quota_window(window.id)
                .await?
                .ok_or_else(|| classify_database_error(error))?;
            return validate_quota_window(&existing, tenant_id, producer_id, window);
        }
        Ok(())
    }

    async fn recording_ingest_quota_window(
        &self,
        window_id: RecordingIngestQuotaWindowId,
    ) -> Result<Option<RecordingIngestQuotaWindowRecord>, StoreError> {
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $window;")
            .bind(("window", window_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }

    async fn recording_ingest_quota_rejection(
        &self,
        checkpoint: &RecordingIngestQuotaCheckpoint,
        draft: &RecordingIngestBatchDraft,
    ) -> Result<Option<RecordingIngestQuota>, StoreError> {
        let minute = self
            .recording_ingest_quota_window(checkpoint.minute.id)
            .await?;
        if minute
            .is_some_and(|window| window.batch_count >= i64::from(draft.maximum_batches_per_minute))
        {
            return Ok(Some(RecordingIngestQuota::MaximumBatchesPerMinute));
        }
        let byte_len = checked_i64("byte_len", draft.byte_len)?;
        let maximum_bytes_per_day =
            checked_i64("maximum_bytes_per_day", draft.maximum_bytes_per_day)?;
        let day = self
            .recording_ingest_quota_window(checkpoint.day.id)
            .await?;
        if day
            .is_some_and(|window| window.byte_len.saturating_add(byte_len) > maximum_bytes_per_day)
        {
            return Ok(Some(RecordingIngestQuota::MaximumBytesPerDay));
        }
        Ok(None)
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
        let stream = self
            .recording_ingest_stream(tenant_id, stream_id)
            .await?
            .ok_or_else(|| StoreError::RecordingIngestStreamNotFound(stream_id.to_string()))?;
        self.mark_recording_ingest_materialized_at_checkpoint(
            tenant_id,
            stream_id,
            stream,
            through_sequence,
        )
        .await
    }

    /// Advances a materialization checkpoint held by the serialized recording
    /// materializer without rereading it before and after the transaction.
    pub async fn mark_recording_ingest_materialized_at_checkpoint(
        &self,
        tenant_id: TenantId,
        stream_id: RecordingIngestStreamId,
        mut stream: RecordingIngestStreamRecord,
        through_sequence: u64,
    ) -> Result<RecordingIngestStreamRecord, StoreError> {
        if stream.id != stream_id.record_id() || stream.tenant != tenant_id.record_id() {
            return Err(StoreError::RecordingIngestStreamNotFound(
                stream_id.to_string(),
            ));
        }
        let through = checked_i64("through_sequence", through_sequence)?;
        if through >= stream.next_sequence {
            return Err(StoreError::InvalidRecordingIngestField {
                field: "through_sequence",
                reason: "must identify a durable batch",
            });
        }
        if stream
            .materialized_through_sequence
            .is_some_and(|materialized| materialized >= through)
        {
            return Ok(stream);
        }
        let now = Utc::now();
        let committed = self
            .db
            .query("BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $stream SET materialized_through_sequence = $through, updated_at = $now, revision += 1 WHERE (materialized_through_sequence = NONE OR materialized_through_sequence < $through) AND revision = $revision RETURN AFTER); IF $updated = NONE { THROW 'recording_ingest_checkpoint_conflict'; }; UPDATE recording_ingest_batch SET state = 'materialized', materialized_at = $now WHERE tenant = $tenant AND stream = $stream AND sequence <= $through AND state = 'durable' RETURN NONE; COMMIT TRANSACTION;")
            .bind(("tenant", tenant_id.record_id()))
            .bind(("stream", stream_id.record_id()))
            .bind(("through", through))
            .bind(("revision", stream.revision))
            .bind(("now", now))
            .await
            .and_then(|response| response.check());
        if let Err(error) = committed {
            let current = self
                .recording_ingest_stream(tenant_id, stream_id)
                .await?
                .ok_or_else(|| StoreError::RecordingIngestStreamNotFound(stream_id.to_string()))?;
            if current
                .materialized_through_sequence
                .is_some_and(|materialized| materialized >= through)
            {
                return Ok(current);
            }
            return Err(classify_database_error(error));
        }
        stream.materialized_through_sequence = Some(through);
        stream.updated_at = now;
        stream.revision =
            stream
                .revision
                .checked_add(1)
                .ok_or(StoreError::InvalidRecordingIngestField {
                    field: "revision",
                    reason: "exceeds the persistence range",
                })?;
        Ok(stream)
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

fn validate_quota_window(
    existing: &RecordingIngestQuotaWindowRecord,
    tenant_id: TenantId,
    producer_id: &str,
    expected: &RecordingIngestQuotaWindow,
) -> Result<(), StoreError> {
    if existing.id != expected.id.record_id()
        || existing.tenant != tenant_id.record_id()
        || existing.producer_id != producer_id
        || existing.period != expected.period
        || existing.window_started_at != expected.started_at
        || existing.window_ends_at != expected.ends_at
        || existing.batch_count < 0
        || existing.byte_len < 0
        || existing.updated_at < existing.created_at
    {
        return Err(StoreError::InvalidRecordingIngestField {
            field: "quota_checkpoint",
            reason: "stored quota window does not match its deterministic identity",
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
            quota: RecordingIngestQuota::MaximumConcurrentStreams,
        }
    } else if message.contains("recording_ingest_batches_per_minute_quota") {
        StoreError::RecordingIngestQuotaExceeded {
            quota: RecordingIngestQuota::MaximumBatchesPerMinute,
        }
    } else if message.contains("recording_ingest_bytes_per_day_quota") {
        StoreError::RecordingIngestQuotaExceeded {
            quota: RecordingIngestQuota::MaximumBytesPerDay,
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
            payload_format: "rrd_0_35_0".to_owned(),
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

    #[test]
    fn quota_window_identity_is_stable_and_utc_bounded() {
        assert_eq!(
            RecordingIngestQuotaPeriod::Minute.into_value(),
            surrealdb::types::Value::String("minute".to_owned())
        );
        let tenant_id = TenantId::new();
        let started_at = "2026-08-05T07:25:00Z".parse().unwrap();
        let first = RecordingIngestQuotaWindow::new(
            tenant_id,
            "producer-a",
            RecordingIngestQuotaPeriod::Minute,
            started_at,
            started_at + TimeDelta::minutes(1),
        );
        let repeated = RecordingIngestQuotaWindow::new(
            tenant_id,
            "producer-a",
            RecordingIngestQuotaPeriod::Minute,
            started_at,
            started_at + TimeDelta::minutes(1),
        );
        let next = RecordingIngestQuotaWindow::new(
            tenant_id,
            "producer-a",
            RecordingIngestQuotaPeriod::Minute,
            started_at + TimeDelta::minutes(1),
            started_at + TimeDelta::minutes(2),
        );
        assert_eq!(first, repeated);
        assert_ne!(first.id, next.id);
        assert!(first.contains(started_at));
        assert!(first.contains(started_at + TimeDelta::seconds(59)));
        assert!(!first.contains(started_at + TimeDelta::minutes(1)));
    }
}
