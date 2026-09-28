//! Usage visibility follows the linked Task's current owner and the caller's clearance.
use super::{TaskRuntime, owner_record, tenant_record};
use crate::types::{TaskError, TaskOwner, task_id_from_record};
use std::collections::BTreeSet;
use surrealdb::{Connection, method::Query};
use veoveo_platform_store::{
    DomainUsageRecord, PlatformTable, RecordId, deterministic_work_context_id, task_record_id,
};
use veoveo_types::TaskId;

const VISIBLE_USAGE: &str = "server = $server AND tenant = $tenant
    AND task.server = $server AND task.tenant = $tenant
    AND task.owner = $owner AND task.profile = $profile
    AND task.request.owner.principal_key = $principal_key
    AND task.request.owner.profile = $profile_key
    AND (task.request.owner.tenant_key ?? NONE) = $tenant_key
    AND task.request.owner.data_labels ALLINSIDE $labels";

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
            Self::WorkContext(_) => {
                "AND task.work_context = $work_context
                AND task.authority.context_key = $work_context_key
                AND task.request.owner.authority.work_context = $work_context_key
                AND task.request.owner.authority.tenant = $authority_tenant"
            }
        }
    }

    fn task_predicate(self) -> &'static str {
        match self {
            Self::Owner(_) => "",
            Self::WorkContext(_) => {
                "AND work_context = $work_context
                AND authority.context_key = $work_context_key
                AND request.owner.authority.work_context = $work_context_key
                AND request.owner.authority.tenant = $authority_tenant"
            }
        }
    }
}

struct UsageScope {
    server: RecordId,
    tenant: RecordId,
    owner: RecordId,
    profile: RecordId,
    principal_key: String,
    profile_key: String,
    tenant_key: Option<String>,
    labels: BTreeSet<String>,
    context: Option<ContextScope>,
}

struct ContextScope {
    record: RecordId,
    key: veoveo_types::WorkContextId,
    tenant: veoveo_types::TenantId,
}

impl UsageScope {
    fn bind<C: Connection>(self, query: Query<'_, C>) -> Query<'_, C> {
        // Scalar bindings let the planner use the server/task compound index;
        // object-property parameters are not folded into index constraints in 3.2.4.
        let query = query
            .bind(("server", self.server))
            .bind(("tenant", self.tenant))
            .bind(("owner", self.owner))
            .bind(("profile", self.profile))
            .bind(("principal_key", self.principal_key))
            .bind(("profile_key", self.profile_key))
            .bind(("tenant_key", self.tenant_key))
            .bind(("labels", self.labels));
        match self.context {
            Some(context) => query
                .bind(("work_context", context.record))
                .bind(("work_context_key", context.key.to_string()))
                .bind(("authority_tenant", context.tenant.to_string())),
            None => query,
        }
    }

    fn new(runtime: &TaskRuntime, access: TaskUsageAccess<'_>) -> Result<Self, TaskError> {
        let owner = access.owner();
        let context = match access {
            TaskUsageAccess::Owner(_) => None,
            TaskUsageAccess::WorkContext(_) => {
                if owner.authority.tenant.as_str() != owner.tenant_key() {
                    return Err(TaskError::InvalidAuthority(
                        "task owner and Work Context belong to different tenants".into(),
                    ));
                }
                Some(ContextScope {
                    record: deterministic_work_context_id(
                        owner.tenant_key(),
                        owner.authority.work_context.as_str(),
                    )?
                    .record_id(),
                    key: owner.authority.work_context.clone(),
                    tenant: owner.authority.tenant.clone(),
                })
            }
        };
        Ok(Self {
            server: RecordId::new(PlatformTable::McpServer.as_str(), runtime.server.clone()),
            tenant: tenant_record(owner)?,
            owner: owner_record(owner)?,
            profile: RecordId::new(PlatformTable::Profile.as_str(), owner.profile.clone()),
            principal_key: owner.principal_key.clone(),
            profile_key: owner.profile.clone(),
            tenant_key: owner.tenant_key.clone(),
            labels: owner.data_labels.clone(),
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
                "SELECT VALUE id FROM task WHERE id = $task AND server = $server
                AND tenant = $tenant AND owner = $owner AND profile = $profile
                AND request.owner.principal_key = $principal_key
                AND request.owner.profile = $profile_key
                AND (request.owner.tenant_key ?? NONE) = $tenant_key
                AND request.owner.data_labels ALLINSIDE $labels {context} LIMIT 1;",
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
            // The 3.2.4 task/time index can include rows whose leading Task key
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
