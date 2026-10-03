use super::{mcp::internal, state::AppState};
use rmcp::{ErrorData as McpError, model::ReadResourceResult};
use veoveo_mcp_contract::{
    GatewayInternalIdentity,
    hosting::{json_read, served_by_host},
};
use veoveo_platform_store::RecordingId;
use veoveo_recording_mcp::{contract::RecordingResource, uris};

pub(super) async fn read(
    state: &AppState,
    identity: &GatewayInternalIdentity,
    uri: &str,
    resource: RecordingResource,
) -> Result<ReadResourceResult, McpError> {
    match resource {
        RecordingResource::Docs | RecordingResource::Document(_) | RecordingResource::Contract => {
            Err(served_by_host())
        }
        RecordingResource::Explorer => {
            let html = veoveo_mcp_apps_extension::workbench_app_html(
                &veoveo_mcp_apps_extension::WorkbenchApp {
                    app_id: "recording-explorer",
                    title: "Explorer",
                    subtitle: "Browse recordings and inspect their timeline data",
                    empty_message: "No recordings are visible to this identity.",
                    resources: &[veoveo_mcp_apps_extension::WorkbenchResource {
                        label: "Recording catalog",
                        uri: uris::CATALOG_URI,
                    }],
                    tools: &[
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Create bounded Arrow projection",
                            name: "create_recording_projection",
                            arguments_json: r#"{"dataset_id":"","recording_id":"","entity_paths":["/sensor"],"component_ids":["Scalars:scalars"],"timeline":"tick","sampling":{"kind":"range","start":0,"end":100},"sparse_fill":"none","maximum_entities":8,"maximum_columns":8,"maximum_samples":1000,"maximum_rows":10000,"maximum_bytes":33554432,"deadline_ms":15000,"idempotency_key":"","units":{},"coordinate_frame_refs":[]}"#,
                        },
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Seal recording",
                            name: "seal_recording",
                            arguments_json: r#"{"recording_id":""}"#,
                        },
                    ],
                    stream_result: Some(
                        veoveo_mcp_apps_extension::WorkbenchStreamResult::RecordingProjection {
                            tool_name: "create_recording_projection",
                        },
                    ),
                },
            );
            Ok(ReadResourceResult::new(vec![
                veoveo_mcp_apps_extension::app_html_contents(uri, &html),
            ]))
        }
        RecordingResource::Catalog(after) => json_read(
            uri,
            &state
                .recordings
                .catalog_page(identity, after.as_ref())
                .await
                .map_err(query_error)?,
        ),
        RecordingResource::Recording(address) => {
            let id = RecordingId::from_uuid(address.id().as_uuid());
            let value = state
                .recordings
                .recording_view(identity, id)
                .await
                .map_err(internal)?
                .ok_or_else(|| McpError::resource_not_found("Recording was not found", None))?;
            json_read(uri, &value)
        }
        RecordingResource::Layers(address) => {
            let id = RecordingId::from_uuid(address.id().as_uuid());
            let value = state
                .recordings
                .layer_views(identity, id)
                .await
                .map_err(internal)?
                .ok_or_else(|| McpError::resource_not_found("Recording was not found", None))?;
            json_read(uri, &value)
        }
    }
}

pub(super) fn query_error(error: anyhow::Error) -> McpError {
    if let Some(veoveo_platform_store::StoreError::InvalidRecordingField { field, reason }) =
        error.downcast_ref::<veoveo_platform_store::StoreError>()
    {
        McpError::invalid_params(format!("invalid Recording {field}: {reason}"), None)
    } else {
        McpError::internal_error(error.to_string(), None)
    }
}
