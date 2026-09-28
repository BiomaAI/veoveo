//! Timeseries usage reads under the linked Task's current owner policy.
use veoveo_platform_store::DomainUsageRecord;
use veoveo_task_runtime::TaskUsageAccess;
use veoveo_task_runtime::{TaskOwner, TaskRuntime};

use crate::contract::{
    TIMESERIES_USAGE_PAGE_SIZE, TimeseriesTaskUsageUri, TimeseriesUsageCursor, TimeseriesUsagePage,
};

/// A reader bound to the Timeseries Task ledger. Domain policy is the same
/// principal/profile/tenant/label policy as `TaskOwner::allows`.
pub struct TimeseriesUsage<'a> {
    tasks: &'a TaskRuntime,
}

impl<'a> TimeseriesUsage<'a> {
    pub fn new(tasks: &'a TaskRuntime) -> anyhow::Result<Self> {
        anyhow::ensure!(
            tasks.server() == "timeseries",
            "expected Timeseries Task runtime"
        );
        Ok(Self { tasks })
    }

    pub async fn page(
        &self,
        owner: &TaskOwner,
        cursor: Option<&TimeseriesUsageCursor>,
    ) -> anyhow::Result<TimeseriesUsagePage> {
        let page = self
            .tasks
            .usage_page(
                TaskUsageAccess::Owner(owner),
                cursor.map(TimeseriesUsageCursor::after),
                TIMESERIES_USAGE_PAGE_SIZE,
            )
            .await?;
        Ok(TimeseriesUsagePage::from_task_ids(
            page.task_ids,
            page.next_task_id,
        )?)
    }

    pub async fn task(
        &self,
        owner: &TaskOwner,
        uri: &TimeseriesTaskUsageUri,
    ) -> anyhow::Result<Vec<DomainUsageRecord>> {
        Ok(self
            .tasks
            .usage(TaskUsageAccess::Owner(owner), uri.task_id())
            .await?)
    }
}
