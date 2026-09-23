//! Exact-filter Task observation. One projected database wake serves all listeners.
use super::*;
use surrealdb::Notification;

// Read the committed tail in sequence order. The available-at index scans and
// sorts the entire historical outbox before LIMIT, delaying leases and readers.
pub(super) const AVAILABLE_OUTBOX_TAIL: &str = "SELECT VALUE sequence FROM outbox_event WITH INDEX outbox_event_sequence_unique WHERE available_at <= $now ORDER BY sequence DESC LIMIT 1";

#[derive(Default)]
pub(super) struct SharedWake {
    source: Mutex<Option<watch::Sender<u64>>>,
}
#[derive(SurrealValue)]
struct Hint {
    sequence: i64,
}

impl SharedWake {
    async fn subscribe(&self, store: PlatformStore, server: String) -> watch::Receiver<u64> {
        let mut source = self.source.lock().await;
        if let Some(source) = source.as_ref().filter(|source| source.receiver_count() > 0) {
            return source.subscribe();
        }
        let (sender, receiver) = watch::channel(0u64);
        *source = Some(sender.clone());
        tokio::spawn(async move {
            loop {
                let connect = async {
                    let mut response = store.client().query("LIVE SELECT sequence FROM outbox_event WHERE aggregate_type = 'task' AND payload.snapshot.server = $server;")
                        .bind(("server", server.clone())).await?.check()?;
                    response.stream::<Notification<Hint>>(0)
                };
                let mut stream = tokio::select! {
                    _ = sender.closed() => return,
                    result = connect => match result {
                        Ok(stream) => stream,
                        Err(_) => { tokio::select! { _ = sender.closed() => return, _ = tokio::time::sleep(Duration::from_secs(1)) => {} } continue; }
                    }
                };
                // Recover changes between the baseline and first LIVE establishment,
                // and across every lost source. The durable cursor supplies content.
                sender.send_modify(|generation| *generation = generation.wrapping_add(1));
                loop {
                    tokio::select! {
                        _ = sender.closed() => return,
                        event = stream.next() => match event {
                            Some(Ok(event)) => { let _ = event.data.sequence; sender.send_modify(|generation| *generation = generation.wrapping_add(1)); }
                            _ => break,
                        }
                    }
                }
            }
        });
        receiver
    }
}

impl TaskRuntime {
    /// Observe exact Tasks within this runtime's server. Trusted domain workers use
    /// this for cross-replica cancellation; public callers must pass through the
    /// authorized `subscribe_durable_tasks` projection.
    pub async fn live_updates_for(&self, ids: &[String]) -> Result<TaskUpdateStream, TaskError> {
        if ids.is_empty() {
            return Ok(Box::pin(futures::stream::pending()));
        }
        let mut ids = ids.to_vec();
        ids.sort();
        ids.dedup();
        let records: Vec<_> = ids
            .iter()
            .map(|id| parse_task_id(id).map(|id| id.record_id()))
            .collect::<Result<_, _>>()?;
        let mut wake = self
            .subscription_wake
            .subscribe(self.store.clone(), self.server.clone())
            .await;
        let mut response = self.store.client().query(format!(
            "RETURN {{ cursor: array::first(({AVAILABLE_OUTBOX_TAIL})), tasks: (SELECT * FROM $records WHERE server = $server) }};"))
            .bind(("records", records)).bind(("server", RecordId::new("mcp_server", self.server.clone())))
            .bind(("now", Utc::now())).await?.check()?;
        let baseline: TaskUpdateBaseline = response
            .take::<Option<TaskUpdateBaseline>>(0)?
            .ok_or_else(|| TaskError::InvalidRecord("missing Task baseline".into()))?;
        let runtime = self.clone();
        let mut cursor = TaskUpdateCursor::from_sequence(baseline.cursor.unwrap_or(0))
            .expect("nonnegative sequence");
        let initial = baseline
            .tasks
            .into_iter()
            .map(record_to_snapshot)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Box::pin(async_stream::stream! {
            for snapshot in initial { yield Ok(TaskUpdate { cursor, snapshot }); }
            // Reconciliation also covers lost LIVE notifications; it reads native
            // state only and never invokes a provider or dispatches an operation.
            let mut reconcile = tokio::time::interval(Duration::from_secs(15));
            reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    changed = wake.changed() => if changed.is_err() { break; },
                    _ = reconcile.tick() => {},
                }
                loop {
                    let page = runtime.filtered_page(cursor, &ids).await;
                    let events = match page { Ok(events) => events, Err(error) => { yield Err(error); return; } };
                    let full = events.len() == 256;
                    for event in events {
                        cursor = TaskUpdateCursor::from_sequence(event.sequence).expect("nonnegative sequence");
                        match task_snapshot_from_event(&event) {
                            Ok(snapshot) => yield Ok(TaskUpdate { cursor, snapshot }),
                            Err(error) => { yield Err(error); return; }
                        }
                    }
                    if !full { break; }
                }
            }
        }))
    }

    async fn filtered_page(
        &self,
        cursor: TaskUpdateCursor,
        ids: &[String],
    ) -> Result<Vec<OutboxEventRecord>, TaskError> {
        let mut response = self.store.client().query("SELECT * FROM outbox_event WHERE sequence > $cursor AND available_at <= $now AND aggregate_type = 'task' AND aggregate_id IN $ids AND payload.snapshot.server = $server ORDER BY sequence ASC LIMIT 256;")
            .bind(("cursor", cursor.sequence())).bind(("now", Utc::now())).bind(("ids", ids.to_vec())).bind(("server", self.server.clone())).await?.check()?;
        Ok(response.take(0)?)
    }
}

#[cfg(test)]
#[path = "../../../../testing/fixtures/store.rs"]
mod fixture;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires the pinned disposable SurrealDB Docker fixture"]
    async fn available_tail_uses_reverse_index_and_excludes_future_events() {
        let db = fixture::TestDb::new().await;
        db.a.client().query("CREATE outbox_event SET aggregate_type = 'fixture', aggregate_id = 'past', event_type = 'fixture', schema_version = 1, payload = {}; CREATE outbox_event SET aggregate_type = 'fixture', aggregate_id = 'future', event_type = 'fixture', schema_version = 1, payload = {}, available_at = time::now() + 1d;")
            .await.unwrap().check().unwrap();
        let mut response = db.b.client().query(format!("{AVAILABLE_OUTBOX_TAIL} EXPLAIN; {AVAILABLE_OUTBOX_TAIL}; SELECT VALUE sequence FROM outbox_event WHERE aggregate_id = 'past';"))
            .bind(("now", Utc::now())).await.unwrap().check().unwrap();
        let plan: surrealdb::types::Value = response.take(0).unwrap();
        let plan = format!("{plan:?}");
        assert!(
            plan.contains("outbox_event_sequence_unique") && plan.contains("Backward"),
            "{plan}"
        );
        assert!(
            !plan.contains("Sort") && !plan.contains("TableScan"),
            "{plan}"
        );
        let tail: Vec<i64> = response.take(1).unwrap();
        let past: Vec<i64> = response.take(2).unwrap();
        assert_eq!(tail, past);
        assert_eq!(tail.len(), 1);
    }
}
