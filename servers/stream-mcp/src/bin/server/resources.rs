//! Parse each address once and authorize private reads through their owner.
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{ReadResourceResult, ResourceContents},
    service::RequestContext,
};
use veoveo_stream_mcp::contract::StreamTaskKind;
use veoveo_stream_mcp::{
    catalog::{model_view, pipeline_view},
    contract::{RunId, StreamResource},
};
use veoveo_task_runtime::{TaskOwner, TaskRuntime, TaskSnapshot};
use veoveo_types::TaskTypeDefinition;

use veoveo_mcp_contract::hosting::{gateway_identity, json_read, plane_caller, served_by_host};

use super::{app_state::AppState, index, inline_artifact, internal, run_view, runtime_owner};

/// Reads one admitted address. The host serves documents and the contract.
pub(super) async fn read(
    state: &AppState,
    resource: StreamResource,
    uri: &str,
    context: &RequestContext<RoleServer>,
) -> Result<ReadResourceResult, McpError> {
    match resource {
        StreamResource::LiveApp => Ok(ReadResourceResult::new(vec![
            veoveo_mcp_apps_extension::app_html_contents(uri, state.live_app.as_str()),
        ])),
        StreamResource::Docs | StreamResource::Document(_) | StreamResource::Contract => {
            Err(served_by_host())
        }
        StreamResource::Pipelines => json_read(uri, &state.catalog.pipeline_views()),
        StreamResource::Models => json_read(uri, &state.catalog.model_views()),
        StreamResource::Pipeline(address) => {
            let pipeline = state
                .catalog
                .pipeline(address.id())
                .ok_or_else(|| McpError::resource_not_found("Stream pipeline not found", None))?;
            json_read(uri, &pipeline_view(pipeline))
        }
        StreamResource::Model(address) => {
            let model = state
                .catalog
                .model(address.id())
                .ok_or_else(|| McpError::resource_not_found("Stream model not found", None))?;
            json_read(uri, &model_view(model))
        }
        private @ (StreamResource::Runs(_)
        | StreamResource::Sessions(_)
        | StreamResource::Session(_)
        | StreamResource::SessionResults(_)
        | StreamResource::SessionPreview(_)
        | StreamResource::Run(_)
        | StreamResource::RunResults(_)
        | StreamResource::Artifact(_)) => {
            let owner = runtime_owner(&gateway_identity(context)?);
            match private {
                StreamResource::Runs(cursor) => json_read(
                    uri,
                    &index::runs_page(&state.tasks, &owner, cursor.as_ref()).await?,
                ),
                StreamResource::Sessions(cursor) => json_read(
                    uri,
                    &index::sessions_page(&state.live, &owner, cursor.as_ref()).await?,
                ),
                StreamResource::Session(address) => {
                    let view = state
                        .live
                        .view(*address.id(), &owner)
                        .await
                        .ok_or_else(session_missing)?;
                    json_read(uri, &view)
                }
                StreamResource::SessionResults(address) => {
                    let results = state
                        .live
                        .results(*address.id(), &owner)
                        .await
                        .ok_or_else(session_missing)?;
                    json_read(uri, &results)
                }
                StreamResource::SessionPreview(address) => {
                    let preview = state
                        .live
                        .preview(*address.id(), &owner)
                        .await
                        .ok_or_else(session_missing)?;
                    json_read(uri, &preview)
                }
                StreamResource::Run(address) => {
                    let snapshot = run_snapshot(&state.tasks, &owner, *address.id()).await?;
                    json_read(uri, &run_view(&snapshot)?)
                }
                StreamResource::RunResults(address) => {
                    let snapshot = run_snapshot(&state.tasks, &owner, *address.id()).await?;
                    let run = run_view(&snapshot)?;
                    let output = run.output().ok_or_else(|| {
                        McpError::resource_not_found("run results are not available", None)
                    })?;
                    let caller = plane_caller(context)?;
                    let artifact =
                        inline_artifact(state, &caller, &output.results_artifact.artifact_id())
                            .await?;
                    let request: veoveo_stream_mcp::task_request::DurableStreamRequest =
                        serde_json::from_value(snapshot.request.clone())
                            .map_err(|_| replay_error())?;
                    decode_replay(
                        &run,
                        request.input.video(),
                        &artifact.metadata,
                        &artifact.bytes,
                    )?;
                    let text = String::from_utf8(artifact.bytes).map_err(|_| {
                        McpError::internal_error("results artifact is not UTF-8", None)
                    })?;
                    Ok(ReadResourceResult::new(vec![
                        ResourceContents::text(text, uri)
                            .with_mime_type("application/vnd.veoveo.stream-results+json"),
                    ]))
                }
                StreamResource::Artifact(id) => {
                    let caller = plane_caller(context)?;
                    let artifact = inline_artifact(state, &caller, &id).await?;
                    let mut content =
                        ResourceContents::blob(BASE64_STANDARD.encode(artifact.bytes), uri);
                    if let Some(mime_type) = artifact.metadata.mime_type {
                        content = content.with_mime_type(mime_type);
                    }
                    Ok(ReadResourceResult::new(vec![content]))
                }
                _ => unreachable!("public resources handled above"),
            }
        }
    }
}

fn replay_error() -> McpError {
    McpError::internal_error(
        "stored Stream replay does not match its selected run and Artifact",
        None,
    )
}

fn decode_replay(
    run: &veoveo_stream_mcp::contract::RunView,
    selected: &veoveo_stream_mcp::contract::RecordingVideoSelection,
    artifact: &veoveo_artifact_contract::ArtifactMetadata,
    bytes: &[u8],
) -> Result<veoveo_stream_mcp::contract::AnalysisResults, McpError> {
    let results: veoveo_stream_mcp::contract::AnalysisResults =
        serde_json::from_slice(bytes).map_err(|_| replay_error())?;
    run.check_replay(selected, artifact, &results)
        .map_err(|_| replay_error())?;
    Ok(results)
}

fn session_missing() -> McpError {
    McpError::resource_not_found("Stream session not found", None)
}

pub(super) async fn run_snapshot(
    tasks: &TaskRuntime,
    owner: &TaskOwner,
    id: RunId,
) -> Result<TaskSnapshot, McpError> {
    let missing = || McpError::resource_not_found("Stream run not found", None);
    let snapshot = tasks
        .for_owner(owner)
        .of_type(StreamTaskKind::RunRecording.name())
        .get(id.task_id())
        .await
        .map_err(internal)?
        .ok_or_else(missing)?;
    Ok(snapshot)
}

/// Static catalogs never accept list changes; only owned mutable resource families subscribe.
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
                    StreamResource::parse(uri).is_ok_and(|resource| {
                        resource.subscription_run().is_some()
                            || resource.subscription_session().is_some()
                    })
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
#[path = "replay_resource_tests.rs"]
mod replay_resource_tests;
