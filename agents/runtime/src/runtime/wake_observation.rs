//! Native wake changes and timers derived from the agent's durable queue.
use super::*;
use futures::stream::BoxStream;
use veoveo_platform_store::{ChangefeedConsumerId, StoreError};

#[derive(SurrealValue)]
struct DueWakes {
    now: DateTime<Utc>,
    pending: Option<DateTime<Utc>>,
    claimed: Option<DateTime<Utc>>,
}

impl AgentRuntime {
    /// Every item asks the scheduler to reconcile its fenced queue. A persisted
    /// cursor is acknowledged after the scheduler resumes this stream.
    pub async fn wake_changes(
        &self,
    ) -> Result<BoxStream<'static, std::result::Result<(), StoreError>>> {
        let consumer = ChangefeedConsumerId::new(format!("agent-wakes/{}", self.agent_id))?;
        let cursor = self.store.changefeed_checkpoint(&consumer).await?;
        let store = self.store.clone();
        let mut changes = store.observe_changes(
            vec![
                veoveo_modules::ObservationTable::from(crate::AgentObservationTable::Wake),
                veoveo_modules::ObservationTable::from(crate::AgentObservationTable::Agent),
                veoveo_modules::ObservationTable::from(crate::AgentObservationTable::ManagedAgent),
                veoveo_modules::ObservationTable::from(
                    crate::AgentObservationTable::AgentDefinition,
                ),
                veoveo_modules::ObservationTable::from(PlatformTable::Principal),
                veoveo_modules::ObservationTable::from(PlatformTable::Tenant),
                veoveo_modules::ObservationTable::from(PlatformTable::WorkContext),
            ],
            cursor,
        );
        Ok(Box::pin(async_stream::stream! {
            while let Some(delivery) = changes.next().await {
                match delivery {
                    Ok(delivery) => {
                        yield Ok(());
                        if let Err(error) = store.checkpoint_changes(&consumer, delivery.cursor()).await {
                            yield Err(error);
                        }
                    }
                    Err(error) => yield Err(error),
                }
            }
        }))
    }

    /// Read only the earliest pending availability and claimed lease expiry.
    /// Database time converts these deadlines to a duration without client skew.
    pub async fn next_wake_delay(&self) -> Result<Option<Duration>> {
        if self.managed.is_some()
            && self.managed_scheduler_mode().await? != crate::ManagedSchedulerMode::Running
        {
            return Ok(None);
        }
        let mut response = self
            .store
            .client()
            .query(include_str!(
                "../queries/runtime/wake_observation/next_wake_delay.surql"
            ))
            .bind(("agent", self.agent_id.record_id()))
            .await?
            .check()?;
        let row: DueWakes =
            response
                .take::<Option<DueWakes>>(0)?
                .ok_or(AgentRuntimeError::NotFound {
                    entity: "wake schedule",
                })?;
        Ok(row
            .pending
            .into_iter()
            .chain(row.claimed)
            .min()
            .map(|due| (due - row.now).to_std().unwrap_or(Duration::ZERO)))
    }
}
