//! One native-feed source wakes the runtime's current-state Task readers.
use super::*;
use std::collections::BTreeSet;
use veoveo_platform_store::{
    ChangefeedConsumerId, ChangefeedCursor, ChangefeedDelivery, TaskChange, decode_changefeed_entry,
};

#[derive(Default)]
pub(super) struct SharedWake {
    source: Mutex<std::sync::Weak<watch::Sender<WakeGeneration>>>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct WakeGeneration {
    pub(super) cursor: ChangefeedCursor,
    pub(super) connections: u64,
}

impl SharedWake {
    pub(super) async fn subscribe(
        &self,
        store: PlatformStore,
        server: &str,
        worker: &str,
    ) -> Result<watch::Receiver<WakeGeneration>, TaskError> {
        let mut source = self.source.lock().await;
        if let Some(source) = source
            .upgrade()
            .filter(|source| source.receiver_count() > 0)
        {
            return Ok(source.subscribe());
        }
        let consumer = ChangefeedConsumerId::new(format!("tasks/{server}/{worker}"))?;
        let cursor = store.changefeed_checkpoint(&consumer).await?;
        let (sender, receiver) = watch::channel(WakeGeneration {
            cursor,
            connections: 0,
        });
        let sender = Arc::new(sender);
        *source = Arc::downgrade(&sender);
        tokio::spawn(async move {
            let mut changes = store.observe_changes(vec![PlatformTable::Task], cursor);
            loop {
                let delivery = tokio::select! {
                    _ = sender.closed() => return,
                    delivery = changes.next() => match delivery {
                        Some(Ok(delivery)) => delivery,
                        Some(Err(error)) => {
                            tracing::warn!(%error, "Task changefeed reconnecting");
                            continue;
                        }
                        None => return,
                    }
                };
                let cursor = delivery.cursor();
                let reconcile = matches!(delivery, ChangefeedDelivery::Reconcile { .. });
                let relevant = match &delivery {
                    ChangefeedDelivery::Reconcile { .. } => true,
                    ChangefeedDelivery::Changes { entries, .. } => !entries.is_empty(),
                };
                if relevant {
                    // Readers replay from their own cursors and apply current SQL
                    // admission. Coalescing these hints cannot discard Task state.
                    sender.send_modify(|generation| {
                        generation.cursor = cursor;
                        if reconcile {
                            generation.connections = generation.connections.wrapping_add(1);
                        }
                    });
                }
                if let Err(error) = store.checkpoint_changes(&consumer, cursor).await {
                    // The in-memory source remains current. A restart replays the
                    // older checkpoint and reconciles before accepting new changes.
                    tracing::warn!(%error, "Task changefeed checkpoint was not persisted");
                }
            }
        });
        Ok(receiver)
    }
}

impl TaskRuntime {
    pub(super) async fn task_wake(&self) -> Result<watch::Receiver<WakeGeneration>, TaskError> {
        self.subscription_wake
            .subscribe(self.store.clone(), &self.server, &self.worker_id)
            .await
    }

    /// Trusted domain workers observe exact IDs. Public readers use OwnerTaskQuery.
    pub async fn live_updates_for(&self, ids: &[TaskId]) -> Result<TaskUpdateStream, TaskError> {
        let ids = ids
            .iter()
            .copied()
            .map(validate_task_id)
            .collect::<Result<BTreeSet<_>, _>>()?;
        if ids.is_empty() {
            return Ok(Box::pin(futures::stream::pending()));
        }
        self.observe_tasks(Some(ids), None).await
    }

    async fn observe_tasks(
        &self,
        selected: Option<BTreeSet<TaskId>>,
        after: Option<TaskUpdateCursor>,
    ) -> Result<TaskUpdateStream, TaskError> {
        let mut wake = self.task_wake().await?;
        let mut generation = *wake.borrow_and_update();
        let anchor = self.store.changefeed_cursor_now().await?;
        let mut cursor = after.map_or(anchor, |cursor| {
            ChangefeedCursor::from_versionstamp(cursor.versionstamp()).expect("checked Task cursor")
        });
        let initial = if after.is_none() {
            self.current_tasks(selected.as_ref()).await?
        } else {
            vec![]
        };
        let runtime = self.clone();
        Ok(Box::pin(async_stream::stream! {
            let task_cursor = TaskUpdateCursor::from_versionstamp(cursor.versionstamp()).expect("checked feed cursor");
            for record in initial {
                match record_to_snapshot(record) {
                    Ok(snapshot) => yield Ok(TaskUpdate { cursor: task_cursor, snapshot }),
                    Err(error) => { yield Err(error); return; }
                }
            }
            // Also replay immediately when resuming an established, idle source.
            let mut resume = after.is_some();
            loop {
                if !resume && wake.changed().await.is_err() { return; }
                resume = false;
                let current = *wake.borrow_and_update();
                let records = if current.connections != generation.connections {
                    runtime.current_tasks(selected.as_ref()).await
                } else {
                    match runtime.changed_task_ids(cursor, current.cursor).await {
                        Ok(mut ids) => {
                            if let Some(selected) = &selected { ids.retain(|id| selected.contains(id)); }
                            runtime.current_tasks(Some(&ids)).await
                        }
                        Err(error) => Err(error),
                    }
                };
                generation = current;
                cursor = cursor.max(current.cursor);
                let task_cursor = TaskUpdateCursor::from_versionstamp(cursor.versionstamp()).expect("checked feed cursor");
                match records {
                    Ok(records) => for record in records {
                        match record_to_snapshot(record) {
                            Ok(snapshot) => yield Ok(TaskUpdate { cursor: task_cursor, snapshot }),
                            Err(error) => { yield Err(error); return; }
                        }
                    },
                    Err(error) => { yield Err(error); return; }
                }
            }
        }))
    }

    pub(super) async fn current_tasks(
        &self,
        ids: Option<&BTreeSet<TaskId>>,
    ) -> Result<Vec<TaskRecord>, TaskError> {
        if ids.is_some_and(BTreeSet::is_empty) {
            return Ok(vec![]);
        }
        let sql = match ids {
            Some(_) => include_str!("../../queries/runtime/subscriptions/selected_tasks.surql"),
            None => include_str!("../../queries/runtime/subscriptions/server_tasks.surql"),
        };
        let mut query = self
            .store
            .client()
            .query(sql)
            .bind(("server", RecordId::new("mcp_server", self.server.clone())));
        if let Some(ids) = ids {
            query = query.bind((
                "records",
                ids.iter().copied().map(task_record_id).collect::<Vec<_>>(),
            ));
        }
        let mut response = query.await?.check()?;
        Ok(response.take(0)?)
    }

    pub(super) async fn changed_task_ids(
        &self,
        mut after: ChangefeedCursor,
        through: ChangefeedCursor,
    ) -> Result<BTreeSet<TaskId>, TaskError> {
        let mut ids = BTreeSet::new();
        while after < through {
            let batches = self.store.replay_changes(after, 1_000).await?;
            if batches.is_empty() {
                break;
            }
            for batch in batches {
                if batch.versionstamp >= through.versionstamp() {
                    return Ok(ids);
                }
                for change in batch.changes {
                    if let Some(change) = TaskChange::decode(&decode_changefeed_entry(&change)?)? {
                        ids.insert(change.task_id);
                    }
                }
                after = ChangefeedCursor::from_versionstamp(batch.versionstamp + 1)
                    .ok_or_else(|| TaskError::InvalidRecord("Task feed cursor overflow".into()))?;
            }
        }
        Ok(ids)
    }
}
