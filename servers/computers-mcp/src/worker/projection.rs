use super::*;
use veoveo_task_runtime::{TaskFailure, TaskRetentionPin};
impl<G: Preflight> LifecycleWorker<G> {
    pub(super) async fn project(&self, operation: &Operation) -> Result<()> {
        let transition = match operation.stage {
            OperationStage::Succeeded => {
                let payload = veoveo_computers::api::LifecycleResult::new(
                    operation.computer_id,
                    operation.operation_id,
                    operation.action,
                );
                lifecycle_completion(payload)?
            }
            OperationStage::Cancelled => TaskTransition::Cancelled,
            OperationStage::Failed => TaskTransition::Failed(TaskFailure {
                code: "authority_denied".into(),
                message: "You don't have permission to perform this Computer action.".into(),
                details: None,
            }),
            _ => return Err(WorkerError::Configuration),
        };
        self.tasks
            .transition(operation.task_id(), transition)
            .await?;
        self.acknowledge(operation).await
    }
    pub(super) async fn acknowledge(&self, operation: &Operation) -> Result<()> {
        self.store.acknowledge_task_projection(operation).await?;
        self.tasks
            .acknowledge_retention_pin(
                operation.task_id(),
                &TaskRetentionPin::new(format!("computer-operation/{}", operation.operation_id))
                    .map_err(|_| WorkerError::Configuration)?,
            )
            .await?;
        Ok(())
    }
}

fn lifecycle_completion(payload: veoveo_computers::api::LifecycleResult) -> Result<TaskTransition> {
    let product_uri = payload.result_uri();
    let mut result = rmcp::model::CallToolResult::structured(
        serde_json::to_value(payload).map_err(|_| WorkerError::Configuration)?,
    );
    result.content = vec![rmcp::model::ContentBlock::text(
        "Computer operation completed",
    )];
    if let Some(uri) = product_uri {
        result
            .content
            .push(rmcp::model::ContentBlock::resource_link(
                rmcp::model::Resource::new(uri.to_uri(), "Computer"),
            ));
    }
    Ok(veoveo_task_runtime::mcp_task_completion(
        "Computer operation completed",
        result,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_computers::api::LifecycleResult;

    #[test]
    fn lifecycle_completion_admits_only_the_declared_product() {
        let computer_id = veoveo_computers::api::ComputerId::new();
        let operation_id = veoveo_types::TaskId::new();
        for action in [Action::Create, Action::Start, Action::Stop] {
            let payload = LifecycleResult::new(computer_id, operation_id, action);
            let expected_uri = payload.result_uri().map(|uri| uri.to_uri().to_string());
            assert_eq!(expected_uri.is_some(), action == Action::Create);
            let TaskTransition::Succeeded {
                message,
                result,
                result_uri,
            } = lifecycle_completion(payload.clone()).unwrap()
            else {
                panic!("lifecycle completion must succeed");
            };
            assert_eq!(message, "Computer operation completed");
            assert_eq!(result_uri.map(|uri| uri.to_string()), expected_uri);
            assert_eq!(result["content"][0]["text"], message);
            assert_eq!(
                serde_json::from_value::<LifecycleResult>(result["structuredContent"].clone())
                    .unwrap(),
                payload
            );
            if let Some(uri) = expected_uri {
                assert_eq!(result["structuredContent"]["resultUri"], uri);
                assert_eq!(result["content"].as_array().unwrap().len(), 2);
                assert_eq!(result["content"][1]["type"], "resource_link");
                assert_eq!(result["content"][1]["uri"], uri);
            } else {
                assert!(result["structuredContent"].get("resultUri").is_none());
                assert_eq!(result["content"].as_array().unwrap().len(), 1);
            }
        }
    }

    #[test]
    fn lifecycle_completion_boundary_rejects_missing_or_unexpected_product_links() {
        let computer_id = veoveo_computers::api::ComputerId::new();
        for action in [Action::Create, Action::Start, Action::Stop] {
            let payload = LifecycleResult::new(computer_id, veoveo_types::TaskId::new(), action);
            let TaskTransition::Succeeded { result, .. } = lifecycle_completion(payload).unwrap()
            else {
                panic!("lifecycle completion must succeed");
            };
            let mut result: rmcp::model::CallToolResult = serde_json::from_value(result).unwrap();
            if action == Action::Create {
                result.content.pop();
            } else {
                result
                    .content
                    .push(rmcp::model::ContentBlock::resource_link(
                        rmcp::model::Resource::new(
                            veoveo_computers::api::ComputerResultUri::new(computer_id).to_uri(),
                            "Computer",
                        ),
                    ));
            }
            assert!(
                veoveo_task_runtime::mcp_task_completion("Computer operation completed", result)
                    .is_err()
            );
        }
    }
}
