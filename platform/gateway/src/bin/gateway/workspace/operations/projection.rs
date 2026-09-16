use super::super::projection::uuid;
use axum::http::StatusCode;
use base64::{Engine, engine::general_purpose::STANDARD};
use rmcp::model::{
    CallToolResult, ContentBlock, DetailedTask, InputRequiredResult, TaskPayload, TaskStatus,
};
use veoveo_mcp_contract::workspace as wire;
use veoveo_platform_store::workspace::{WorkspaceOperation, WorkspaceOperationPhase as Phase};

pub(super) fn summary(value: &WorkspaceOperation) -> Result<wire::OperationSummary, StatusCode> {
    Ok(wire::OperationSummary {
        id: wire::OperationId(uuid(&value.id)?),
        chat_id: wire::ChatId(uuid(&value.chat)?),
        run_id: value.run.as_ref().map(uuid).transpose()?.map(wire::RunId),
        tool: value.tool.clone(),
        phase: match value.phase {
            Phase::Dispatching => wire::OperationPhase::Dispatching,
            Phase::InputRequired => wire::OperationPhase::InputRequired,
            Phase::Task => wire::OperationPhase::Task,
            Phase::Completed => wire::OperationPhase::Completed,
            Phase::Failed => wire::OperationPhase::Failed,
            Phase::Unconfirmed => wire::OperationPhase::Unconfirmed,
        },
        revision: value.revision,
        created_at: value.created_at,
    })
}

pub(super) fn stored(value: &WorkspaceOperation) -> Result<wire::OperationView, StatusCode> {
    let mut view = wire::OperationView {
        operation: summary(value)?,
        task: None,
        inputs: vec![],
        result: None,
    };
    match value.phase {
        Phase::InputRequired => {
            let input: InputRequiredResult = decode(value)?;
            if let Some(requests) = input.input_requests {
                view.inputs = super::inputs::project(&requests)?;
            }
        }
        Phase::Completed => view.result = Some(result(decode(value)?)),
        _ => {}
    }
    Ok(view)
}

pub(super) fn decode<T: serde::de::DeserializeOwned>(
    operation: &WorkspaceOperation,
) -> Result<T, StatusCode> {
    serde_json::from_str(
        operation
            .response
            .as_deref()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?,
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub(super) fn task(
    value: &WorkspaceOperation,
    task: DetailedTask,
) -> Result<wire::OperationView, StatusCode> {
    let mut view = stored(value)?;
    view.task = Some(wire::TaskView {
        id: task.task.task_id,
        state: match task.task.status {
            TaskStatus::Working => wire::TaskState::Working,
            TaskStatus::InputRequired => wire::TaskState::InputRequired,
            TaskStatus::Completed => wire::TaskState::Completed,
            TaskStatus::Failed => wire::TaskState::Failed,
            TaskStatus::Cancelled => wire::TaskState::Cancelled,
            _ => return Err(StatusCode::BAD_GATEWAY),
        },
        message: task.task.status_message,
        created_at: task.task.created_at,
        updated_at: task.task.last_updated_at,
        ttl_ms: task.task.ttl_ms,
        poll_interval_ms: task.task.poll_interval_ms,
    });
    match task.payload {
        TaskPayload::InputRequired { input_requests } => {
            view.inputs = super::inputs::project(&input_requests)?
        }
        TaskPayload::Completed { result: payload } => {
            view.result = Some(result(
                serde_json::from_value(serde_json::Value::Object(payload))
                    .map_err(|_| StatusCode::BAD_GATEWAY)?,
            ));
        }
        _ => {}
    }
    Ok(view)
}

fn result(result: CallToolResult) -> wire::OperationResult {
    let mut output = wire::OperationResult {
        is_error: result.is_error.unwrap_or(false),
        text: vec![],
        resources: vec![],
        images: vec![],
        omitted_images: 0,
        structured: result.structured_content,
    };
    let mut image_bytes = 0;
    for content in result.content {
        match content {
            ContentBlock::Text(text) => output.text.push(text.text),
            ContentBlock::ResourceLink(resource) => {
                output.resources.push(wire::OperationResource {
                    uri: resource.uri,
                    name: resource.name,
                    mime_type: resource.mime_type,
                })
            }
            ContentBlock::Image(image) => {
                let mime_type = match image.mime_type.as_str() {
                    "image/png" => Some(wire::ResultImageMime::Png),
                    "image/jpeg" => Some(wire::ResultImageMime::Jpeg),
                    "image/webp" => Some(wire::ResultImageMime::Webp),
                    _ => None,
                };
                if let Some(mime_type) = mime_type
                    && output.images.len() < 8
                    && image.data.len() <= 1024 * 1024 - image_bytes
                    && !image.data.is_empty()
                    && STANDARD.decode(&image.data).is_ok()
                {
                    image_bytes += image.data.len();
                    output.images.push(wire::OperationImage {
                        mime_type,
                        data: image.data,
                    });
                } else {
                    output.omitted_images += 1;
                }
            }
            _ => {}
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(mime: &str, data: &str) -> ContentBlock {
        serde_json::from_value(serde_json::json!({"type":"image","mimeType":mime,"data":data}))
            .unwrap()
    }

    #[test]
    fn result_images_reject_active_content_malformed_data_and_excess_payload() {
        let large = "AAAA".repeat(256 * 1024);
        let projected = result(CallToolResult::success(vec![
            image("image/svg+xml", &STANDARD.encode("<svg/>")),
            image("image/png", "not base64"),
            image("image/png", ""),
            image("image/jpeg", &large),
            image("image/webp", "AAAA"),
            ContentBlock::text("The result metadata remains available."),
        ]));
        assert_eq!(projected.images.len(), 1);
        assert_eq!(projected.images[0].mime_type, wire::ResultImageMime::Jpeg);
        assert_eq!(projected.omitted_images, 4);
        assert_eq!(projected.text, ["The result metadata remains available."]);
        let projected = result(CallToolResult::success(vec![image("image/png", "AAAA"); 9]));
        assert_eq!(projected.images.len(), 8);
        assert_eq!(projected.omitted_images, 1);
    }
}
