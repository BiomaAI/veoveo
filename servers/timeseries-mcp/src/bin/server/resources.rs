//! Caller-authorized resource payloads selected by the owning contract.
use super::{TimeseriesMcp, outputs::usage_record, ownership::runtime_owner};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{ReadResourceResult, ResourceContents},
    service::RequestContext,
};
use veoveo_mcp_contract::{
    UsageReport,
    hosting::{gateway_identity, json_read, plane_caller, served_by_host},
};
use veoveo_timeseries_mcp::{
    contract::TimeseriesResource, forecast::RRD_MIME_TYPE, usage::TimeseriesUsage,
};
impl TimeseriesMcp {
    /// Reads one admitted address. The host serves documents and the contract.
    pub(super) async fn read_timeseries_resource(
        &self,
        resource: TimeseriesResource,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = gateway_identity(context)?;
        match resource {
            TimeseriesResource::Docs
            | TimeseriesResource::Document(_)
            | TimeseriesResource::Contract => Err(served_by_host()),
            TimeseriesResource::ForecastApp => Ok(ReadResourceResult::new(vec![
                veoveo_mcp_apps_extension::app_html_contents(
                    uri,
                    include_str!("../../../assets/forecast-app.html"),
                ),
            ])),
            TimeseriesResource::Usage(index) => {
                let page = TimeseriesUsage::new(&self.state.tasks)
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?
                    .page(&runtime_owner(&identity), index.cursor())
                    .await
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?;
                json_read(uri, &page)
            }
            TimeseriesResource::TaskUsage(usage_uri) => {
                let task_id = usage_uri.task_id();
                let records = TimeseriesUsage::new(&self.state.tasks)
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?
                    .task(&runtime_owner(&identity), &usage_uri)
                    .await
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?
                    .into_iter()
                    .map(|record| usage_record(task_id, record))
                    .collect::<Vec<_>>();
                if records.is_empty() {
                    return Err(McpError::resource_not_found(
                        format!("unknown usage task '{task_id}'"),
                        None,
                    ));
                }
                json_read(
                    uri,
                    &UsageReport::new(task_id.to_string(), uri).with_records(records),
                )
            }
            TimeseriesResource::Artifact(artifact_uri) => {
                let artifact_id = artifact_uri.artifact_id();
                // The plane enforces access with the caller's identity.
                let artifact = self
                    .state
                    .artifacts
                    .get(&plane_caller(context)?, &artifact_id)
                    .await
                    .map_err(|err| McpError::internal_error(err.to_string(), None))?
                    .ok_or_else(|| {
                        McpError::resource_not_found(
                            format!("unknown artifact '{artifact_id}'"),
                            None,
                        )
                    })?;
                let content = ResourceContents::blob(BASE64_STANDARD.encode(&artifact.bytes), uri)
                    .with_mime_type(
                        artifact
                            .metadata
                            .mime_type
                            .unwrap_or_else(|| RRD_MIME_TYPE.to_string()),
                    );
                Ok(ReadResourceResult::new(vec![content]))
            }
        }
    }
}
