//! Usage visibility follows the linked Task's current owner and the caller's clearance.
use super::{
    TaskRuntime,
    context_scope::ContextScope,
    owner_reads::{OwnerScope, VISIBLE_TASK},
};
use crate::types::{TaskError, TaskOwner, task_id_from_record};
use surrealdb::{Connection, method::Query};
use veoveo_platform_store::{DomainUsageRecord, RecordId, task_record_id};
use veoveo_types::TaskId;

const VISIBLE_USAGE: &str = include_str!("../../queries/usage_visible.surql");

/// The domain selects its policy explicitly for every usage or Task admission read.
/// Both variants enforce `TaskOwner::allows`. `WorkContext` additionally requires
/// the Task's indexed context and stored authority to agree with the caller.
/// This selection does not establish Work Context membership or permissions.
#[derive(Clone, Copy, Debug)]
pub enum TaskUsageAccess<'a> {
    Owner(&'a TaskOwner),
    WorkContext(&'a TaskOwner),
}

impl<'a> TaskUsageAccess<'a> {
    fn owner(self) -> &'a TaskOwner {
        match self {
            Self::Owner(owner) | Self::WorkContext(owner) => owner,
        }
    }

    fn usage_predicate(self) -> &'static str {
        match self {
            Self::Owner(_) => "",
            Self::WorkContext(_) => ContextScope::USAGE_PREDICATE,
        }
    }

    fn task_predicate(self) -> &'static str {
        match self {
            Self::Owner(_) => "",
            Self::WorkContext(_) => ContextScope::TASK_PREDICATE,
        }
    }
}

struct UsageScope {
    owner: OwnerScope,
    context: Option<ContextScope>,
}

impl UsageScope {
    fn bind<C: Connection>(self, query: Query<'_, C>) -> Query<'_, C> {
        let query = self.owner.bind(query);
        match self.context {
            Some(context) => context.bind(query),
            None => query,
        }
    }

    fn new(runtime: &TaskRuntime, access: TaskUsageAccess<'_>) -> Result<Self, TaskError> {
        let owner = access.owner();
        let context = match access {
            TaskUsageAccess::Owner(_) => None,
            TaskUsageAccess::WorkContext(_) => Some(ContextScope::new(owner)?),
        };
        Ok(Self {
            owner: OwnerScope::new(runtime, owner)?,
            context,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskUsagePage {
    pub task_ids: Vec<TaskId>,
    pub next_task_id: Option<TaskId>,
}

impl TaskRuntime {
    /// Admit a caller-owned Task before its first usage row is recorded.
    pub async fn task_visible(
        &self,
        access: TaskUsageAccess<'_>,
        task_id: TaskId,
    ) -> Result<bool, TaskError> {
        let context = access.task_predicate();
        let mut response = UsageScope::new(self, access)?
            .bind(self.store.client().query(format!(
                "SELECT VALUE id FROM task WHERE id = $task AND {VISIBLE_TASK} {context} LIMIT 1;",
            )))
            .bind(("task", task_record_id(task_id)))
            .await?
            .check()?;
        let ids: Vec<RecordId> = response.take(0)?;
        Ok(!ids.is_empty())
    }

    /// Page distinct usage Tasks after applying the selected access policy in SQL.
    pub async fn usage_page(
        &self,
        access: TaskUsageAccess<'_>,
        after: Option<TaskId>,
        limit: usize,
    ) -> Result<TaskUsagePage, TaskError> {
        if !(1..=1000).contains(&limit) {
            return Err(TaskError::InvalidPageQuery);
        }
        let position = if after.is_some() {
            // The 3.3.0 task/time index can include rows whose leading Task key
            // equals the exclusive bound. Keep inequality in the SQL residual.
            "AND task > $after AND task != $after"
        } else {
            ""
        };
        let context = access.usage_predicate();
        let mut response = UsageScope::new(self, access)?.bind(self.store.client()
            .query(format!("SELECT VALUE task FROM domain_usage WHERE {VISIBLE_USAGE} {context} {position} GROUP BY task ORDER BY task ASC LIMIT $limit;"))
            )
            .bind(("after", after.map(task_record_id)))
            .bind(("limit", limit + 1))
            .await?.check()?;
        let mut records: Vec<RecordId> = response.take(0)?;
        let has_more = records.len() > limit;
        records.truncate(limit);
        let task_ids = records
            .iter()
            .map(task_id_from_record)
            .collect::<Result<Vec<_>, _>>()?;
        let next_task_id =
            has_more.then(|| *task_ids.last().expect("an overfull page is nonempty"));
        Ok(TaskUsagePage {
            task_ids,
            next_task_id,
        })
    }

    /// Read one Task's usage with the same SQL visibility as collection pages.
    /// A missing Task, missing usage, or denied caller produces no rows.
    pub async fn usage(
        &self,
        access: TaskUsageAccess<'_>,
        task_id: TaskId,
    ) -> Result<Vec<DomainUsageRecord>, TaskError> {
        let context = access.usage_predicate();
        let mut response = UsageScope::new(self, access)?.bind(self.store.client()
            .query(format!("SELECT * FROM domain_usage WHERE {VISIBLE_USAGE} {context} AND task = $task ORDER BY recorded_at ASC, id ASC;"))
            )
            .bind(("task", task_record_id(task_id)))
            .await?.check()?;
        Ok(response.take(0)?)
    }
}
