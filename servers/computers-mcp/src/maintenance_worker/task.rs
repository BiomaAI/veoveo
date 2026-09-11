use super::*;
use std::future::Future;
use veoveo_computers::maintenance::MaintenanceStep;
use veoveo_task_runtime::{TaskRetentionPin, TaskStatus, TaskTransition};

pub(super) fn message(step: MaintenanceStep) -> &'static str {
    match step {
        MaintenanceStep::Stop => "Stopping the current environment; files are retained",
        MaintenanceStep::Capture => "Saving the environment's access policy",
        MaintenanceStep::Retire => "Removing the stopped environment",
        MaintenanceStep::Transfer => "Transferring the retained home",
        MaintenanceStep::Create => "Starting the replacement environment",
        MaintenanceStep::Restore => "Restoring the environment's access policy",
    }
}
impl MaintenanceWorker {
    pub(super) async fn waiting(
        &self,
        operation: &MaintenanceOperation,
        message: &str,
    ) -> Result<()> {
        let id = operation.task_id().to_string();
        let task = self
            .tasks
            .get(&id)
            .await?
            .ok_or_else(|| TaskError::NotFound(id.clone()))?;
        if task.is_terminal()
            || task.status == TaskStatus::CancelRequested
            || task.status == TaskStatus::Waiting && task.status_message.as_deref() == Some(message)
        {
            return Ok(());
        }
        self.tasks
            .transition_if_current(
                &task,
                TaskTransition::Waiting {
                    message: message.into(),
                    progress: task.progress,
                },
            )
            .await?;
        Ok(())
    }
    pub(super) async fn project(&self, operation: &MaintenanceOperation) -> Result<()> {
        let transition = match operation.stage {
            MaintenanceStage::Succeeded => {
                let uri = veoveo_computers::api::computer_uri(operation.computer_id);
                let payload = veoveo_computers::api::MaintenanceResult {
                    result_uri: uri.clone(),
                    computer_id: operation.computer_id,
                    maintenance_id: operation.operation_id,
                    template_id: operation.target.template_id.clone(),
                };
                let mut result = rmcp::model::CallToolResult::structured(
                    serde_json::to_value(payload).map_err(|_| WorkerError::Configuration)?,
                );
                result.content = vec![
                    rmcp::model::ContentBlock::text(
                        "Environment updated; retained files are available",
                    ),
                    rmcp::model::ContentBlock::resource_link(rmcp::model::Resource::new(
                        uri, "Computer",
                    )),
                ];
                TaskTransition::Succeeded {
                    message: "Environment updated".into(),
                    result: serde_json::to_value(result).map_err(|_| WorkerError::Configuration)?,
                }
            }
            MaintenanceStage::Cancelled => TaskTransition::Cancelled,
            _ => return Err(WorkerError::Configuration),
        };
        self.tasks
            .transition(&operation.task_id().to_string(), transition)
            .await?;
        self.acknowledge(operation).await
    }
    pub(super) async fn acknowledge(&self, operation: &MaintenanceOperation) -> Result<()> {
        self.store.acknowledge_maintenance_task(operation).await?;
        self.tasks
            .acknowledge_retention_pin(
                &operation.task_id().to_string(),
                &TaskRetentionPin::new(format!("computer-maintenance/{}", operation.operation_id))
                    .map_err(|_| WorkerError::Configuration)?,
            )
            .await?;
        Ok(())
    }
    pub(super) async fn with_lease<F: Future>(
        &self,
        claim: &mut ClaimedTask,
        future: F,
    ) -> Result<F::Output> {
        tokio::pin!(future);
        let mut renewal = tokio::time::interval(Duration::from_secs(10));
        renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        renewal.tick().await;
        loop {
            tokio::select! {
                biased;
                _ = renewal.tick() => {
                    let snapshot = tokio::time::timeout(Duration::from_secs(5),self.tasks.renew_lease(&claim.snapshot.task_id.to_string(),LEASE))
                        .await.map_err(|_|WorkerError::LeaseLost)??;
                    claim.lease_expires_at = snapshot.lease_expires_at.ok_or(WorkerError::LeaseLost)?;
                    claim.snapshot = snapshot;
                }
                output = &mut future => return Ok(output),
            }
        }
    }
}
