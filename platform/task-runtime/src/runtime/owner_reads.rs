//! Current Task owner policy, applied before records cross the database boundary.
use super::{
    OwnerTaskQuery, TaskRuntime, owner_query::OwnerSelection, owner_record, tenant_record,
};
use crate::types::{TaskError, TaskOwner, TaskSnapshot, record_to_snapshot, validate_task_id};
use std::collections::BTreeSet;
use surrealdb::{
    Connection,
    method::{Query, Transaction},
};
use veoveo_platform_store::{PlatformTable, RecordId, TaskRecord, task_record_id};
use veoveo_types::TaskId;

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
    /// Read at most 1000 typed identities under the current owner/context/operation policy.
    pub async fn get_many(
        &self,
        tasks: &[veoveo_types::TaskId],
    ) -> Result<Vec<TaskSnapshot>, TaskError> {
        self.read_batch(self.runtime.store.client().query(self.batch_sql()), tasks)
            .await
    }

    /// Read the same typed selection within the caller's native database transaction.
    /// Module catalog and Task hydration can therefore share one committed read view.
    pub async fn get_many_in<C: Connection>(
        &self,
        transaction: &Transaction<C>,
        tasks: &[veoveo_types::TaskId],
    ) -> Result<Vec<TaskSnapshot>, TaskError> {
        self.read_batch(transaction.query(self.batch_sql()), tasks)
            .await
    }

    fn batch_sql(&self) -> &'static str {
        match self.selection() {
            OwnerSelection::Owner => include_str!("../../queries/owner/batch_owner.surql"),
            OwnerSelection::Operations => {
                include_str!("../../queries/owner/batch_operations.surql")
            }
            OwnerSelection::Context => include_str!("../../queries/owner/batch_context.surql"),
            OwnerSelection::ContextOperations => {
                include_str!("../../queries/owner/batch_context_operations.surql")
            }
        }
    }

    async fn read_batch<C: Connection>(
        &self,
        query: Query<'_, C>,
        tasks: &[veoveo_types::TaskId],
    ) -> Result<Vec<TaskSnapshot>, TaskError> {
        if tasks.is_empty() {
            return Ok(Vec::new());
        }
        if tasks.len() > 1000 {
            return Err(TaskError::InvalidPageQuery);
        }
        let records = tasks
            .iter()
            .copied()
            .map(validate_task_id)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(task_record_id)
            .collect::<Vec<_>>();
        let mut response = self.bind(query)?.bind(("tasks", records)).await?.check()?;
        let records: Vec<TaskRecord> = response.take(0)?;
        records.into_iter().map(record_to_snapshot).collect()
    }

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
            .bind(self.runtime.store.client().query(match self.selection() {
                OwnerSelection::Owner => include_str!("../../queries/owner/get_owner.surql"),
                OwnerSelection::Operations => {
                    include_str!("../../queries/owner/get_operations.surql")
                }
                OwnerSelection::Context => include_str!("../../queries/owner/get_context.surql"),
                OwnerSelection::ContextOperations => {
                    include_str!("../../queries/owner/get_context_operations.surql")
                }
            }))?
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
