//! Current Reason terminal results and authorized Task delivery.
// TODO(foundations): qualify current result delivery and GPU completion on the
// rebuilt reference installation through its runbook.
use futures::StreamExt;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, ContentBlock, GetTaskParams, GetTaskResult, Resource, TaskPayload},
};
use veoveo_mcp_contract::set_related_task_meta;
use veoveo_reason_mcp::contract::AnalyzeRecordingOutput;
use veoveo_reason_mcp::contract::ReasonTaskKind;
use veoveo_task_runtime::{
    DurableTaskSubscription, TaskOwner, TaskRuntime, TaskSnapshot, TaskStatus, authorized_snapshot,
    project_snapshot, subscribe_durable_tasks,
};
use veoveo_types::TaskTypeDefinition;

use super::{internal, resources::analysis_view};

pub(super) const ANALYSIS_COMPLETED: &str = "Analysis completed.";

pub(super) fn analysis_tool_result(
    output: AnalyzeRecordingOutput,
) -> anyhow::Result<CallToolResult> {
    let mut result = CallToolResult::success(vec![
        ContentBlock::text(ANALYSIS_COMPLETED),
        ContentBlock::ResourceLink(
            Resource::new(output.result_uri().to_string(), "analysis_result")
                .with_title("Reason analysis result")
                .with_mime_type("application/vnd.veoveo.reason-results+json"),
        ),
    ]);
    set_related_task_meta(&mut result.meta, output.analysis_id().to_string());
    result.structured_content = Some(serde_json::to_value(output)?);
    Ok(result)
}

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
