//! Resource dispatch and caller-scoped analysis reads.
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{CallToolResult, ReadResourceResult, ResourceContents},
    service::RequestContext,
};
use serde::Serialize;
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
            let request: tasks::DurableReasonRequest =
                serde_json::from_value(snapshot.request.clone())
                    .map_err(|_| retained_output_error())?;
            let ReasonTaskInput::Analyze(request) = request.input;
            let text = decode_results(output, &request, &artifact)?;
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
    let output = analysis_output(snapshot.result.as_ref())?;
    if let Some(output) = &output {
        output
            .check_request(&input)
            .map_err(|_| retained_output_error())?;
    }
    AnalysisView::new(
        AnalysisId::try_from(snapshot.task_id).map_err(internal)?,
        input.pipeline_id,
        AnalysisDetails {
            status: snapshot.status,
            progress: snapshot.progress,
            task_kind: veoveo_reason_mcp::contract::ReasoningKind::from(&input.task),
            recording_uri: input.video.recording_uri.clone(),
            entity_path: input.video.entity_path.clone(),
            timeline: input.video.timeline.clone(),
            created_at: snapshot.created_at.to_rfc3339(),
            updated_at: snapshot.updated_at.to_rfc3339(),
        },
    )
    .with_error(snapshot.error.as_ref().map(|error| error.message.clone()))
    .with_output(output)
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
            json!({"content": [], "structuredContent": {"analysisUri": "private payload"}}),
        ] {
            let error = analysis_output(Some(&value)).unwrap_err();
            assert_eq!(error, retained_output_error());
            assert!(error.data.is_none());
        }
    }
}

fn decode_results(
    output: &AnalyzeRecordingOutput,
    request: &veoveo_reason_mcp::contract::AnalyzeRecordingRequest,
    artifact: &veoveo_artifact_contract::ArtifactObject,
) -> Result<String, McpError> {
    let results: veoveo_reason_mcp::contract::ReasoningResults =
        serde_json::from_slice(&artifact.bytes).map_err(|_| retained_output_error())?;
    output
        .check_results(request, &artifact.metadata, &results)
        .map_err(|_| retained_output_error())?;
    String::from_utf8(artifact.bytes.clone()).map_err(|_| retained_output_error())
}

#[cfg(test)]
mod current_result_controls {
    use super::*;

    fn task_request() -> tasks::DurableReasonRequest {
        use veoveo_artifact_contract::{
            ArtifactReadCapabilitySecret, ArtifactWriteCapabilitySecret,
            IssuedArtifactReadCapability, IssuedArtifactWriteCapability,
        };
        use veoveo_reason_mcp::contract::{
            AnalyzeRecordingRequest, IndexRange, RecordingVideoSelection, ReasoningTask,
        };

        let task_id = "01983da0-0000-7000-8000-000000000001".parse().unwrap();
        let expires_at = "2030-01-01T00:00:00Z".parse().unwrap();
        // These inert capabilities are never issued or redeemed by this fixture producer.
        let secret = "inert_fixture_capability_not_issued_0000";
        tasks::DurableReasonRequest {
            input: ReasonTaskInput::Analyze(AnalyzeRecordingRequest {
                video: RecordingVideoSelection::new(
                    "recording://recordings/01983da0-0000-7000-8000-000000000000"
                        .parse()
                        .unwrap(),
                    "/camera/front".into(),
                    "sensor_time".into(),
                    IndexRange::new(10, 20).unwrap(),
                )
                .unwrap(),
                pipeline_id: "traffic-events".parse().unwrap(),
                task: ReasoningTask::DetectEvents {
                    prompt: "Vehicles entering the intersection".into(),
                },
                sampling: Default::default(),
                decode: Default::default(),
                grounding: None,
                include_source_clip: false,
            }),
            grounding: None,
            artifact_write_capability: IssuedArtifactWriteCapability {
                capability_id: "01983da0-0000-7000-8000-000000000004".parse().unwrap(),
                secret: ArtifactWriteCapabilitySecret::new(secret).unwrap(),
                task_id,
                expires_at,
            },
            artifact_read_capability: IssuedArtifactReadCapability {
                capability_id: "01983da0-0000-7000-8000-000000000004".parse().unwrap(),
                secret: ArtifactReadCapabilitySecret::new(secret).unwrap(),
                task_id,
                expires_at,
            },
        }
    }

