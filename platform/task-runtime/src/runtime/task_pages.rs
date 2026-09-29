use super::{OwnerTaskQuery, owner_reads::VISIBLE_TASK};
use crate::types::{TaskError, TaskPage, TaskPageCursor, record_to_snapshot};
use veoveo_platform_store::TaskRecord;
use veoveo_platform_store::task_record_id;

impl OwnerTaskQuery {
    /// Apply the same authority as `TaskOwner::allows` before the database limit.
    /// `in_work_context` adds this caller's context policy before the limit.
    /// The cursor is a position, never an authorization grant or a snapshot lease.
    pub async fn page(
        &self,
        after: Option<&TaskPageCursor>,
        limit: usize,
    ) -> Result<TaskPage, TaskError> {
        if !(1..=1000).contains(&limit) {
            return Err(TaskError::InvalidPageQuery);
        }
        let position = if after.is_some() {
            "AND (created_at > $after_created_at OR (created_at = $after_created_at AND id > $after_task))"
        } else {
            ""
        };
        let mut query = self.bind(self.runtime.store.client().query(format!(
            "SELECT * FROM task WHERE {VISIBLE_TASK} {} {position} ORDER BY created_at ASC, id ASC LIMIT $limit;", self.selection_predicate()
        )))?.bind(("limit", limit + 1));
        if let Some(after) = after {
            query = query
                .bind(("after_created_at", after.created_at))
                .bind(("after_task", task_record_id(after.task_id)));
        }
        let mut response = query.await?.check()?;
        let mut records: Vec<TaskRecord> = response.take(0)?;
        let has_more = records.len() > limit;
        records.truncate(limit);
        let items = records
            .into_iter()
            .map(record_to_snapshot)
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more.then(|| {
            let last = items
                .last()
                .expect("a nonempty bounded page has a last item");
            TaskPageCursor {
                created_at: last.created_at,
                task_id: last.task_id,
            }
        });
        Ok(TaskPage { items, next_cursor })
    }
}
