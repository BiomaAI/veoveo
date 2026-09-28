//! DuckDb usage reads under the linked Task's current owner policy.
use veoveo_platform_store::DomainUsageRecord;
use veoveo_task_runtime::{TaskOwner, TaskRuntime};

use crate::contract::{
    DUCKDB_USAGE_PAGE_SIZE, DuckDbTaskUsageUri, DuckDbUsageCursor, DuckDbUsagePage,
};

/// A reader bound to the DuckDb Task ledger. Domain policy is the same
/// principal/profile/tenant/label policy as `TaskOwner::allows`.
pub struct DuckDbUsage<'a> {
    tasks: &'a TaskRuntime,
}

impl<'a> DuckDbUsage<'a> {
    pub fn new(tasks: &'a TaskRuntime) -> anyhow::Result<Self> {
        anyhow::ensure!(tasks.server() == "duckdb", "expected DuckDb Task runtime");
        Ok(Self { tasks })
    }

    pub async fn page(
        &self,
        owner: &TaskOwner,
        cursor: Option<&DuckDbUsageCursor>,
    ) -> anyhow::Result<DuckDbUsagePage> {
        let page = self
            .tasks
            .usage_page_for_owner(
                owner,
                cursor.map(DuckDbUsageCursor::after),
                DUCKDB_USAGE_PAGE_SIZE,
            )
            .await?;
        Ok(DuckDbUsagePage::from_task_ids(
            page.task_ids,
            page.next_task_id,
        )?)
    }

    pub async fn task(
        &self,
        owner: &TaskOwner,
        uri: &DuckDbTaskUsageUri,
    ) -> anyhow::Result<Vec<DomainUsageRecord>> {
        Ok(self.tasks.usage_for_owner(owner, uri.task_id()).await?)
    }
}
