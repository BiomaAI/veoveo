//! Native table change delivery and consumer-owned recovery checkpoints.
use std::{collections::BTreeSet, time::Duration};

use chrono::{TimeDelta, Utc};
use futures::{FutureExt, StreamExt, stream::BoxStream};
use surrealdb::{
    Notification,
    types::{RecordId, SurrealValue},
};

use crate::{PlatformStore, PlatformTable, StoreError};

use super::{ChangefeedCursor, ChangefeedEntry, decode_changefeed_entry};

/// A stable identity owned by one logical consumer. Replicas that independently
/// process the same changes must use different identities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangefeedConsumerId(String);

impl ChangefeedConsumerId {
    pub fn new(value: impl Into<String>) -> Result<Self, StoreError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 256
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
            })
        {
            return Err(StoreError::InvalidChangefeedConsumer);
        }
        Ok(Self(value))
    }

    fn record(&self) -> RecordId {
        RecordId::new("changefeed_checkpoint", self.0.clone())
    }
}

/// A consumer reconciles its current-state projection before acknowledging a
/// baseline, or applies every relevant change before acknowledging a page.
#[derive(Clone, Debug)]
pub enum ChangefeedDelivery {
    Reconcile {
        cursor: ChangefeedCursor,
    },
    Changes {
        cursor: ChangefeedCursor,
        entries: Vec<ChangefeedEntry>,
    },
}

impl ChangefeedDelivery {
    pub fn cursor(&self) -> ChangefeedCursor {
        match self {
            Self::Reconcile { cursor } | Self::Changes { cursor, .. } => *cursor,
        }
    }
}

#[derive(SurrealValue)]
struct Checkpoint {
    versionstamp: i64,
}

#[derive(SurrealValue)]
struct Identity {
    id: RecordId,
}

impl PlatformStore {
    pub async fn changefeed_checkpoint(
        &self,
        consumer: &ChangefeedConsumerId,
    ) -> Result<ChangefeedCursor, StoreError> {
        let mut response = self
            .client()
            .query("SELECT versionstamp FROM ONLY $checkpoint;")
            .bind(("checkpoint", consumer.record()))
            .await?
            .check()?;
        let row: Option<Checkpoint> = response.take(0)?;
        ChangefeedCursor::from_versionstamp(row.map_or(0, |row| row.versionstamp)).ok_or(
            StoreError::InvalidChangefeedEntry {
                reason: "consumer checkpoint is negative",
            },
        )
    }

    pub async fn checkpoint_changes(
        &self,
        consumer: &ChangefeedConsumerId,
        cursor: ChangefeedCursor,
    ) -> Result<(), StoreError> {
        self.client()
            .query(
                "UPSERT ONLY $checkpoint SET \
                 versionstamp = math::max([versionstamp ?? 0, $cursor]), \
                 updated_at = $now RETURN NONE;",
            )
            .bind(("checkpoint", consumer.record()))
            .bind(("cursor", cursor.versionstamp()))
            .bind(("now", Utc::now()))
            .await?
            .check()?;
        Ok(())
    }

    /// LIVE provides wake signals; the native feed supplies committed changes.
    /// Consumers own typed table decoding, current-state reconciliation and
    /// checkpoint acknowledgement. No idle timer queries the database.
    pub fn observe_changes(
        &self,
        tables: Vec<PlatformTable>,
        after: ChangefeedCursor,
    ) -> BoxStream<'static, Result<ChangefeedDelivery, StoreError>> {
        let store = self.clone();
        let names: BTreeSet<_> = tables.into_iter().map(PlatformTable::as_str).collect();
        let query = names
            .iter()
            .map(|name| format!("LIVE SELECT id FROM {name};"))
            .collect::<String>();
        Box::pin(async_stream::stream! {
            if names.is_empty() { return; }
            let mut cursor = after;
            loop {
                let connect = async {
                    let anchor = store.changefeed_cursor_now().await?;
                    let mut response = store.client().query(query.clone()).await?.check()?;
                    let stream = response.stream::<Notification<Identity>>(())?;
                    Ok::<_, StoreError>((anchor, stream))
                };
                let (anchor, mut live) = match tokio::time::timeout(Duration::from_secs(15), connect).await {
                    Ok(Ok(source)) => source,
                    result => {
                        let error = match result {
                            Ok(Err(error)) => error,
                            Err(_) => StoreError::ChangefeedConnectionTimeout,
                            Ok(Ok(_)) => unreachable!(),
                        };
                        yield Err(error);
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };
                // All admitted platform tables retain at least seven days.
                // Reconcile conservatively before the shortest retention ends.
                let stale = cursor == ChangefeedCursor::initial()
                    || cursor.timestamp().zip(anchor.timestamp())
                        .is_none_or(|(then, now)| now - then >= TimeDelta::days(6));
                if stale {
                    cursor = anchor;
                }
                // Reconcile on every source establishment. A lost connection can
                // overlap retention cleanup even when its cursor looks recent.
                yield Ok(ChangefeedDelivery::Reconcile { cursor });
                loop {
                    let batches = match store.replay_changes(cursor, 1_000).await {
                        Ok(batches) => batches,
                        Err(error) => { yield Err(error); break; }
                    };
                    if let Some(last) = batches.last() {
                        let Some(next) = last.versionstamp.checked_add(1)
                            .and_then(ChangefeedCursor::from_versionstamp)
                            .filter(|next| *next > cursor)
                        else {
                            yield Err(StoreError::InvalidChangefeedEntry {
                                reason: "consumer recovery cursor did not advance",
                            });
                            return;
                        };
                        let mut entries = Vec::new();
                        for batch in batches {
                            for value in batch.changes {
                                let entry = match decode_changefeed_entry(&value) {
                                    Ok(entry) => entry,
                                    Err(error) => { yield Err(error); return; }
                                };
                                if entry.table().is_some_and(|table| names.contains(table)) {
                                    entries.push(entry);
                                }
                            }
                        }
                        cursor = next;
                        yield Ok(ChangefeedDelivery::Changes { cursor, entries });
                        tokio::task::yield_now().await;
                        continue;
                    }
                    match live.next().await {
                        Some(Ok(hint)) => {
                            let _ = hint.data.id;
                            // One feed drain covers all queued hints. Bound this
                            // coalescing so a hot writer cannot starve replay.
                            for _ in 0..1_000 {
                                match live.next().now_or_never() {
                                    Some(Some(Ok(hint))) => { let _ = hint.data.id; }
                                    Some(Some(Err(error))) => { yield Err(error.into()); break; }
                                    _ => break,
                                }
                            }
                        }
                        Some(Err(error)) => { yield Err(error.into()); break; }
                        None => break,
                    }
                }
            }
        })
    }
}
