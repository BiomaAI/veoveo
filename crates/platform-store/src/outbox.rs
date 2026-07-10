use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{OpenObject, OutboxEventRecord, PlatformStore, StoreError};

const MAX_OUTBOX_LIMIT: u32 = 1_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct OutboxDraft {
    pub tenant: Option<RecordId>,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub event_type: String,
    pub schema_version: i64,
    pub payload: OpenObject,
    pub occurred_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
}

impl OutboxDraft {
    pub fn now(
        tenant: Option<RecordId>,
        aggregate_type: impl Into<String>,
        aggregate_id: impl Into<String>,
        event_type: impl Into<String>,
        schema_version: i64,
        payload: OpenObject,
    ) -> Self {
        let now = Utc::now();
        Self {
            tenant,
            aggregate_type: aggregate_type.into(),
            aggregate_id: aggregate_id.into(),
            event_type: event_type.into(),
            schema_version,
            payload,
            occurred_at: now,
            available_at: now,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutboxPage {
    pub events: Vec<OutboxEventRecord>,
    pub next_sequence: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct OutboxCheckpoint {
    id: RecordId,
    consumer: String,
    last_sequence: i64,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
struct OutboxCheckpointContent {
    consumer: String,
    last_sequence: i64,
    updated_at: DateTime<Utc>,
}

impl PlatformStore {
    /// Append an event. State-changing callers should include the equivalent
    /// `CREATE outbox_event` statement in their own SurrealDB transaction when
    /// atomic state-and-event publication is required.
    pub async fn append_outbox(&self, event: OutboxDraft) -> Result<OutboxEventRecord, StoreError> {
        let mut response = self
            .db
            .query("CREATE ONLY outbox_event CONTENT $event RETURN AFTER;")
            .bind(("event", event))
            .await?
            .check()?;
        let created: Option<OutboxEventRecord> = response.take(0)?;
        created.ok_or(StoreError::MissingRecord {
            operation: "append_outbox",
        })
    }

    pub async fn read_outbox(
        &self,
        after_sequence: i64,
        limit: u32,
    ) -> Result<OutboxPage, StoreError> {
        if limit == 0 || limit > MAX_OUTBOX_LIMIT {
            return Err(StoreError::InvalidOutboxLimit {
                max: MAX_OUTBOX_LIMIT,
            });
        }
        let mut response = self
            .db
            .query(
                "SELECT * FROM outbox_event WHERE sequence > $after AND available_at <= $now ORDER BY sequence ASC LIMIT $limit;",
            )
            .bind(("after", after_sequence.max(0)))
            .bind(("now", Utc::now()))
            .bind(("limit", i64::from(limit)))
            .await?
            .check()?;
        let events: Vec<OutboxEventRecord> = response.take(0)?;
        let next_sequence = events
            .last()
            .map(|event| event.sequence)
            .unwrap_or(after_sequence.max(0));
        Ok(OutboxPage {
            events,
            next_sequence,
        })
    }

    /// Highest committed outbox sequence at the time of the query.
    pub async fn latest_outbox_sequence(&self) -> Result<i64, StoreError> {
        let mut response = self
            .db
            .query("SELECT VALUE sequence FROM outbox_event ORDER BY sequence DESC LIMIT 1;")
            .await?
            .check()?;
        Ok(response
            .take::<Vec<i64>>(0)?
            .into_iter()
            .next()
            .unwrap_or(0))
    }

    /// Persist the last contiguous sequence processed by a single-writer
    /// consumer. The deterministic record key prevents duplicate checkpoints.
    pub async fn checkpoint_outbox(
        &self,
        consumer: &str,
        last_sequence: i64,
    ) -> Result<(), StoreError> {
        let last_sequence = last_sequence.max(0);
        if self.outbox_checkpoint(consumer).await? >= last_sequence {
            return Ok(());
        }
        let record = checkpoint_record(consumer);
        let content = OutboxCheckpointContent {
            consumer: consumer.to_owned(),
            last_sequence,
            updated_at: Utc::now(),
        };
        self.db
            .query("UPSERT ONLY $record CONTENT $content RETURN NONE;")
            .bind(("record", record))
            .bind(("content", content))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn outbox_checkpoint(&self, consumer: &str) -> Result<i64, StoreError> {
        let record = checkpoint_record(consumer);
        let mut response = self
            .db
            .query("SELECT * FROM ONLY $record;")
            .bind(("record", record))
            .await?
            .check()?;
        let checkpoint: Option<OutboxCheckpoint> = response.take(0)?;
        Ok(checkpoint.map(|value| value.last_sequence).unwrap_or(0))
    }
}

fn checkpoint_record(consumer: &str) -> RecordId {
    let digest = Sha256::digest(consumer.as_bytes());
    let mut key = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut key, "{byte:02x}").expect("writing to String cannot fail");
    }
    RecordId::new("outbox_checkpoint", key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_keys_are_deterministic_and_do_not_expose_consumer_names() {
        let first = checkpoint_record("console-projection");
        let second = checkpoint_record("console-projection");
        assert_eq!(first, second);
        assert!(!format!("{first:?}").contains("console-projection"));
    }
}
