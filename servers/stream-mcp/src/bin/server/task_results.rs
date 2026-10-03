//! Stream product handoffs and current, owner-authorized Task delivery.
use futures::StreamExt;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, ContentBlock, GetTaskParams, GetTaskResult, Resource, TaskPayload},
};
use veoveo_mcp_contract::set_related_task_meta;
use veoveo_stream_mcp::contract::StreamTaskKind;
use veoveo_stream_mcp::{
    annotation::RESULTS_MIME_TYPE,
    contract::{
        RunDetails, RunId, RunRecordingOutput, RunView, StartLiveSessionOutput,
        StopLiveSessionOutput,
    },
};
use veoveo_task_runtime::{
    DurableTaskSubscription, TaskOwner, TaskPayloadState, TaskRuntime, TaskSnapshot, TaskStatus,
    authorized_snapshot, project_snapshot, subscribe_durable_tasks,
};
use veoveo_types::ResourceUri;
use veoveo_types::TaskTypeDefinition;

use super::{
    internal,
    tasks::{DurableStreamRequest, StreamTaskInput},
};

pub(super) const RUN_COMPLETED: &str = "Recording run completed.";

fn product_result(
    status: &'static str,
    uri: ResourceUri,
    title: &'static str,
    mime_type: &'static str,
    output: impl serde::Serialize,
) -> anyhow::Result<CallToolResult> {
    let mut result = CallToolResult::success(vec![
        ContentBlock::text(status),
        ContentBlock::ResourceLink(
            Resource::new(uri.to_string(), "result")
                .with_title(title)
                .with_mime_type(mime_type),
        ),
    ]);
    result.structured_content = Some(serde_json::to_value(output)?);
    Ok(result)
}

pub(super) fn recording_result(output: RunRecordingOutput) -> anyhow::Result<CallToolResult> {
    let id = output.run_id();
    let mut result = product_result(
        RUN_COMPLETED,
        output.result_uri().to_uri(),
        "Stream recording results",
        RESULTS_MIME_TYPE,
        output,
    )?;
    set_related_task_meta(&mut result.meta, id.to_string());
    Ok(result)
}

pub(super) fn live_started_result(
    output: StartLiveSessionOutput,
) -> anyhow::Result<CallToolResult> {
    product_result(
        "Live session started.",
        output.result_uri().to_uri(),
        "Stream session",
        "application/json",
        output,
    )
}

pub(super) fn live_stopped_result(output: StopLiveSessionOutput) -> anyhow::Result<CallToolResult> {
    product_result(
        "Live session stopped.",
        output.result_uri.to_uri(),
        "Stream session",
        "application/json",
        output,
    )
}

pub(super) async fn get_task(
    runtime: &TaskRuntime,
    owner: &TaskOwner,
    request: GetTaskParams,
) -> Result<GetTaskResult, McpError> {
    let snapshot = authorized_snapshot(
        &runtime
            .for_owner(owner)
            .of_type(StreamTaskKind::RunRecording.name()),
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
            .of_type(StreamTaskKind::RunRecording.name()),
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
            // Reauthorize through SQL before decoding a retained product. An older
            // watch observation cannot confer current access to that product.
            let snapshot = authorized_snapshot(
                &runtime
                    .for_owner(&owner)
                    .of_type(StreamTaskKind::RunRecording.name()),
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
        run_view(snapshot)?;
    }
    Ok(())
}

pub(super) fn run_view(snapshot: &TaskSnapshot) -> Result<RunView, McpError> {
    let request: DurableStreamRequest =
        serde_json::from_value(snapshot.request.clone()).map_err(|_| retained_output_error())?;
    let StreamTaskInput::RunRecording(input) = request.input;
    let output = retained_result(snapshot)?
        .as_ref()
        .map(run_output)
        .transpose()?
        .flatten();
    RunView::new(
        RunId::try_from(snapshot.task_id).map_err(|_| retained_output_error())?,
        input.pipeline_id,
        RunDetails {
            status: task_status(snapshot.status).to_owned(),
            progress: snapshot.progress,
            recording_uri: input.video.recording_uri,
            entity_path: input.video.entity_path,
            timeline: input.video.timeline,
            created_at: snapshot.created_at.to_rfc3339(),
            updated_at: snapshot.updated_at.to_rfc3339(),
        },
    )
    .with_error(snapshot.error.as_ref().map(|error| error.message.clone()))
    .with_output(output)
    .map_err(|_| retained_output_error())
}

fn retained_result(snapshot: &TaskSnapshot) -> Result<Option<CallToolResult>, McpError> {
    match &snapshot.result {
        Some(result) => serde_json::from_value(result.clone())
            .map(Some)
            .map_err(|_| retained_output_error()),
        None if snapshot.status == TaskStatus::Succeeded => Err(retained_output_error()),
        None => Ok(None),
    }
}

fn run_output(result: &CallToolResult) -> Result<Option<RunRecordingOutput>, McpError> {
    if result.is_error == Some(true) {
        return Ok(None);
    }
    let output: RunRecordingOutput = serde_json::from_value(
        result
            .structured_content
            .clone()
            .ok_or_else(retained_output_error)?,
    )
    .map_err(|_| retained_output_error())?;
    let expected = recording_result(output.clone()).map_err(|_| retained_output_error())?;
    if result.content != expected.content {
        return Err(retained_output_error());
    }
    Ok(Some(output))
}

fn retained_output_error() -> McpError {
    McpError::internal_error(
        "stored Stream output does not satisfy the current contract",
        None,
    )
}

fn task_status(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "queued",
        TaskStatus::Running => "running",
        TaskStatus::Waiting => "waiting",
        TaskStatus::Succeeded => "succeeded",
        TaskStatus::Failed => "failed",
        TaskStatus::CancelRequested => "cancel_requested",
        TaskStatus::Cancelled => "cancelled",
    }
}

pub(super) async fn completed_payload(
    runtime: &TaskRuntime,
    owner: &TaskOwner,
    id: RunId,
) -> Result<CallToolResult, McpError> {
    let snapshot = authorized_snapshot(
        &runtime
            .for_owner(owner)
            .of_type(StreamTaskKind::RunRecording.name()),
        &id.to_string(),
    )
    .await?;
    validate_snapshot(&snapshot)?;
    match runtime
        .await_payload_state(id.task_id())
        .await
        .map_err(internal)?
    {
        TaskPayloadState::Completed(_) => {
            let snapshot = authorized_snapshot(
                &runtime
                    .for_owner(owner)
                    .of_type(StreamTaskKind::RunRecording.name()),
                &id.to_string(),
            )
            .await?;
            validate_snapshot(&snapshot)?;
            retained_result(&snapshot)?.ok_or_else(retained_output_error)
        }
        TaskPayloadState::Failed(error) => {
            Err(McpError::internal_error(error.message, error.details))
        }
        TaskPayloadState::Cancelled => {
            Err(McpError::invalid_request("stream task was cancelled", None))
        }
        TaskPayloadState::Running => Err(internal("stream task wait ended while still running")),
        TaskPayloadState::Unknown => Err(internal("stream task disappeared before completion")),
    }
}

#[cfg(test)]
#[path = "task_results_tests.rs"]
mod tests;
