//! Media's MCP completion handoff, including retained Task result projection.
use crate::{contract::MediaGenerationResult, reads::MediaReads};
use futures::StreamExt;
use rmcp::{
    ErrorData as McpError,
    model::{
        CallToolResult, ContentBlock, DetailedTask, GetTaskParams, GetTaskResult, Resource,
        TaskPayload,
    },
};
use veoveo_mcp_contract::set_related_task_meta;
use veoveo_task_runtime::{DurableTaskSubscription, TaskOwner, TaskRuntime};

pub const GENERATION_COMPLETED: &str = "Generation completed.";

pub fn generation_tool_result(generation: MediaGenerationResult) -> anyhow::Result<CallToolResult> {
    let mut result = CallToolResult::success(vec![
        ContentBlock::text(GENERATION_COMPLETED),
        ContentBlock::ResourceLink(
            Resource::new(generation.result_uri().as_str(), "generation_result")
                .with_title("Media generation result")
                .with_mime_type("application/json"),
        ),
    ]);
    set_related_task_meta(&mut result.meta, generation.task_id().to_string());
    result.structured_content = Some(serde_json::to_value(generation)?);
    Ok(result)
}

pub async fn get_task(
    runtime: &TaskRuntime,
    owner: &TaskOwner,
    request: GetTaskParams,
) -> Result<GetTaskResult, McpError> {
    MediaReads::new(runtime).map_err(internal)?;
    let task = veoveo_task_runtime::get_durable_task(runtime, owner, request).await?;
    project_completed(runtime, owner, task.task)
        .await
        .map(GetTaskResult::new)
}

pub async fn subscribe_tasks(
    runtime: &TaskRuntime,
    owner: TaskOwner,
    task_ids: Vec<String>,
) -> Result<DurableTaskSubscription, McpError> {
    MediaReads::new(runtime).map_err(internal)?;
    let subscription =
        veoveo_task_runtime::subscribe_durable_tasks(runtime, owner.clone(), task_ids).await?;
    let runtime = runtime.clone();
    let updates = subscription.updates.then(move |task| {
        let runtime = runtime.clone();
        let owner = owner.clone();
        async move { project_completed(&runtime, &owner, task?).await }
    });
    Ok(DurableTaskSubscription {
        accepted_task_ids: subscription.accepted_task_ids,
        updates: Box::pin(updates),
    })
}

async fn project_completed(
    runtime: &TaskRuntime,
    owner: &TaskOwner,
    mut task: DetailedTask,
) -> Result<DetailedTask, McpError> {
    if !matches!(task.payload, TaskPayload::Completed { .. }) {
        return Ok(task);
    }
    let task_id = task
        .task
        .task_id
        .parse()
        .map_err(|_| McpError::internal_error("Media Task lacks native identity", None))?;
    let generation = MediaReads::new(runtime)
        .map_err(internal)?
        .generation_for_task(owner, task_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| McpError::invalid_params("unknown generation result", None))?;
    let result = serde_json::to_value(generation_tool_result(generation).map_err(internal)?)
        .map_err(internal)?;
    task.task.status_message = Some(GENERATION_COMPLETED.into());
    task.payload = TaskPayload::Completed {
        result: result
            .as_object()
            .expect("tool result is an object")
            .clone(),
    };
    Ok(task)
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}
