use std::sync::{Arc, LazyLock};

use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, CancelTaskParams,
        CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock,
        GetPromptRequestParams, GetTaskParams, GetTaskResult, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        Prompt, ReadResourceRequestParams, ReadResourceResult, Reference, Resource,
        ResourceContents, ResourceTemplate, ServerCapabilities, ServerInfo, SubscriptionFilter,
        UpdateTaskParams,
    },
    service::{RequestContext, SubscriptionContext},
    tool_handler, tool_router,
};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;
use veoveo_mcp_contract::{GatewayInternalIdentity, Page, docs::ServerDocs, paginate};

use crate::{
    clock::assess_clock,
    contract::{
        AssessClockRequest, CancelTemporalEventRequest, ClockAssessment, ClockQualityPolicy,
        ConvertTimeOutput, ConvertTimeRequest, CreateTemporalEventRequest, EvaluateWindowsOutput,
        EvaluateWindowsRequest, ExpandScheduleOutput, ExpandScheduleRequest, ResolveTimeOutput,
        ResolveTimeRequest, TemporalEvent, TemporalEventId, TemporalEventState,
        ValidateTimelineOutput, ValidateTimelineRequest,
    },
    prompts::TimePrompt,
    server::tasks::TimeTaskExtension,
    state::TimeApplication,
    uris,
};

const LIST_PAGE_SIZE: usize = 100;

/// The crate documents embedded at build time and served under the well-known
/// surface: `time://docs`, `time://docs/{doc_id}`, `time://contract`, and the
/// administrative `admin/docs` routes (contract C18-C21).
pub(crate) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("time"));

#[derive(Clone)]
pub struct TimeMcp {
    state: Arc<TimeApplication>,
    task_service: TimeTaskExtension,
    #[allow(dead_code)]
    tool_router: ToolRouter<TimeMcp>,
}

#[tool_router]
impl TimeMcp {
    pub fn new(state: Arc<TimeApplication>) -> Self {
        Self {
            task_service: TimeTaskExtension::new(state.clone()),
            state,
            tool_router: Self::tool_router(),
        }
    }

