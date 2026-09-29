//! Domain validation between current SQL selection and public Task projection.
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use futures::StreamExt;
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, ContentBlock, DetailedTask, GetTaskParams, GetTaskResult},
};
use serde::Deserialize;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_task_runtime::{
    DurableTaskSubscription, TaskRuntime, TaskSnapshot, TaskStatus, authorized_snapshot,
    project_snapshot, subscribe_authorized_snapshots,
};

use super::{request::ViewCaptureTaskRequest, task_query};
use crate::{
    contract::{CapturedFrame, FrameRecord, FrameRenderReport},
    mcp::frame_tool_result,
};

pub(super) async fn get_task(
    runtime: &TaskRuntime,
    identity: &GatewayInternalIdentity,
    request: GetTaskParams,
) -> Result<GetTaskResult, McpError> {
    let snapshot = authorized_snapshot(&task_query(runtime, identity)?, &request.task_id).await?;
    project_checked(runtime, snapshot)
        .await
        .map(GetTaskResult::new)
}

pub(super) async fn subscribe_tasks(
    runtime: &TaskRuntime,
    identity: &GatewayInternalIdentity,
    task_ids: Vec<String>,
) -> Result<DurableTaskSubscription, McpError> {
    let subscription =
        subscribe_authorized_snapshots(&task_query(runtime, identity)?, task_ids).await?;
    let runtime = runtime.clone();
    let updates = subscription.updates.then(move |update| {
        let runtime = runtime.clone();
        async move {
            let update = update
                .map_err(|_| McpError::internal_error("could not read current View Task", None))?;
            project_checked(&runtime, update.snapshot).await
        }
    });
    Ok(DurableTaskSubscription {
        accepted_task_ids: subscription
            .accepted_task_ids
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
        updates: Box::pin(updates),
    })
}

async fn project_checked(
    runtime: &TaskRuntime,
    snapshot: TaskSnapshot,
) -> Result<DetailedTask, McpError> {
    if snapshot.status == TaskStatus::Succeeded {
        validate_completed(&snapshot).map_err(|error| {
            // Error variants contain no retained request, image bytes or provider data.
            McpError::internal_error(format!("invalid stored View capture result: {error}"), None)
        })?;
    }
    project_snapshot(runtime, snapshot)
        .await
        .map_err(|_| McpError::internal_error("could not project current View Task", None))
}

fn validate_completed(snapshot: &TaskSnapshot) -> Result<(), CaptureResultError> {
    let request = ViewCaptureTaskRequest::deserialize(&snapshot.request)
        .map_err(|_| CaptureResultError::Request)?;
    request
        .validate_owner(&snapshot.owner)
        .map_err(|_| CaptureResultError::Owner)?;
    let stored = snapshot
        .result
        .as_ref()
        .ok_or(CaptureResultError::Missing)?;
    let result = CallToolResult::deserialize(stored).map_err(|_| CaptureResultError::Envelope)?;
    let [ContentBlock::Text(_), ContentBlock::Image(image)] = result.content.as_slice() else {
        return Err(CaptureResultError::Envelope);
    };
    let record = FrameRecord::deserialize(
        result
            .structured_content
            .as_ref()
            .ok_or(CaptureResultError::Frame)?,
    )
    .map_err(|_| CaptureResultError::Frame)?;
    let bytes = BASE64_STANDARD
        .decode(&image.data)
        .map_err(|_| CaptureResultError::Image)?;
    let capture = request.request();
    let parent = request.snapshot();
    // Rebuild through the same owning constructor as capture. This binds every
    // repeated field to the admitted input and computes byte length and digest.
    let frame = CapturedFrame::builder(
        record.frame_id().clone(),
        parent.view(),
        parent.composition().record(),
        capture.scene_time,
        &capture.policy,
    )
    .map_err(|_| CaptureResultError::Frame)?
    .finish(
        record.captured_at(),
        FrameRenderReport {
            detail_complete: record.detail_complete(),
            actual_max_screen_error_px: record.actual_max_screen_error_px(),
            visible_tile_count: record.visible_tile_count(),
            pending_tile_count: record.pending_tile_count(),
            rendered_overlay_count: record.rendered_overlay_count(),
            overlay_truncated: record.overlay_truncated(),
            attribution: record.attribution().clone(),
        },
        record.encoding(),
        bytes,
    )
    .map_err(|_| CaptureResultError::Frame)?;
    let expected = frame_tool_result(&frame).map_err(|_| CaptureResultError::Envelope)?;
    let expected = serde_json::to_value(expected).map_err(|_| CaptureResultError::Envelope)?;
    if &expected != stored {
        return Err(CaptureResultError::Binding);
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum CaptureResultError {
    #[error("capture request failed admission")]
    Request,
    #[error("capture owner disagrees with its Task")]
    Owner,
    #[error("completed Task has no result")]
    Missing,
    #[error("completion envelope is invalid")]
    Envelope,
    #[error("frame metadata failed admission")]
    Frame,
    #[error("image payload is invalid")]
    Image,
    #[error("completion disagrees with the admitted capture or image bytes")]
    Binding,
}

#[cfg(test)]
mod tests;