    #[test]
    fn actual_task_request_producer_matches_current_fixture() {
        let current = serde_json::to_value(task_request()).unwrap();
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/task-request.json");
        if std::env::var_os("UPDATE_REASON_SOURCE_FIXTURES").is_some() {
            std::fs::write(
                &path,
                serde_json::to_string_pretty(&current).unwrap() + "\n",
            )
            .unwrap();
        }
        let captured: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(current, captured);
        let restored: tasks::DurableReasonRequest = serde_json::from_value(captured).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), current);
    }

    #[test]
    fn actual_results_decoder_binds_current_bytes_to_request_and_artifact() {
        let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
        let output: AnalyzeRecordingOutput = serde_json::from_slice(
            &std::fs::read(directory.join("analysis-output-v1.json")).unwrap(),
        )
        .unwrap();
        let request: tasks::DurableReasonRequest =
            serde_json::from_slice(&std::fs::read(directory.join("task-request.json")).unwrap())
                .unwrap();
        let ReasonTaskInput::Analyze(request) = request.input;
        let artifact = veoveo_artifact_contract::ArtifactObject {
            metadata: output.results_artifact.clone(),
            bytes: std::fs::read(directory.join("reason-results-v2.json")).unwrap(),
        };
        assert_eq!(
            decode_results(&output, &request, &artifact)
                .unwrap()
                .as_bytes(),
            artifact.bytes
        );
        for (pointer, value) in [
            ("/schema", serde_json::json!("veoveo.reason-results/v1")),
            ("/pipelineId", serde_json::json!("other-pipeline")),
            ("/modelId", serde_json::json!("other-model")),
            ("/elapsedMs", serde_json::json!(11)),
            ("/requestedRange/end", serde_json::json!(19)),
        ] {
            let mut wire: serde_json::Value = serde_json::from_slice(&artifact.bytes).unwrap();
            *wire.pointer_mut(pointer).unwrap() = value;
            let changed = veoveo_artifact_contract::ArtifactObject {
                metadata: artifact.metadata.clone(),
                bytes: serde_json::to_vec(&wire).unwrap(),
            };
            assert_eq!(
                decode_results(&output, &request, &changed).unwrap_err(),
                retained_output_error()
            );
        }
        for (current, retired) in [
            ("pipelineId", "pipeline_id"),
            ("modelId", "model_id"),
            ("sourceSnapshot", "source_snapshot"),
            ("observedFrames", "observed_frames"),
        ] {
            for mode in ["replacement", "mixed", "conflicting"] {
                let mut wire: serde_json::Value = serde_json::from_slice(&artifact.bytes).unwrap();
                let value = wire[current].clone();
                if mode == "replacement" {
                    wire.as_object_mut().unwrap().remove(current);
                }
                wire[retired] = if mode == "conflicting" {
                    serde_json::Value::Null
                } else {
                    value
                };
                let changed = veoveo_artifact_contract::ArtifactObject {
                    metadata: artifact.metadata.clone(),
                    bytes: serde_json::to_vec(&wire).unwrap(),
                };
                assert_eq!(
                    decode_results(&output, &request, &changed).unwrap_err(),
                    retained_output_error()
                );
            }
        }
        let mut wrong_artifact = artifact.clone();
        wrong_artifact.metadata.metadata["provenance"]["pipelineId"] = "other-pipeline".into();
        assert!(decode_results(&output, &request, &wrong_artifact).is_err());
        let mut wrong_request = request;
        wrong_request.task = veoveo_reason_mcp::contract::ReasoningTask::DetectEvents {
            prompt: "different purpose".into(),
        };
        assert!(decode_results(&output, &wrong_request, &artifact).is_err());
    }
}
