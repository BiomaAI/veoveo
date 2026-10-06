//! Resource dispatch and caller-scoped analysis reads.
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{CallToolResult, ReadResourceResult, ResourceContents},
    service::RequestContext,
};
use serde::Serialize;
use veoveo_platform_store::TaskStatus;
use veoveo_reason_mcp::contract::ReasonTaskKind;
use veoveo_reason_mcp::{
    catalog::{model_view, pipeline_view},
    contract::{
        AnalysisDetails, AnalysisId, AnalysisResource, AnalysisView, AnalyzeRecordingOutput,
        ReasonResource,
    },
    uris,
};
use veoveo_task_runtime::{TaskOwner, TaskRuntime, TaskSnapshot};
use veoveo_types::TaskTypeDefinition;

use veoveo_mcp_contract::hosting::{gateway_identity, plane_caller, served_by_host};

use super::{
    app_state::AppState,
    index, internal, invalid_params,
    ownership::runtime_owner,
    tasks::{self, ReasonTaskInput},
};

/// Reads one admitted address. The host serves documents and the contract.
pub(super) async fn read(
    state: &AppState,
    resource: ReasonResource,
    uri: &str,
    context: &RequestContext<RoleServer>,
) -> Result<ReadResourceResult, McpError> {
    let identity = gateway_identity(context)?;
    match resource {
        ReasonResource::Knowledge(address) => super::knowledge::read(state, address, context).await,
        ReasonResource::Docs | ReasonResource::Document(_) | ReasonResource::Contract => {
            Err(served_by_host())
        }
        ReasonResource::AnalysesApp => {
            let html = veoveo_mcp_apps_extension::workbench_app_html(
                &veoveo_mcp_apps_extension::WorkbenchApp {
                    app_id: "reason-analyses",
                    title: "Analyses",
                    subtitle: "Ask questions about recording ranges and review the model's answers",
                    empty_message: "No reasoning analyses are visible to this identity.",
                    resources: &[
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Analyses",
                            uri: uris::ANALYSES_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Pipelines",
                            uri: uris::PIPELINES_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Models",
                            uri: uris::MODELS_URI,
                        },
                    ],
                    tools: &[veoveo_mcp_apps_extension::WorkbenchTool {
                        label: "Analyze recording",
                        name: "analyze_recording",
                        arguments_json: "{}",
                    }],
                    stream_result: None,
                },
            );
            Ok(ReadResourceResult::new(vec![
                veoveo_mcp_apps_extension::app_html_contents(uri, &html),
            ]))
        }
        ReasonResource::Pipelines => json_resource(uri, &state.catalog.pipeline_views()),
        ReasonResource::Models => json_resource(uri, &state.catalog.model_views()),
        ReasonResource::Pipeline(address) => {
            let pipeline = state
                .catalog
                .pipeline(address.id())
                .ok_or_else(|| McpError::resource_not_found("pipeline not found", None))?;
            json_resource(uri, &pipeline_view(pipeline))
        }
        ReasonResource::Model(address) => {
            let model = state
                .catalog
                .model(address.id())
                .ok_or_else(|| McpError::resource_not_found("model not found", None))?;
            json_resource(uri, &model_view(model))
        }
        ReasonResource::Analyses(cursor) => {
            let page =
                index::analyses_page(&state.tasks, &runtime_owner(&identity), cursor.as_ref())
                    .await?;
            json_resource(uri, &page)
        }
        ReasonResource::Analysis(address) => {
            let snapshot =
                analysis_snapshot(&state.tasks, &runtime_owner(&identity), *address.id()).await?;
            json_resource(uri, &analysis_view(&snapshot)?)
        }
        ReasonResource::Results(address) => {
            let snapshot =
                analysis_snapshot(&state.tasks, &runtime_owner(&identity), *address.id()).await?;
            let view = analysis_view(&snapshot)?;
            let output = view.output().ok_or_else(|| {
                McpError::resource_not_found("analysis results are not available", None)
            })?;
            let caller = plane_caller(context)?;
            let artifact =
                inline_artifact(state, &caller, &output.results_artifact.artifact_id()).await?;
            let text = String::from_utf8(artifact.bytes)
                .map_err(|_| McpError::internal_error("results artifact is not UTF-8", None))?;
            Ok(ReadResourceResult::new(vec![
                ResourceContents::text(text, uri)
                    .with_mime_type("application/vnd.veoveo.reason-results+json"),
            ]))
        }
        ReasonResource::Artifact(id) => {
            let caller = plane_caller(context)?;
            let artifact = inline_artifact(state, &caller, &id).await?;
            let mut content = ResourceContents::blob(BASE64_STANDARD.encode(artifact.bytes), uri);
            if let Some(mime_type) = artifact.metadata.mime_type {
                content = content.with_mime_type(mime_type);
            }
            Ok(ReadResourceResult::new(vec![content]))
        }
    }
}

pub(super) async fn analysis_snapshot(
    tasks: &TaskRuntime,
    owner: &TaskOwner,
    analysis_id: AnalysisId,
) -> Result<TaskSnapshot, McpError> {
    let snapshot = tasks
        .for_owner(owner)
        .of_type(ReasonTaskKind::AnalyzeRecording.name())
        .get(analysis_id.task_id())
        .await
        .map_err(internal)?
        .ok_or_else(|| McpError::resource_not_found("analysis not found", None))?;
    Ok(snapshot)
}