    /// The capability inventory declared at `time://contract` (contract C19).
    ///
    #[tool(
        title = "Resolve operational time",
        description = "Resolve RFC 3339/9557, civil, military DTG, Unix, TAI, GPS, Julian TAI, or mission-relative time against the active versioned authority releases.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ResolveTimeOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn resolve_time(
        &self,
        Parameters(request): Parameters<ResolveTimeRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "time:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .engine(&scope)
            .await
            .map_err(internal)?
            .resolve(&request)
            .map_err(invalid_params)?;
        structured_result(format!("resolved {}", output.utc_rfc3339), &output)
    }

    #[tool(
        title = "Convert operational time",
        description = "Project one authority-bound TimeInstant into UTC, selected IANA zones, TAI, TT, TDB, GPST, and GST representations.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ConvertTimeOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn convert_time(
        &self,
        Parameters(request): Parameters<ConvertTimeRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "time:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .engine(&scope)
            .await
            .map_err(internal)?
            .convert(&request)
            .map_err(invalid_params)?;
        structured_result("converted authority-bound time".to_owned(), &output)
    }

    #[tool(
        title = "Evaluate time windows",
        description = "Calculate union, intersection, or difference for half-open authority-bound time windows.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<EvaluateWindowsOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn evaluate_windows(
        &self,
        Parameters(request): Parameters<EvaluateWindowsRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "time:schedule")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .engine(&scope)
            .await
            .map_err(internal)?
            .evaluate_windows(&request)
            .map_err(invalid_params)?;
        structured_result(
            format!("calculated {} window(s)", output.windows.len()),
            &output,
        )
    }

    #[tool(
        title = "Assess clock quality",
        description = "Assess the measured host clock offset, error bound, stratum, diversity, holdover, and traceability against an explicit or tenant policy.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ClockAssessment>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn assess_clock(
        &self,
        Parameters(request): Parameters<AssessClockRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "time:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let policy = match request.policy {
            Some(policy) => policy,
            None => self
                .state
                .catalog
                .clock_policy(&scope)
                .await
                .map_err(internal)?
                .map(|value| value.0)
                .unwrap_or_else(default_clock_policy),
        };
        let output = assess_clock(self.state.clock.quality().await.map_err(internal)?, policy);
        structured_result(format!("clock acceptable: {}", output.acceptable), &output)
    }

    #[tool(
        title = "Expand operational calendar",
        description = "Expand a versioned civil-time operational calendar into authority-bound half-open windows. This bulk operation requires Task API invocation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ExpandScheduleOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn expand_schedule(
        &self,
        Parameters(_request): Parameters<ExpandScheduleRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "expand_schedule requires task-based invocation",
            None,
        ))
    }

    #[tool(
        title = "Validate mission timeline",
        description = "Resolve named temporal points and validate precedence plus minimum and maximum separation constraints. This bulk operation requires Task API invocation.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ValidateTimelineOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn validate_timeline(
        &self,
        Parameters(_request): Parameters<ValidateTimelineRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "validate_timeline requires task-based invocation",
            None,
        ))
    }

    #[tool(
        title = "Create temporal event",
        description = "Create an owner-scoped event at an authority-bound instant and emit resource updates when it becomes due.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TemporalEvent>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn create_temporal_event(
        &self,
        Parameters(request): Parameters<CreateTemporalEventRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "time:event:write")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let engine = self.state.engine(&scope).await.map_err(internal)?;
        engine
            .convert(&ConvertTimeRequest {
                instant: request.due.clone(),
                zone_ids: Vec::new(),
                scales: Vec::new(),
            })
            .map_err(invalid_params)?;
        if request.name.trim().is_empty()
            || request.name.len() > 256
            || request.idempotency_key.trim().is_empty()
        {
            return Err(invalid_params(
                "event name and idempotency key must be non-empty",
            ));
        }
        let event = TemporalEvent {
            event_id: TemporalEventId::new(format!("event-{}", Uuid::now_v7()))
                .map_err(invalid_params)?,
            name: request.name,
            due: request.due,
            state: TemporalEventState::Scheduled,
            record_version: 1,
        };
        let event = self
            .state
            .catalog
            .create_event(&scope, event, request.idempotency_key)
            .await
            .map_err(internal)?;
        self.state
            .cancel_event_watcher(&scope, &event.event_id)
            .await;
        self.state
            .schedule_event(scope, event.clone())
            .await
            .map_err(internal)?;
        self.state
            .subscriptions
            .notify_resource_updated(uris::EVENTS_URI)
            .await;
        self.state
            .subscriptions
            .notify_resource_list_changed()
            .await;
        structured_result(format!("scheduled {}", event.event_id), &event)
    }

    #[tool(
        title = "Cancel temporal event",
        description = "Cancel an owner-scoped temporal event under optimistic concurrency.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TemporalEvent>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn cancel_temporal_event(
        &self,
        Parameters(request): Parameters<CancelTemporalEventRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, "time:event:write")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let event = self
            .state
            .catalog
            .cancel_event(&scope, &request.event_id, request.expected_record_version)
            .await
            .map_err(internal)?;
        self.state
            .cancel_event_watcher(&scope, &event.event_id)
            .await;
        self.state
            .subscriptions
            .notify_resource_updated(uris::EVENTS_URI)
            .await;
        self.state
            .subscriptions
            .notify_resource_updated(uris::event_uri(event.event_id.as_str()))
            .await;
        structured_result(format!("cancelled {}", event.event_id), &event)
    }
}

#[tool_handler]
impl ServerHandler for TimeMcp {
    fn supported_protocol_versions(
        &self,
    ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
        veoveo_mcp_contract::final_protocol_versions()
    }

    fn get_info(&self) -> ServerInfo {
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        veoveo_mcp_apps_extension::extend_capabilities(&mut capabilities);
        capabilities.extensions.get_or_insert_default().insert(
            rmcp::model::TASKS_EXTENSION_ID.to_owned(),
            rmcp::model::JsonObject::new(),
        );
        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = rmcp::model::Implementation::new("time", env!("CARGO_PKG_VERSION"));
        info.instructions = Some("Authoritative time interpretation and operational scheduling for agents. Resolve civil, military, GNSS, Unix, TAI, and mission-relative expressions against versioned TZDB and leap-second releases. Invoke schedule expansion and timeline validation through the Task API. Carry TimeInstant authority bindings and uncertainty into Map and Optimization calls.".to_owned());
        info
    }

    async fn call_tool(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        if let Some(created) =
            veoveo_task_runtime::start_durable_tool_task(&self.task_service, &mut request, &context)
                .await?
        {
            return Ok(created.into());
        }
        let call = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(call).await
    }

