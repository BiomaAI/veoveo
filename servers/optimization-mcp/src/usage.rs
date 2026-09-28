//! Optimization usage reads under the linked Task's current owner and Work Context policy.
use veoveo_platform_store::DomainUsageRecord;
use veoveo_task_runtime::TaskUsageAccess;
use veoveo_task_runtime::{TaskOwner, TaskRuntime};

use crate::contract::{
    OPTIMIZATION_USAGE_PAGE_SIZE, OptimizationTaskUsageUri, OptimizationUsageCursor,
    OptimizationUsagePage,
};

/// A reader bound to the Optimization Task ledger. Domain policy requires
/// owner, profile, tenant and labels plus agreement with the current Work Context.
pub struct OptimizationUsage<'a> {
    tasks: &'a TaskRuntime,
}

impl<'a> OptimizationUsage<'a> {
    pub fn new(tasks: &'a TaskRuntime) -> anyhow::Result<Self> {
        anyhow::ensure!(
            tasks.server() == "optimization",
            "expected Optimization Task runtime"
        );
        Ok(Self { tasks })
    }

    pub async fn page(
        &self,
        owner: &TaskOwner,
        cursor: Option<&OptimizationUsageCursor>,
    ) -> anyhow::Result<OptimizationUsagePage> {
        let page = self
            .tasks
            .usage_page(
                TaskUsageAccess::WorkContext(owner),
                cursor.map(OptimizationUsageCursor::after),
                OPTIMIZATION_USAGE_PAGE_SIZE,
            )
            .await?;
        Ok(OptimizationUsagePage::from_task_ids(
            page.task_ids,
            page.next_task_id,
        )?)
    }

    pub async fn task(
        &self,
        owner: &TaskOwner,
        uri: &OptimizationTaskUsageUri,
    ) -> anyhow::Result<Vec<DomainUsageRecord>> {
        Ok(self
            .tasks
            .usage(TaskUsageAccess::WorkContext(owner), uri.task_id())
            .await?)
    }
}
