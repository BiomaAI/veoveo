//! Current Reason terminal results and authorized Task delivery.
use futures::StreamExt;
use rmcp::{
    ErrorData as McpError,
    model::{GetTaskParams, GetTaskResult, TaskPayload},
};
use veoveo_reason_mcp::contract::ReasonTaskKind;
use veoveo_task_runtime::{
    DurableTaskSubscription, TaskOwner, TaskRuntime, TaskSnapshot, TaskStatus, authorized_snapshot,
    project_snapshot, subscribe_durable_tasks,
};
use veoveo_types::TaskTypeDefinition;

use super::{internal, resources::analysis_view};

#[cfg(test)]
pub(super) use veoveo_reason_mcp::task_product::ANALYSIS_COMPLETED;
pub(super) use veoveo_reason_mcp::task_product::analysis_tool_result;

pub(super) async fn get_task(
    runtime: &TaskRuntime,
    owner: &TaskOwner,
    request: GetTaskParams,
) -> Result<GetTaskResult, McpError> {
    let snapshot = authorized_snapshot(
        &runtime
            .for_owner(owner)
            .of_type(ReasonTaskKind::AnalyzeRecording.name()),
        &request.task_id,
    )
    .await?;
    validate_snapshot(&snapshot)?;
    project_snapshot(runtime, snapshot)
        .await
        .map(GetTaskResult::new)
        .map_err(internal)
}

pub(super) async fn subscribe_tasks(
    runtime: &TaskRuntime,
    owner: TaskOwner,
    task_ids: Vec<String>,
) -> Result<DurableTaskSubscription, McpError> {
    let subscription = subscribe_durable_tasks(
        &runtime
            .for_owner(&owner)
            .of_type(ReasonTaskKind::AnalyzeRecording.name()),
        task_ids,
    )
    .await?;
    let runtime = runtime.clone();
    let updates = subscription.updates.then(move |task| {
        let runtime = runtime.clone();
        let owner = owner.clone();
        async move {
            let task = task?;
            if !matches!(task.payload, TaskPayload::Completed { .. }) {
                return Ok(task);
            }
            // Recheck current authorization before decoding the retained domain
            // envelope; the watch's older observation conveys no new authority.
            let snapshot = authorized_snapshot(
                &runtime
                    .for_owner(&owner)
                    .of_type(ReasonTaskKind::AnalyzeRecording.name()),
                &task.task.task_id,
            )
            .await?;
            validate_snapshot(&snapshot)?;
            project_snapshot(&runtime, snapshot).await.map_err(internal)
        }
    });
    Ok(DurableTaskSubscription {
        accepted_task_ids: subscription.accepted_task_ids,
        updates: Box::pin(updates),
    })
}

fn validate_snapshot(snapshot: &TaskSnapshot) -> Result<(), McpError> {
    if snapshot.status == TaskStatus::Succeeded {
        analysis_view(snapshot)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "task_results_tests.rs"]
mod tests;