    async fn get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let caller =
            veoveo_task_runtime::DurableTaskService::authenticate(&self.task_service, &context)?;
        veoveo_task_runtime::DurableTaskService::get_task(&self.task_service, &caller, request)
            .await
    }

    async fn update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller =
            veoveo_task_runtime::DurableTaskService::authenticate(&self.task_service, &context)?;
        veoveo_task_runtime::DurableTaskService::update_task(&self.task_service, &caller, request)
            .await
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller =
            veoveo_task_runtime::DurableTaskService::authenticate(&self.task_service, &context)?;
        veoveo_task_runtime::DurableTaskService::cancel_task(
            &self.task_service,
            &caller,
            request.task_id,
        )
        .await
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = self.tool_router.list_all();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        tools = tools
            .into_iter()
            .map(|tool| {
                veoveo_mcp_apps_extension::link_tool_to_app(
                    tool,
                    uris::TIMELINE_APP_URI,
                    &[
                        veoveo_mcp_apps_extension::UiVisibility::Model,
                        veoveo_mcp_apps_extension::UiVisibility::App,
                    ],
                )
            })
            .collect();
        let page = mcp_page(tools, request.as_ref())?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let identity = require_scope(&context, "time:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let mut resources = root_resources();
        resources.push(
            veoveo_mcp_apps_extension::app_resource(uris::TIMELINE_APP_URI, "timeline")
                .with_title("Timeline")
                .with_description(
                    "Authoritative time, operational calendars, epochs, and temporal events.",
                ),
        );
        for calendar in self
            .state
            .catalog
            .list_calendars(&scope)
            .await
            .map_err(internal)?
        {
            resources.push(descriptor(
                uris::calendar_uri(calendar.calendar_id.as_str(), calendar.version),
                calendar.name,
                "Versioned operational calendar.",
            ));
        }
        for epoch in self
            .state
            .catalog
            .list_epochs(&scope)
            .await
            .map_err(internal)?
        {
            resources.push(descriptor(
                uris::epoch_uri(epoch.epoch_id.as_str()),
                epoch.name,
                "Versioned mission epoch.",
            ));
        }
        for event in self
            .state
            .catalog
            .list_events(&scope)
            .await
            .map_err(internal)?
        {
            self.state
                .schedule_event(scope.clone(), event.clone())
                .await
                .map_err(internal)?;
            resources.push(descriptor(
                uris::event_uri(event.event_id.as_str()),
                event.name,
                "Owner-scoped temporal event.",
            ));
        }
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
        let page = mcp_page(resources, request.as_ref())?;
        Ok(ListResourcesResult {
            resources: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let page = mcp_page(resource_templates(), request.as_ref())?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        let cacheable = request.request_state.is_none() && request.input_responses.is_none();
        async {
        let identity = require_scope(&context, "time:read")?;
        let uri = request.uri.as_str();
        // Well-known surface (contract C18, C19): readable by any identity
        // that can list resources.
        if uri == uris::DOCS_URI {
            return json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>());
        }
        if let Some(doc_id) = uris::parse_doc(uri) {
            let doc = SERVER_DOCS
                .doc(doc_id)
                .ok_or_else(|| not_found("server document"))?;
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
            ]));
        }
        if uri == uris::CONTRACT_URI {
            return json_resource(uri, SERVER_DOCS.contract_declaration());
        }
        if uri == uris::TIMELINE_APP_URI {
            let html = veoveo_mcp_apps_extension::workbench_app_html(
                &veoveo_mcp_apps_extension::WorkbenchApp {
                    app_id: "time-timeline",
                    title: "Timeline",
                    subtitle: "Resolve authoritative time and manage operational temporal state",
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
                },
            );
            return Ok(ReadResourceResult::new(vec![
                veoveo_mcp_apps_extension::app_html_contents(uri, &html),
            ]));
        }
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let engine = self.state.engine(&scope).await.map_err(internal)?;
        match uri {
            uris::CLOCK_QUALITY_URI => {
                return json_resource(uri, &self.state.clock.quality().await.map_err(internal)?);
            }
            uris::CLOCK_CURRENT_URI => {
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
                return json_resource(
                    uri,
                    &json!({"time": time, "effective_policy": policy, "clock_quality": quality}),
                );
            }
            uris::AUTHORITIES_CURRENT_URI => {
                return json_resource(uri, &engine.authority().effective);
            }
            uris::CALENDARS_URI => {
                return json_resource(
                    uri,
                    &self
                        .state
                        .catalog
                        .list_calendars(&scope)
                        .await
                        .map_err(internal)?,
                );
            }
            uris::EPOCHS_URI => {
                return json_resource(
                    uri,
                    &self
                        .state
                        .catalog
                        .list_epochs(&scope)
                        .await
                        .map_err(internal)?,
                );
            }
            uris::EVENTS_URI => {
                return json_resource(
                    uri,
                    &self
                        .state
                        .catalog
                        .list_events(&scope)
                        .await
                        .map_err(internal)?,
                );
            }
            _ => {}
        }
        if let Some(release_id) = uris::parse_authority_release(uri) {
            let release_id =
                crate::contract::AuthorityReleaseId::new(release_id).map_err(invalid_params)?;
            let effective = &engine.authority().effective;
            let reference = if effective.tzdb.release_id == release_id {
                effective.tzdb.clone()
            } else if effective.leap_seconds.release_id == release_id {
                effective.leap_seconds.clone()
            } else {
                let release = self
                    .state
                    .catalog
                    .release(&scope, &release_id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("authority release"))?;
                self.state
                    .catalog
                    .authority_reference(&scope, &release)
                    .await
                    .map_err(internal)?
            };
            return json_resource(uri, &reference);
        }
        if let Some(zone_id) = uris::parse_zone(uri) {
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
                    instant: now.instant,
                    zone_ids: vec![zone_id.to_owned()],
                    scales: Vec::new(),
                })
                .map_err(invalid_params)?;
            return json_resource(
                uri,
                &json!({"zone_id": zone_id, "tzdb_release_id": engine.authority().binding.tzdb_release_id, "current": projection.zoned.into_iter().next()}),
            );
        }
        if let Some((calendar_id, version)) = uris::parse_calendar(uri) {
            let id = crate::contract::CalendarId::new(calendar_id).map_err(invalid_params)?;
            return json_resource(
                uri,
                &self
                    .state
                    .catalog
                    .calendar(&scope, &id, version)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("calendar version"))?,
            );
        }
        if let Some(epoch_id) = uris::parse_epoch(uri) {
            let id = crate::contract::MissionEpochId::new(epoch_id).map_err(invalid_params)?;
            return json_resource(
                uri,
                &self
                    .state
                    .catalog
                    .epoch(&scope, &id)
                    .await
                    .map_err(internal)?
                    .ok_or_else(|| not_found("mission epoch"))?,
            );
        }
        if let Some(event_id) = uris::parse_event(uri) {
            let id = TemporalEventId::new(event_id).map_err(invalid_params)?;
            let event = self
                .state
                .catalog
                .event(&scope, &id)
                .await
                .map_err(internal)?
                .ok_or_else(|| not_found("temporal event"))?;
            self.state
                .schedule_event(scope, event.clone())
                .await
                .map_err(internal)?;
            return json_resource(uri, &event);
        }
        Err(McpError::resource_not_found(
            format!("unknown Time resource `{uri}`"),
            None,
        ))
        }
        .await
        .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let prompts: Vec<Prompt> = TimePrompt::ALL
            .into_iter()
            .map(TimePrompt::definition)
            .collect();
        let page = mcp_page(prompts, request.as_ref())?;
        Ok(ListPromptsResult {
            prompts: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<rmcp::model::GetPromptResponse, McpError> {
        async {
            TimePrompt::by_name(&request.name)
                .ok_or_else(|| McpError::invalid_params("unknown Time prompt", None))?
                .render(request.arguments)
        }
        .await
        .map(Into::into)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let identity = require_scope(&context, "time:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let engine = self.state.engine(&scope).await.map_err(internal)?;
        let values = match (reference.uri.as_str(), request.argument.name.as_str()) {
            (uris::DOC_TEMPLATE, "doc_id") => {
                SERVER_DOCS.iter().map(|doc| doc.id.to_owned()).collect()
            }
            (uris::ZONE_TEMPLATE, "zone_id") => engine
                .authority()
                .tzdb
                .available()
                .map(|name| name.to_string())
                .collect(),
            (uris::CALENDAR_TEMPLATE, "calendar_id") => self
                .state
                .catalog
                .list_calendars(&scope)
                .await
                .map_err(internal)?
                .into_iter()
                .map(|value| value.calendar_id.to_string())
                .collect(),
            (uris::CALENDAR_TEMPLATE, "version") => self
                .state
                .catalog
                .list_calendars(&scope)
                .await
                .map_err(internal)?
                .into_iter()
                .map(|value| value.version.to_string())
                .collect(),
            (uris::EPOCH_TEMPLATE, "epoch_id") => self
                .state
                .catalog
                .list_epochs(&scope)
                .await
                .map_err(internal)?
                .into_iter()
                .map(|value| value.epoch_id.to_string())
                .collect(),
            (uris::EVENT_TEMPLATE, "event_id") => self
                .state
                .catalog
                .list_events(&scope)
                .await
                .map_err(internal)?
                .into_iter()
                .map(|value| value.event_id.to_string())
                .collect(),
            _ => Vec::new(),
        };
        let needle = request.argument.value.to_ascii_lowercase();
        let mut matching: Vec<String> = values
            .into_iter()
            .filter(|value| value.to_ascii_lowercase().contains(&needle))
            .collect();
        matching.sort();
        matching.dedup();
        let total = matching.len();
        matching.truncate(CompletionInfo::MAX_VALUES);
        Ok(CompleteResult::new(
            CompletionInfo::with_pagination(
                matching,
                Some(total as u32),
                total > CompletionInfo::MAX_VALUES,
            )
            .map_err(internal)?,
        ))
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        veoveo_mcp_contract::accepted_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        let request_context = context.request_context().clone();
        let identity = require_scope(&request_context, "time:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        for uri in context.accepted().resource_subscriptions.iter().flatten() {
            if !is_subscribable(uri) {
                return Err(McpError::invalid_params(
                    "resource is immutable or not subscribable",
                    None,
                ));
            }
            if uri == uris::EVENTS_URI {
                for event in self
                    .state
                    .catalog
                    .list_events(&scope)
                    .await
                    .map_err(internal)?
                {
                    self.state
                        .schedule_event(scope.clone(), event)
                        .await
                        .map_err(internal)?;
                }
            } else if let Some(event_id) = uris::parse_event(uri) {
                let event_id = TemporalEventId::new(event_id).map_err(invalid_params)?;
                if let Some(event) = self
                    .state
                    .catalog
                    .event(&scope, &event_id)
                    .await
                    .map_err(internal)?
                {
                    self.state
                        .schedule_event(scope.clone(), event)
                        .await
                        .map_err(internal)?;
                }
            }
        }
        veoveo_task_runtime::listen_durable_subscriptions(
            &self.task_service,
            context,
            Some(self.state.subscriptions.as_ref()),
            None,
        )
        .await
    }
}

fn internal_identity(
    context: &RequestContext<RoleServer>,
) -> Result<GatewayInternalIdentity, McpError> {
    context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<GatewayInternalIdentity>())
        .cloned()
        .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))
}
fn require_scope(
    context: &RequestContext<RoleServer>,
    required: &str,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    if !identity
        .actor
        .scopes
        .iter()
        .any(|scope| scope.as_str() == required)
    {
        return Err(McpError::invalid_request(
            format!("scope `{required}` is required"),
            None,
        ));
    }
    Ok(identity)
}
fn structured_result<T: Serialize>(text: String, value: &T) -> Result<CallToolResult, McpError> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(serde_json::to_value(value).map_err(internal)?);
    Ok(result)
}
fn json_resource<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResult, McpError> {
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(serde_json::to_string(value).map_err(internal)?, uri)
            .with_mime_type("application/json"),
    ]))
}
fn mcp_page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    paginate(items, request, LIST_PAGE_SIZE).map_err(invalid_params)
}
fn invalid_params(error: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}
fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}
fn not_found(kind: &str) -> McpError {
    McpError::resource_not_found(format!("unknown {kind}"), None)
}
fn descriptor(uri: String, title: String, description: &str) -> Resource {
    Resource::new(uri, title.clone())
        .with_title(title)
        .with_description(description)
        .with_mime_type("application/json")
}
fn template(uri: &str, title: &str, description: &str) -> ResourceTemplate {
    ResourceTemplate::new(uri, title)
        .with_title(title)
        .with_description(description)
        .with_mime_type("application/json")
}
fn root_resources() -> Vec<Resource> {
    let mut resources = well_known_resources();
    resources.extend(
        [
            (uris::CLOCK_CURRENT_URI, "Current authoritative time"),
            (uris::CLOCK_QUALITY_URI, "Clock quality"),
            (uris::AUTHORITIES_CURRENT_URI, "Active time authorities"),
            (uris::CALENDARS_URI, "Operational calendars"),
            (uris::EPOCHS_URI, "Mission epochs"),
            (uris::EVENTS_URI, "Temporal events"),
        ]
        .into_iter()
        .map(|(uri, title)| {
            descriptor(
                uri.to_owned(),
                title.to_owned(),
                "Authorized Time domain resource.",
            )
        }),
    );
    resources
}
/// Well-known surface resources (contract C18, C19). `list_resources` serves
/// these for every authorized identity and `stable_resource_uris` declares
/// them in the `time://contract` capability inventory, so the two cannot
/// diverge.
fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![descriptor(
        uris::DOCS_URI.to_owned(),
        "Server documents".to_owned(),
        "Index of the crate documents embedded at build time.",
    )];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(uris::doc_uri(doc.id), doc.title)
                .with_title(doc.title)
                .with_description("Crate document embedded at build time.")
                .with_mime_type("text/markdown"),
        );
    }
    resources.push(descriptor(
        uris::CONTRACT_URI.to_owned(),
        "Contract declaration".to_owned(),
        "Machine-readable contract revision, compliance, and capability inventory.",
    ));
    resources
}
/// Every advertised resource template. `list_resource_templates` serves this
/// list and the `time://contract` capability inventory declares it, so the
/// two cannot diverge.
fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(uris::DOC_TEMPLATE, "Server document")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        template(
            uris::ZONE_TEMPLATE,
            "IANA time zone",
            "Zone interpretation under active TZDB.",
        ),
        template(
            uris::AUTHORITY_RELEASE_TEMPLATE,
            "Time authority release",
            "Immutable compiler-ready authority provenance.",
        ),
        template(
            uris::CALENDAR_TEMPLATE,
            "Operational calendar",
            "Versioned operational calendar.",
        ),
        template(
            uris::EPOCH_TEMPLATE,
            "Mission epoch",
            "Versioned mission epoch.",
        ),
        template(
            uris::EVENT_TEMPLATE,
            "Temporal event",
            "Owner-scoped temporal event.",
        ),
    ]
}
fn is_subscribable(uri: &str) -> bool {
    matches!(
        uri,
        uris::CLOCK_CURRENT_URI
            | uris::CLOCK_QUALITY_URI
            | uris::AUTHORITIES_CURRENT_URI
            | uris::CALENDARS_URI
            | uris::EPOCHS_URI
            | uris::EVENTS_URI
    ) || uris::parse_event(uri).is_some()
        || uris::parse_calendar(uri).is_some()
        || uris::parse_epoch(uri).is_some()
}
fn default_clock_policy() -> ClockQualityPolicy {
    ClockQualityPolicy {
        maximum_error_nanoseconds: 100_000_000,
        maximum_stratum: 4,
        minimum_source_diversity: 2,
        maximum_holdover_seconds: 300,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!TimeMcp::tool_router().list_all().is_empty());
    }
}

#[cfg(test)]
mod well_known_tests {
    use veoveo_mcp_contract::docs::{
        CONTRACT_REVISION, ComplianceStatus, DOC_ID_AGENTS, DOC_ID_DESIGN,
    };

    use super::SERVER_DOCS;

    #[test]
    fn embedded_documents_carry_the_crate_manual_and_design() {
        assert_eq!(SERVER_DOCS.server(), "time");
        let agents = SERVER_DOCS.doc(DOC_ID_AGENTS).expect("agents document");
        assert!(agents.body.contains("## Contract Compliance"));
        let design = SERVER_DOCS.doc(DOC_ID_DESIGN).expect("design document");
        assert!(!design.body.is_empty());
        let index = SERVER_DOCS.llms_txt();
        assert!(index.contains("(agents)"));
        assert!(index.contains("(design)"));
    }

    #[test]
    fn contract_declaration_resolves_from_the_embedded_manual() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        assert_eq!(declaration.server, "time");
        assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance
                .iter()
                .find(|item| item.id == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        let json = serde_json::to_value(&declaration).expect("declaration serializes");
        assert_eq!(json["server"], "time");
    }

    #[test]
    fn contract_declaration_defers_runtime_surface_to_discover() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}