pub(super) fn subscribable_analysis_id(uri: &str) -> Result<AnalysisId, McpError> {
    AnalysisResource::parse(uri)
        .map(|resource| resource.analysis_id())
        .map_err(invalid_params)
}

pub(super) fn analysis_view(snapshot: &TaskSnapshot) -> Result<AnalysisView, McpError> {
    let request: tasks::DurableReasonRequest =
        serde_json::from_value(snapshot.request.clone()).map_err(internal)?;
    let ReasonTaskInput::Analyze(input) = request.input;
    AnalysisView::new(
        AnalysisId::try_from(snapshot.task_id).map_err(internal)?,
        input.pipeline_id,
        AnalysisDetails {
            status: task_status(snapshot.status).to_owned(),
            progress: snapshot.progress,
            task_kind: input.task.kind().to_owned(),
            recording_uri: input.video.recording_uri.clone(),
            entity_path: input.video.entity_path.clone(),
            timeline: input.video.timeline.clone(),
            created_at: snapshot.created_at.to_rfc3339(),
            updated_at: snapshot.updated_at.to_rfc3339(),
        },
    )
    .with_error(snapshot.error.as_ref().map(|error| error.message.clone()))
    .with_output(analysis_output(snapshot.result.as_ref())?)
    .map_err(|_| retained_output_error())
}

fn analysis_output(
    stored: Option<&serde_json::Value>,
) -> Result<Option<AnalyzeRecordingOutput>, McpError> {
    let Some(stored) = stored else {
        return Ok(None);
    };
    let result: CallToolResult =
        serde_json::from_value(stored.clone()).map_err(|_| retained_output_error())?;
    veoveo_reason_mcp::task_product::validate(&result).map_err(|_| retained_output_error())
}

fn retained_output_error() -> McpError {
    McpError::internal_error(
        "stored Reason output does not satisfy the current contract",
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

fn json_resource<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResult, McpError> {
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(serde_json::to_string(value).map_err(internal)?, uri)
            .with_mime_type("application/json"),
    ]))
}

async fn inline_artifact(
    state: &AppState,
    caller: &veoveo_mcp_contract::PlaneCaller,
    artifact_id: &veoveo_artifact_contract::ArtifactId,
) -> Result<veoveo_artifact_contract::ArtifactObject, McpError> {
    let metadata = state
        .artifacts
        .head(caller, artifact_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| {
            McpError::resource_not_found(format!("Artifact `{artifact_id}` was not found."), None)
        })?;
    if metadata.byte_len > state.max_inline_resource_bytes {
        return Err(McpError::invalid_request(
            format!(
                "Artifact `{artifact_id}` is {} bytes, over the {}-byte limit for inline MCP resources. Download it through the artifact download route instead.",
                metadata.byte_len, state.max_inline_resource_bytes
            ),
            None,
        ));
    }
    let artifact = state
        .artifacts
        .get(caller, artifact_id)
        .await
        .map_err(internal)?
        .ok_or_else(|| {
            McpError::resource_not_found(format!("Artifact `{artifact_id}` was not found."), None)
        })?;
    if artifact.bytes.len() as u64 != metadata.byte_len
        || artifact.bytes.len() as u64 > state.max_inline_resource_bytes
    {
        return Err(McpError::internal_error(
            "artifact byte length changed while reading inline resource",
            None,
        ));
    }
    Ok(artifact)
}

/// Discovery is immutable; only Tasks and concrete analysis contents have sources.
pub(super) fn accepted_subscription_filter(
    requested: &rmcp::model::SubscriptionFilter,
) -> Option<rmcp::model::SubscriptionFilter> {
    let mut accepted = rmcp::model::SubscriptionFilter::builder().build();
    accepted.task_ids = requested.task_ids.clone().filter(|ids| !ids.is_empty());
    accepted.resource_subscriptions = requested
        .resource_subscriptions
        .as_ref()
        .map(|uris| {
            uris.iter()
                .filter(|uri| {
                    subscribable_analysis_id(uri).is_ok()
                        || super::subscriptions::finding(uri).is_some()
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .filter(|uris| !uris.is_empty());
    if accepted.task_ids.is_none() && accepted.resource_subscriptions.is_none() {
        None
    } else {
        Some(accepted)
    }
}

#[cfg(test)]
mod retained_output_tests {
    use super::*;
    use rmcp::model::ContentBlock;
    use serde_json::json;

    #[test]
    fn absent_results_and_explicit_tool_errors_have_no_product() {
        assert!(analysis_output(None).unwrap().is_none());
        let error = CallToolResult::error(vec![ContentBlock::text("analysis failed")]);
        assert!(
            analysis_output(Some(&serde_json::to_value(error).unwrap()))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn malformed_retained_success_is_an_error_instead_of_an_absent_product() {
        for value in [
            json!({"content": "sensitive invalid payload"}),
            json!({"content": []}),
            json!({"content": [], "structuredContent": {"analysis_uri": "private payload"}}),
        ] {
            let error = analysis_output(Some(&value)).unwrap_err();
            assert_eq!(error, retained_output_error());
            assert!(error.data.is_none());
        }
    }
}
