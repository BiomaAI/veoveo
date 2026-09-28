use super::{
    TaskRuntime,
    owner_reads::{OwnerScope, VISIBLE_TASK},
};
use crate::types::{TaskError, TaskOwner, TaskPage, TaskPageCursor, record_to_snapshot};
use veoveo_platform_store::TaskRecord;
use veoveo_platform_store::task_record_id;

impl TaskRuntime {
    /// Apply the same authority as `TaskOwner::allows` before the database limit.
    /// Domains that further restrict Work Context must use their narrower query.
    /// The cursor is a position, never an authorization grant or a snapshot lease.
    pub async fn list_page_for_owner(
        &self,
        owner: &TaskOwner,
        task_types: &[&str],
        after: Option<&TaskPageCursor>,
        limit: usize,
    ) -> Result<TaskPage, TaskError> {
        if !(1..=1000).contains(&limit)
            || !(1..=32).contains(&task_types.len())
            || task_types.iter().any(|kind| kind.is_empty())
        {
            return Err(TaskError::InvalidPageQuery);
        }
        let position = if after.is_some() {
            "AND (created_at > $after_created_at OR (created_at = $after_created_at AND id > $after_task))"
        } else {
            ""
        };
        let mut query = OwnerScope::new(self, owner)?.bind(self.store.client().query(format!(
            "SELECT * FROM task WHERE {VISIBLE_TASK} AND task_type IN $task_types {position} ORDER BY created_at ASC, id ASC LIMIT $limit;"
        )))
            .bind((
                "task_types",
                task_types
                    .iter()
                    .map(|kind| (*kind).to_owned())
                    .collect::<Vec<_>>(),
            ))
            .bind(("limit", limit + 1));
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
