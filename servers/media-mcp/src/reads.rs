//! SQL-owned Media visibility. Cursors carry position, never caller authority.
mod predictions;
mod usage;

use surrealdb::{Connection, method::Query};
use veoveo_platform_store::{RecordId, deterministic_principal_id, deterministic_tenant_id};
use veoveo_task_runtime::{TaskOwner, TaskRuntime, TaskUsageAccess};
use veoveo_types::TaskId;

// Matches TaskOwner::allows, including the optional tenant spelling in the envelope.
// Both the domain row and its linked Task must agree; no full Task decode is needed.
const VISIBLE_TASK: &str = "tenant = $tenant AND task.tenant = $tenant
    AND task.server = $server AND task.owner = $owner AND task.profile = $profile
    AND task.request.owner.principal_key = $principal_key
    AND task.request.owner.profile = $profile_key
    AND (task.request.owner.tenant_key ?? NONE) = $tenant_key
    AND task.request.owner.data_labels ALLINSIDE $labels";

pub struct MediaReads<'a> {
    tasks: &'a TaskRuntime,
}
impl<'a> MediaReads<'a> {
    pub fn new(tasks: &'a TaskRuntime) -> anyhow::Result<Self> {
        anyhow::ensure!(tasks.server() == "media", "expected Media Task runtime");
        Ok(Self { tasks })
    }

    /// Subscription admission before a Task has produced its first usage row.
    pub async fn task_visible(&self, owner: &TaskOwner, task: TaskId) -> anyhow::Result<bool> {
        Ok(self
            .tasks
            .task_visible(TaskUsageAccess::Owner(owner), task)
            .await?)
    }
}

fn bind_owner<'q, C: Connection>(
    query: Query<'q, C>,
    owner: &TaskOwner,
) -> anyhow::Result<Query<'q, C>> {
    Ok(query
        .bind((
            "tenant",
            deterministic_tenant_id(owner.tenant_key())?.record_id(),
        ))
        .bind((
            "owner",
            deterministic_principal_id(owner.tenant_key(), &owner.principal_key)?.record_id(),
        ))
        .bind(("server", RecordId::new("mcp_server", "media")))
        .bind(("provider", "media".to_owned()))
        .bind(("profile", RecordId::new("profile", owner.profile.clone())))
        .bind(("principal_key", owner.principal_key.clone()))
        .bind(("profile_key", owner.profile.clone()))
        .bind(("tenant_key", owner.tenant_key.clone()))
        .bind(("labels", owner.data_labels.clone())))
}
