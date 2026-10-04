//! Trusted worker history. Public subscriptions re-read current authorized rows.
use super::*;
use veoveo_platform_store::{ChangefeedCursor, TaskChange, decode_changefeed_entry};
use veoveo_types::ServerSlug;

impl TaskRuntime {
    pub async fn live_updates(&self) -> Result<TaskUpdateStream, TaskError> {
        self.history_updates(None).await
    }

    /// Resume native committed states. A returned cursor replays its entire final
    /// transaction on reconnect, so a multi-Task commit cannot lose its later rows.
    /// Consumers must accept duplicate states at that transaction boundary.
    pub async fn live_updates_after(
        &self,
        cursor: TaskUpdateCursor,
    ) -> Result<TaskUpdateStream, TaskError> {
        self.history_updates(Some(cursor)).await
    }

    async fn history_updates(
        &self,
        after: Option<TaskUpdateCursor>,
    ) -> Result<TaskUpdateStream, TaskError> {
        let mut wake = self.task_wake().await?;
        let server = ServerSlug::parse(&self.server)
            .map_err(|_| TaskError::InvalidRecord("invalid runtime server identity".into()))?;
        let head = self.store.changefeed_head().await?;
        let mut cursor = after.map_or(head, |after| {
            ChangefeedCursor::from_versionstamp(after.versionstamp()).expect("checked Task cursor")
        });
        let stale = cursor
            .timestamp()
            .zip(head.timestamp())
            .is_none_or(|(then, now)| now - then >= TimeDelta::days(6));
        let initial = if after.is_none() || stale {
            cursor = head;
            self.current_tasks(None).await?
        } else {
            vec![]
        };
        let runtime = self.clone();
        Ok(Box::pin(async_stream::stream! {
            for record in initial {
                match record_to_snapshot(record) {
                    Ok(snapshot) => yield Ok(TaskUpdate { cursor: TaskUpdateCursor::from_versionstamp(cursor.versionstamp()).expect("checked feed cursor"), snapshot }),
                    Err(error) => { yield Err(error); return; }
                }
            }
            let mut through = head;
            loop {
                while cursor < through {
                    let batches = match runtime.store.replay_changes(cursor, 1_000).await {
                        Ok(batches) => batches,
                        Err(error) => { yield Err(error.into()); return; }
                    };
                    if batches.is_empty() { break; }
                    for batch in batches {
                        if batch.versionstamp >= through.versionstamp() { cursor = through; break; }
                        for change in batch.changes {
                            let record = decode_changefeed_entry(&change).and_then(|entry| TaskChange::snapshot(&entry, &server));
                            match record {
                                Ok(Some(record)) => match record_to_snapshot(record) {
                                    Ok(snapshot) => yield Ok(TaskUpdate { cursor: TaskUpdateCursor::from_versionstamp(batch.versionstamp).expect("committed feed cursor"), snapshot }),
                                    Err(error) => { yield Err(error); return; }
                                },
                                Ok(None) => {},
                                Err(error) => { yield Err(error.into()); return; }
                            }
                        }
                        cursor = ChangefeedCursor::from_versionstamp(batch.versionstamp + 1).expect("committed cursor advances");
                    }
                }
                if wake.changed().await.is_err() { return; }
                let generation = *wake.borrow_and_update();
                through = through.max(generation.cursor);
                if cursor.timestamp().zip(through.timestamp()).is_none_or(|(then, now)| now - then >= TimeDelta::days(6)) {
                    cursor = through;
                    let records = match runtime.current_tasks(None).await {
                        Ok(records) => records,
                        Err(error) => { yield Err(error); return; }
                    };
                    for record in records {
                        match record_to_snapshot(record) {
                            Ok(snapshot) => yield Ok(TaskUpdate { cursor: TaskUpdateCursor::from_versionstamp(cursor.versionstamp()).expect("checked cursor"), snapshot }),
                            Err(error) => { yield Err(error); return; }
                        }
                    }
                }
            }
        }))
    }
}
