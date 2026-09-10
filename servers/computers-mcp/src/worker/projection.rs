use super::*;
use veoveo_task_runtime::{TaskFailure, TaskRetentionPin};
impl<G: Preflight> LifecycleWorker<G> {
    pub(super) async fn project(&self, operation: &Operation) -> Result<()> {
        let transition = match operation.stage {
            OperationStage::Succeeded => {
                let uri = veoveo_computers::api::computer_uri(operation.computer_id);
                let payload = veoveo_computers::api::LifecycleResult {
                    result_uri: (operation.action == Action::Create).then(|| uri.clone()),
                    computer_id: operation.computer_id,
                    operation_id: operation.operation_id,
                    action: operation.action,
                };
                let mut result = rmcp::model::CallToolResult::structured(
                    serde_json::to_value(payload).map_err(|_| WorkerError::Configuration)?,
                );
                result.content = vec![
                    rmcp::model::ContentBlock::text("Computer operation completed"),
                    rmcp::model::ContentBlock::resource_link(rmcp::model::Resource::new(
                        uri, "Computer",
                    )),
                ];
                TaskTransition::Succeeded {
                    message: "Computer operation completed".into(),
                    result: serde_json::to_value(result).map_err(|_| WorkerError::Configuration)?,
                }
            }
            OperationStage::Cancelled => TaskTransition::Cancelled,
            OperationStage::Failed => TaskTransition::Failed(TaskFailure {
                code: "authority_denied".into(),
                message: "Current authority does not permit this Computer action".into(),
                details: None,
            }),
            _ => return Err(WorkerError::Configuration),
        };
        self.tasks
            .transition(&operation.task_id().to_string(), transition)
            .await?;
        self.acknowledge(operation).await
    }
    pub(super) async fn acknowledge(&self, operation: &Operation) -> Result<()> {
        self.store.acknowledge_task_projection(operation).await?;
        self.tasks
            .acknowledge_retention_pin(
                &operation.task_id().to_string(),
                &TaskRetentionPin::new(format!("computer-operation/{}", operation.operation_id))
                    .map_err(|_| WorkerError::Configuration)?,
            )
            .await?;
        Ok(())
    }
}
