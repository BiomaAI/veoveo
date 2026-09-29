//! Current Task owner policy, applied before records cross the database boundary.
use super::{OwnerTaskQuery, TaskRuntime, owner_record, tenant_record};
use crate::types::{TaskError, TaskOwner, TaskSnapshot, record_to_snapshot, validate_task_id};
use std::collections::BTreeSet;
use surrealdb::{Connection, method::Query};
use veoveo_platform_store::{PlatformTable, RecordId, TaskRecord, task_record_id};
use veoveo_types::TaskId;

pub(super) const VISIBLE_TASK: &str = "server = $server AND tenant = $tenant
    AND owner = $owner AND profile = $profile
    AND request.owner.principal_key = $principal_key
    AND request.owner.profile = $profile_key
    AND (request.owner.tenant_key ?? NONE) = $tenant_key
    AND request.owner.data_labels ALLINSIDE $labels";

pub(super) struct OwnerScope {
    server: RecordId,
    tenant: RecordId,
    owner: RecordId,
    profile: RecordId,
    principal_key: String,
    profile_key: String,
    tenant_key: Option<String>,
    labels: BTreeSet<String>,
}

impl OwnerScope {
    pub(super) fn new(runtime: &TaskRuntime, owner: &TaskOwner) -> Result<Self, TaskError> {
        Ok(Self {
            server: RecordId::new(PlatformTable::McpServer.as_str(), runtime.server.clone()),
            tenant: tenant_record(owner)?,
            owner: owner_record(owner)?,
            profile: RecordId::new(PlatformTable::Profile.as_str(), owner.profile.clone()),
            principal_key: owner.principal_key.clone(),
            profile_key: owner.profile.clone(),
            tenant_key: owner.tenant_key.clone(),
            labels: owner.data_labels.clone(),
        })
    }

    pub(super) fn bind<C: Connection>(self, query: Query<'_, C>) -> Query<'_, C> {
        query
            .bind(("server", self.server))
            .bind(("tenant", self.tenant))
            .bind(("owner", self.owner))
            .bind(("profile", self.profile))
            .bind(("principal_key", self.principal_key))
            .bind(("profile_key", self.profile_key))
            .bind(("tenant_key", self.tenant_key))
            .bind(("labels", self.labels))
    }
}

impl OwnerTaskQuery {
    /// Select a current caller-owned Task before decoding its request or result.
    /// Missing and denied Tasks both return `None`.
    /// ```compile_fail
    /// use veoveo_task_runtime::{TaskOwner, TaskRuntime};
    /// async fn wrong(runtime: &TaskRuntime, owner: &TaskOwner) {
    ///     runtime.for_owner(owner).get("raw-task-id").await;
    /// }
    /// ```
    pub async fn get(&self, task: TaskId) -> Result<Option<TaskSnapshot>, TaskError> {
        let task = validate_task_id(task)?;
        let mut response = self
            .bind(self.runtime.store.client().query(format!(
                "SELECT * FROM task WHERE id = $task AND {VISIBLE_TASK} {} LIMIT 1;",
                self.selection_predicate()
            )))?
            .bind(("task", task_record_id(task)))
            .await?
            .check()?;
        let records: Vec<TaskRecord> = response.take(0)?;
        records
            .into_iter()
            .next()
            .map(record_to_snapshot)
            .transpose()
    }
}
