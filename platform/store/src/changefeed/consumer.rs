//! Native table change delivery and consumer-owned recovery checkpoints.
use std::{collections::BTreeSet, time::Duration};

use chrono::{TimeDelta, Utc};
use futures::{FutureExt, StreamExt, stream::BoxStream};
use surrealdb::types::{RecordId, SurrealValue};

use crate::{ObservationReplay, ObservationTable, PlatformStore, StoreError};

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

impl PlatformStore {
    pub async fn changefeed_checkpoint(
        &self,
        consumer: &ChangefeedConsumerId,
    ) -> Result<ChangefeedCursor, StoreError> {
        let mut response = self
            .client()
            .query(include_str!("../../queries/changefeed/checkpoint.surql"))
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
            .query(include_str!("../../queries/changefeed/acknowledge.surql"))
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
    pub fn observe_changes<T: Into<ObservationTable>>(
        &self,
        tables: Vec<T>,
        after: ChangefeedCursor,
    ) -> BoxStream<'static, Result<ChangefeedDelivery, StoreError>> {
        let store = self.clone();
        let tables: Vec<ObservationTable> = tables.into_iter().map(Into::into).collect();
        let names: BTreeSet<_> = tables
            .iter()
            .map(|table| table.as_str().to_owned())
            .collect();
        let retention = tables
            .iter()
            .map(|table| match table.replay() {
                ObservationReplay::LiveOnly => None,
                ObservationReplay::Changefeed(retention) => Some(retention.seconds()),
            })
            .collect::<Option<Vec<_>>>();
        Box::pin(async_stream::stream! {
            if names.is_empty() { return; }
            let Some(retention) = retention else {
                yield Err(StoreError::InvalidChangefeedEntry { reason: "recoverable observation requires declared changefeed retention" });
                return;
            };
            let replay_window = replay_window(retention);
            let mut cursor = after;
            loop {
                let connect = async {
                    let anchor = store.changefeed_cursor_now().await?;
                    let stream = store.live_identities(&tables).await?;
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
                // Leave a one-day safety margin before the shortest declared retention.
                let stale = cursor == ChangefeedCursor::initial()
                    || cursor.timestamp().zip(anchor.timestamp())
                        .is_none_or(|(then, now)| now - then >= replay_window);
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

fn replay_window(retention: Vec<u32>) -> TimeDelta {
    TimeDelta::seconds(i64::from(
        retention
            .into_iter()
            .min()
            .unwrap_or(0)
            .saturating_sub(86_400),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortest_retention_controls_the_conservative_window() {
        assert_eq!(
            replay_window(vec![30 * 86400, 7 * 86400]),
            TimeDelta::days(6)
        );
        assert_eq!(replay_window(vec![86400]), TimeDelta::zero());
        assert_eq!(replay_window(vec![1]), TimeDelta::zero());
        assert_eq!(replay_window(vec![]), TimeDelta::zero());
    }
}
