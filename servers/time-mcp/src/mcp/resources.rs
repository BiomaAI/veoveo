use super::{
    SERVER_DOCS, TimeMcp, default_clock_policy, internal, invalid_params, json_resource, not_found,
    require_scope,
};
use crate::{
    contract::{ConvertTimeRequest, ResolveTimeRequest, TimeResource, TimeScope},
    uris,
};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{ReadResourceRequestParams, ReadResourceResult, ResourceContents},
    service::RequestContext,
};
use serde_json::json;

impl TimeMcp {
    pub(super) async fn read_time_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        let cacheable = request.request_state.is_none() && request.input_responses.is_none();
        self.read_time_contents(&request.uri, &context)
            .await
            .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
    }

    async fn read_time_contents(
        &self,
        uri: &str,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = require_scope(context, TimeScope::Read)?;
        let resource = TimeResource::parse(uri).map_err(|error| match error {
            crate::contract::TimeResourceError::UnknownResource => not_found("Time resource"),
            _ => invalid_params(error),
        })?;
        // Well-known surface (contract C18, C19): readable by any identity
        // that can list resources.
        if resource == TimeResource::Docs {
            return json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>());
        }
        if let TimeResource::Document(doc_id) = &resource {
            let doc = SERVER_DOCS
                .doc(doc_id.as_str())
                .ok_or_else(|| not_found("server document"))?;
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
            ]));
        }
        if resource == TimeResource::Contract {
            return json_resource(uri, SERVER_DOCS.contract_declaration());
        }
        if resource == TimeResource::TimelineApp {
            let html = veoveo_mcp_apps_extension::workbench_app_html(
                &veoveo_mcp_apps_extension::WorkbenchApp {
                    app_id: "time-timeline",
                    title: "Timeline",
                    subtitle: "Resolve times and manage scheduled events",
                    empty_message: "No temporal resources are visible to this identity.",
                    resources: &[
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Current time",
                            uri: uris::CLOCK_CURRENT_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Clock quality",
                            uri: uris::CLOCK_QUALITY_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Calendars",
                            uri: uris::CALENDARS_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Mission epochs",
                            uri: uris::EPOCHS_URI,
                        },
                        veoveo_mcp_apps_extension::WorkbenchResource {
                            label: "Events",
                            uri: uris::EVENTS_URI,
                        },
                    ],
                    tools: &[
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Resolve time",
                            name: "resolve_time",
                            arguments_json: "{}",
                        },
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Convert time",
                            name: "convert_time",
                            arguments_json: "{}",
                        },
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Evaluate windows",
                            name: "evaluate_windows",
                            arguments_json: "{}",
                        },
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Assess clock",
                            name: "assess_clock",
                            arguments_json: "{}",
                        },
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Create event",
                            name: "create_temporal_event",
                            arguments_json: "{}",
                        },
                    ],
                    stream_result: None,
                },
            );
            return Ok(ReadResourceResult::new(vec![
                veoveo_mcp_apps_extension::app_html_contents(uri, &html),
            ]));
        }
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        match &resource {
            TimeResource::Calendars { cursor: after } => json_resource(
                uri,
                &self
                    .state
                    .catalog
                    .calendars_page(&scope, after.as_ref())
                    .await
                    .map_err(crate::index::query_error)?,
            ),
            TimeResource::Epochs { cursor: after } => json_resource(
                uri,
                &self
                    .state
                    .catalog
                    .epochs_page(&scope, after.as_ref())
                    .await
                    .map_err(crate::index::query_error)?,
            ),
            TimeResource::Events { cursor: after } => {
                let page = self
                    .state
                    .catalog
                    .events_page(&scope, after.as_ref(), None)
                    .await
                    .map_err(crate::index::query_error)?;
                self.state
                    .schedule_events(scope.clone(), page.items.clone())
                    .await
                    .map_err(internal)?;
                json_resource(uri, &page)
            }
            TimeResource::AuthorityRelease(release_id) => {
                let reference = if let Some(reference) =
                    self.state.authorities.bootstrap_reference(release_id)
                {
                    reference
                } else {
                    let release = self
                        .state
                        .catalog
                        .release(&scope, release_id)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| not_found("authority release"))?;
                    self.state
                        .catalog
                        .authority_reference(&scope, &release)
                        .await
                        .map_err(internal)?
                };
                json_resource(uri, &reference)
            }
            TimeResource::Zone(zone_id) => {
                let engine = self.state.engine(&scope).await.map_err(internal)?;
                let now = engine
                    .resolve(&ResolveTimeRequest {
                        expression: crate::contract::TimeExpression::Rfc3339 {
                            value: chrono::Utc::now().to_rfc3339(),
                        },
                        additional_uncertainty_nanoseconds: 0,
                    })
                    .map_err(invalid_params)?;
                let projection = engine
                    .convert(&ConvertTimeRequest {
                        instant: now.into_instant(),
                        zone_ids: vec![zone_id.as_str().to_owned()],
                        scales: Vec::new(),
                    })
                    .map_err(invalid_params)?;
                json_resource(
                    uri,
                    &json!({"zone_id": zone_id, "tzdb_release_id": engine.authority().binding().tzdb_release_id(), "current": projection.zoned.into_iter().next()}),
                )
            }
            TimeResource::Calendar { id, version } => json_resource(
                uri,
                &self
                    .state
                    .catalog
                    .calendar(&scope, id, *version)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("calendar version"))?,
            ),
            TimeResource::Epoch(id) => json_resource(
                uri,
                &self
                    .state
                    .catalog
                    .epoch(&scope, id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("mission epoch"))?,
            ),
            TimeResource::Event(id) => {
                let event = self
                    .state
                    .catalog
                    .event(&scope, id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("temporal event"))?;
                self.state
                    .schedule_event(scope, event.clone())
                    .await
                    .map_err(internal)?;
                json_resource(uri, &event)
            }
            TimeResource::ClockQuality => {
                json_resource(uri, &self.state.clock.quality().await.map_err(internal)?)
            }
            TimeResource::ClockCurrent => {
                let engine = self.state.engine(&scope).await.map_err(internal)?;
                let quality = self.state.clock.quality().await.map_err(internal)?;
                let policy = self
                    .state
                    .catalog
                    .clock_policy(&scope)
                    .await
                    .map_err(internal)?
                    .map(|value| value.0)
                    .unwrap_or_else(default_clock_policy);
                let time = engine
                    .resolve(&ResolveTimeRequest {
                        expression: crate::contract::TimeExpression::Rfc3339 {
                            value: chrono::Utc::now().to_rfc3339(),
                        },
                        additional_uncertainty_nanoseconds: quality.error_bound_nanoseconds,
                    })
                    .map_err(invalid_params)?;
                json_resource(
                    uri,
                    &json!({"time": time, "effective_policy": policy, "clock_quality": quality}),
                )
            }
            TimeResource::AuthoritiesCurrent => {
                let engine = self.state.engine(&scope).await.map_err(internal)?;
                json_resource(uri, engine.authority().effective())
            }
            TimeResource::Docs
            | TimeResource::Document(_)
            | TimeResource::Contract
            | TimeResource::TimelineApp => Err(not_found("Time resource")),
        }
    }
}
