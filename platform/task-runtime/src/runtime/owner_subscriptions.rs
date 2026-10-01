//! Public Task streams observe current authorized state, never historical payloads.
use super::{OwnerTaskQuery, TaskUpdateStream, owner_reads::VISIBLE_TASK};
use crate::types::{TaskError, TaskUpdate, TaskUpdateCursor, record_to_snapshot, validate_task_id};
use std::collections::BTreeSet;
use veoveo_platform_store::{TaskRecord, task_record_id};
use veoveo_types::TaskId;

pub struct OwnerTaskSubscription {
    pub accepted_task_ids: Vec<TaskId>,
    pub updates: TaskUpdateStream,
}

impl OwnerTaskQuery {
    /// Admit at most 256 identities. Every delivery reapplies current SQL policy.
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
        let mut wake = self.runtime.task_wake().await?;
        let mut generation = *wake.borrow_and_update();
        // Anchor before the baseline; first source establishment recovers any gap.
        let mut cursor = self.runtime.store.changefeed_cursor_now().await?;
        let initial = self
            .owner_current_tasks(&ids)
            .await?
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
        let ids = accepted_task_ids.iter().copied().collect::<BTreeSet<_>>();
        let query = self.clone();
        let updates = Box::pin(async_stream::stream! {
            let task_cursor = TaskUpdateCursor::from_versionstamp(cursor.versionstamp()).expect("checked feed cursor");
            for snapshot in initial { yield Ok(TaskUpdate { cursor: task_cursor, snapshot }); }
            loop {
                if wake.changed().await.is_err() { return; }
                let current = *wake.borrow_and_update();
                let changed = if current.connections != generation.connections {
                    Ok(ids.clone())
                } else {
                    query.runtime.changed_task_ids(cursor, current.cursor).await.map(|changes| changes.intersection(&ids).copied().collect())
                };
                generation = current;
                cursor = cursor.max(current.cursor);
                let changed = match changed { Ok(ids) => ids, Err(error) => { yield Err(error); return; } };
                let records = match query.owner_current_tasks(&changed).await {
                    Ok(records) => records,
                    Err(error) => { yield Err(error); return; }
                };
                let task_cursor = TaskUpdateCursor::from_versionstamp(cursor.versionstamp()).expect("checked feed cursor");
                for record in records {
                    match record_to_snapshot(record) {
                        Ok(snapshot) => yield Ok(TaskUpdate { cursor: task_cursor, snapshot }),
                        Err(error) => { yield Err(error); return; }
                    }
                }
            }
        });
        Ok(OwnerTaskSubscription {
            accepted_task_ids,
            updates,
        })
    }

    async fn owner_current_tasks(
        &self,
        ids: &BTreeSet<TaskId>,
    ) -> Result<Vec<TaskRecord>, TaskError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let mut response = self
            .bind(self.runtime.store.client().query(format!(
                "SELECT * FROM $records WHERE {VISIBLE_TASK} {};",
                self.selection_predicate()
            )))?
            .bind((
                "records",
                ids.iter().copied().map(task_record_id).collect::<Vec<_>>(),
            ))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}
