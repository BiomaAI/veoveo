//! Finite startup recovery: lease deadlines are wake hints, never claim authority.
use super::*;
use chrono::{DateTime, Utc};
use futures::Stream;
use std::{
    collections::{BTreeMap, BTreeSet},
    pin::Pin,
};
use surrealdb::types::RecordId;
use veoveo_types::TaskId;

pub type TaskRecoveryStream =
    Pin<Box<dyn Stream<Item = Result<RecoveryReport, TaskError>> + Send + 'static>>;

#[derive(SurrealValue)]
struct RetainedTask {
    id: RecordId,
    lease_expires_at: Option<DateTime<Utc>>,
}

impl TaskRuntime {
    /// Observe only nonterminal identities present at this startup baseline.
    /// The first report is immediate. Later reports revisit skipped leases, using
    /// current SQL selection and the existing recovery/claim compare-and-set guards.
    /// Dropping the stream retires its native-feed subscription.
    pub async fn observe_startup_recovery(&self) -> Result<TaskRecoveryStream, TaskError> {
        self.check_required_contributions()?;
        // Register the native source before reading the baseline; a write during
        // baseline admission then remains a pending wake.
        let mut wake = self.task_wake().await?;
        let mut response = self
            .platform_store()
            .client()
            .query(include_str!("../../queries/recovery/baseline.surql"))
            .bind((
                "server",
                RecordId::new("mcp_server", self.server().to_owned()),
            ))
            .await?
            .check()?;
        let baseline: Vec<RetainedTask> = response.take(0)?;
        let mut retained = baseline
            .into_iter()
            .map(|row| crate::types::task_id_from_record(&row.id))
            .collect::<Result<BTreeSet<TaskId>, _>>()?;
        let runtime = self.clone();
        Ok(Box::pin(async_stream::try_stream! {
            loop {
                let records = retained.iter().copied().map(task_record_id).collect::<Vec<_>>();
                let mut response = runtime.platform_store().client()
                    .query(include_str!("../../queries/recovery/retained.surql"))
                    .bind(("server", RecordId::new("mcp_server", runtime.server().to_owned())))
                    .bind(("records", records.clone()))
                    .await?.check()?;
                let rows: Vec<RetainedTask> = response.take(0)?;
                let current = rows.into_iter().map(|row| {
                    crate::types::task_id_from_record(&row.id).map(|id| (id, row.lease_expires_at))
                }).collect::<Result<BTreeMap<_, _>, _>>()?;
                retained = current.keys().copied().collect();
                let mut response = runtime.platform_store().client()
                    .query(include_str!("../../queries/recovery/eligible.surql"))
                    .bind(("server", RecordId::new("mcp_server", runtime.server().to_owned())))
                    .bind(("records", records))
                    .bind(("now", Utc::now()))
                    .await?.check()?;
                let eligible: Vec<TaskRecord> = response.take(0)?;
                let snapshots = eligible.into_iter().map(record_to_snapshot).collect::<Result<_, _>>()?;
                let report = runtime.recover_selected(snapshots).await?;
                for task in report.resumable.iter().chain(&report.webhook_waiting)
                    .chain(&report.provider_waiting).chain(&report.failed_indeterminate).chain(&report.cancelled) {
                    retained.remove(&task.task_id);
                }
                let deadline = current.iter().filter_map(|(id, expiry)| {
                    retained.contains(id).then_some(*expiry).flatten()
                }).min();
                yield report;
                if retained.is_empty() { break; }
                match deadline {
                    Some(deadline) => {
                        let delay = (deadline - Utc::now()).to_std().unwrap_or_default();
                        let changed = tokio::select! {
                            _ = tokio::time::sleep(delay) => Ok(()),
                            changed = wake.changed() => {
                                changed.map_err(|_| TaskError::InvalidRecord("Task recovery wake source closed".into()))
                            }
                        };
                        changed?;
                    }
                    None => {
                        wake.changed().await.map_err(|_| TaskError::InvalidRecord("Task recovery wake source closed".into()))?;
                    }
                }
            }
        }))
    }
}
