//! Caller-authorized resource payloads selected by the owning contract.
use super::{
    SERVER_DOCS, TimeseriesMcp,
    outputs::usage_record,
    ownership::{internal_caller, internal_identity, runtime_owner},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{ReadResourceRequestParams, ReadResourceResult, ResourceContents},
    service::RequestContext,
};
use veoveo_mcp_contract::UsageReport;
use veoveo_timeseries_mcp::{
    contract::TimeseriesResource, forecast::RRD_MIME_TYPE, usage::TimeseriesUsage,
};
impl TimeseriesMcp {
    pub(super) async fn read_timeseries_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        let cacheable = request.request_state.is_none() && request.input_responses.is_none();
        async {
            let identity = internal_identity(&context)?;
            let uri = request.uri.as_str();
            let resource = TimeseriesResource::parse(uri)
                .map_err(|_| McpError::resource_not_found("unknown resource", None))?;
            match resource {
                // Well-known surface (contract C18, C19): readable by any identity
                // that can list resources.
                TimeseriesResource::Docs => Ok(ReadResourceResult::new(vec![
                    ResourceContents::text(
                        serde_json::to_string(&SERVER_DOCS.iter().collect::<Vec<_>>())
                            .unwrap_or_default(),
                        uri,
                    )
                    .with_mime_type("application/json"),
                ])),
                TimeseriesResource::Document(doc_id) => {
                    let doc_id = doc_id.as_str();
                    let doc = SERVER_DOCS.doc(doc_id).ok_or_else(|| {
                        McpError::resource_not_found(format!("unknown document '{doc_id}'"), None)
                    })?;
                    Ok(ReadResourceResult::new(vec![
                        ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
                    ]))
                }
                TimeseriesResource::Contract => {
                    let declaration = SERVER_DOCS.contract_declaration();
                    Ok(ReadResourceResult::new(vec![
                        ResourceContents::text(
                            serde_json::to_string(declaration).unwrap_or_default(),
                            uri,
                        )
                        .with_mime_type("application/json"),
                    ]))
                }
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
                    Ok(ReadResourceResult::new(vec![
                        ResourceContents::text(
                            serde_json::to_string(&page).unwrap_or_default(),
                            uri,
                        )
                        .with_mime_type("application/json"),
                    ]))
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
                    let report = UsageReport::new(task_id.to_string(), uri).with_records(records);
                    Ok(ReadResourceResult::new(vec![
                        ResourceContents::text(
                            serde_json::to_string(&report).unwrap_or_default(),
                            uri,
                        )
                        .with_mime_type("application/json"),
                    ]))
                }
                TimeseriesResource::Artifact(artifact_uri) => {
                    let artifact_id = artifact_uri.artifact_id();
                    // The plane enforces access with the caller's identity.
                    let caller = internal_caller(&context)?;
                    let artifact = self
                        .state
                        .artifacts
                        .get(&caller, &artifact_id)
                        .await
                        .map_err(|err| McpError::internal_error(err.to_string(), None))?
                        .ok_or_else(|| {
                            McpError::resource_not_found(
                                format!("unknown artifact '{artifact_id}'"),
                                None,
                            )
                        })?;
                    let blob = BASE64_STANDARD.encode(&artifact.bytes);
                    let mut content = ResourceContents::blob(blob, uri);
                    content = content.with_mime_type(
                        artifact
                            .metadata
                            .mime_type
                            .unwrap_or_else(|| RRD_MIME_TYPE.to_string()),
                    );
                    Ok(ReadResourceResult::new(vec![content]))
                }
            }
        }
        .await
        .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
    }
}
