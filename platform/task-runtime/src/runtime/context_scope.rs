//! Work Context agreement shared by Task observation and linked usage reads.
use crate::{TaskError, TaskOwner};
use surrealdb::{Connection, method::Query};
use veoveo_platform_store::{RecordId, deterministic_work_context_id};
use veoveo_types::{TenantId, WorkContextId};

#[derive(Clone)]
pub(super) struct ContextScope {
    record: RecordId,
    key: WorkContextId,
    tenant: TenantId,
}

impl ContextScope {
    pub(super) const TASK_PREDICATE: &str = "AND work_context = $work_context
        AND authority.context_key = $work_context_key
        AND request.owner.authority.work_context = $work_context_key
        AND request.owner.authority.tenant = $authority_tenant";

    pub(super) const USAGE_PREDICATE: &str = "AND task.work_context = $work_context
        AND task.authority.context_key = $work_context_key
        AND task.request.owner.authority.work_context = $work_context_key
        AND task.request.owner.authority.tenant = $authority_tenant";

    pub(super) fn new(owner: &TaskOwner) -> Result<Self, TaskError> {
        if owner.authority.tenant.as_str() != owner.tenant_key() {
            return Err(TaskError::InvalidAuthority(
                "task owner and Work Context belong to different tenants".into(),
            ));
        }
        Ok(Self {
            record: deterministic_work_context_id(
                owner.tenant_key(),
                owner.authority.work_context.as_str(),
            )?
            .record_id(),
            key: owner.authority.work_context.clone(),
            tenant: owner.authority.tenant.clone(),
        })
    }

    pub(super) fn bind<'a, C: Connection>(&self, query: Query<'a, C>) -> Query<'a, C> {
        // Scalar bindings preserve planner access to the compound indexes on 3.3.0.
        query
            .bind(("work_context", self.record.clone()))
            .bind(("work_context_key", self.key.to_string()))
            .bind(("authority_tenant", self.tenant.to_string()))
    }
}
