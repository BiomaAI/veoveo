//! Public Task streams observe current authorized state, never historical payloads.
use super::{
    OwnerTaskQuery, TaskUpdateBaseline, TaskUpdateStream, owner_reads::VISIBLE_TASK,
    subscriptions::AVAILABLE_OUTBOX_TAIL,
};
use crate::types::{TaskError, TaskUpdate, TaskUpdateCursor, record_to_snapshot, validate_task_id};
use chrono::Utc;
use std::{collections::BTreeSet, time::Duration};
use surrealdb::types::SurrealValue;
use veoveo_platform_store::{TaskRecord, task_record_id};
use veoveo_types::TaskId;

/// One SQL-authorized baseline and its current-state updates.
pub struct OwnerTaskSubscription {
    pub accepted_task_ids: Vec<TaskId>,
    pub updates: TaskUpdateStream,
}

#[derive(SurrealValue)]
struct UpdatePage {
    cursor: Option<i64>,
    full: bool,
    tasks: Vec<TaskRecord>,
}

impl OwnerTaskQuery {
    /// Admit at most 256 native identities and reapply current owner policy on updates.
    /// Notifications can coalesce intermediate states; this is not an event-log API.
    pub async fn subscribe(&self, ids: &[TaskId]) -> Result<OwnerTaskSubscription, TaskError> {
        if ids.len() > 256 {
            return Err(TaskError::InvalidPageQuery);
        }
        let ids = ids
            .iter()
            .copied()
            .map(validate_task_id)
            .collect::<Result<BTreeSet<_>, _>>()?;
        if ids.is_empty() {
            return Ok(OwnerTaskSubscription {
                accepted_task_ids: vec![],
                updates: Box::pin(futures::stream::pending()),
            });
        }
        let mut wake = self
            .runtime
            .subscription_wake
            .subscribe(self.runtime.store.clone(), self.runtime.server.clone())
            .await;
        let mut connections = wake.borrow().connections;
        let baseline = self
            .owner_baseline(&ids.into_iter().collect::<Vec<_>>())
            .await?;
        let initial = baseline
            .tasks
            .into_iter()
            .map(record_to_snapshot)
            .collect::<Result<Vec<_>, _>>()?;
        let accepted_task_ids = initial
            .iter()
            .map(|snapshot| snapshot.task_id)
            .collect::<Vec<_>>();
        if accepted_task_ids.is_empty() {
            return Ok(OwnerTaskSubscription {
                accepted_task_ids,
                updates: Box::pin(futures::stream::pending()),
            });
        }
        let ids = accepted_task_ids.clone();
        let runtime = self.clone();
        let mut cursor = TaskUpdateCursor::from_sequence(baseline.cursor.unwrap_or(0))
            .ok_or_else(|| TaskError::InvalidRecord("invalid Task baseline cursor".into()))?;
        let updates = Box::pin(async_stream::stream! {
            for snapshot in initial { yield Ok(TaskUpdate { cursor, snapshot }); }
            let mut reconcile = tokio::time::interval(Duration::from_secs(15));
            reconcile.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    changed = wake.changed() => if changed.is_err() { break; },
                    _ = reconcile.tick() => {},
                }
                let current_connections = wake.borrow_and_update().connections;
                if current_connections != connections {
                    connections = current_connections;
                    // A new LIVE source may follow a gap longer than retained events.
                    // Reconcile admitted identities from current SQL-authorized state.
                    let baseline = match runtime.owner_baseline(&ids).await {
                        Ok(baseline) => baseline,
                        Err(error) => { yield Err(error); return; }
                    };
                    let Some(tail) = TaskUpdateCursor::from_sequence(baseline.cursor.unwrap_or(0)) else {
                        yield Err(TaskError::InvalidRecord("invalid Task recovery cursor".into())); return;
                    };
                    if tail.sequence() > cursor.sequence() { cursor = tail; }
                    for record in baseline.tasks {
                        match record_to_snapshot(record) {
                            Ok(snapshot) => yield Ok(TaskUpdate { cursor, snapshot }),
                            Err(error) => { yield Err(error); return; }
                        }
                    }
                }
                loop {
                    let page = match runtime.owner_update_page(&ids, cursor).await {
                        Ok(page) => page,
                        Err(error) => { yield Err(error); return; }
                    };
                    if let Some(sequence) = page.cursor {
                        match TaskUpdateCursor::from_sequence(sequence) {
                            Some(next) => cursor = next,
                            None => { yield Err(TaskError::InvalidRecord("invalid Task update cursor".into())); return; }
                        }
                    }
                    for record in page.tasks {
                        match record_to_snapshot(record) {
                            Ok(snapshot) => yield Ok(TaskUpdate { cursor, snapshot }),
                            Err(error) => { yield Err(error); return; }
                        }
                    }
                    if !page.full { break; }
                }
            }
        });
        Ok(OwnerTaskSubscription {
            accepted_task_ids,
            updates,
        })
    }

    async fn owner_baseline(&self, ids: &[TaskId]) -> Result<TaskUpdateBaseline, TaskError> {
        let mut response = self.bind(self.runtime.store.client().query(format!(
            "RETURN {{ cursor: array::first(({AVAILABLE_OUTBOX_TAIL})), tasks: (SELECT * FROM $records WHERE {VISIBLE_TASK} {}) }};"
        , self.type_predicate())))?
            .bind(("records", ids.iter().copied().map(task_record_id).collect::<Vec<_>>()))
            .bind(("now", Utc::now())).await?.check()?;
        response
            .take::<Option<TaskUpdateBaseline>>(0)?
            .ok_or_else(|| TaskError::InvalidRecord("missing authorized Task baseline".into()))
    }

    async fn owner_update_page(
        &self,
        ids: &[TaskId],
        cursor: TaskUpdateCursor,
    ) -> Result<UpdatePage, TaskError> {
        // Event identities wake current reads. Stored event snapshots are neither
        // selected nor decoded, and cannot preserve authority that has been revoked.
        let mut response = self
            .bind(self.runtime.store.client().query(format!(
                "LET $changes = SELECT sequence, aggregate_id FROM outbox_event
                WHERE sequence > $cursor AND available_at <= $now
                AND aggregate_type = 'task' AND aggregate_id IN $ids
                AND payload.snapshot.server = $server_key ORDER BY sequence ASC LIMIT 256;
             RETURN {{ cursor: array::last($changes.sequence), full: array::len($changes) = 256,
                tasks: (SELECT * FROM $records WHERE {VISIBLE_TASK} {}
                    AND <string> record::id(id) IN $changes.aggregate_id) }};",
                self.type_predicate()
            )))?
            .bind(("cursor", cursor.sequence()))
            .bind(("now", Utc::now()))
            .bind(("server_key", self.runtime.server.clone()))
            .bind((
                "ids",
                ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ))
            .bind((
                "records",
                ids.iter().copied().map(task_record_id).collect::<Vec<_>>(),
            ))
            .await?
            .check()?;
        response
            .take::<Option<UpdatePage>>(1)?
            .ok_or_else(|| TaskError::InvalidRecord("missing authorized Task update page".into()))
    }
}
