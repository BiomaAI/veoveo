use std::sync::Arc;

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock,
        GetPromptRequestParams, GetPromptResult, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParams, Prompt,
        ReadResourceRequestParams, ReadResourceResult, Reference, Resource, ResourceContents,
        ResourceTemplate, ServerCapabilities, ServerInfo, SubscribeRequestParams,
        UnsubscribeRequestParams,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;
use veoveo_mcp_contract::{GatewayInternalIdentity, Page, paginate};

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
    state::TimeApplication,
    uris,
};

const LIST_PAGE_SIZE: usize = 100;

#[derive(Clone)]
pub struct TimeMcp {
    state: Arc<TimeApplication>,
    #[allow(dead_code)]
    tool_router: ToolRouter<TimeMcp>,
}

#[tool_router]
impl TimeMcp {
    pub fn new(state: Arc<TimeApplication>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

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
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = rmcp::model::Implementation::new("time", env!("CARGO_PKG_VERSION"));
        info.instructions = Some("Authoritative time interpretation and operational scheduling for agents. Resolve civil, military, GNSS, Unix, TAI, and mission-relative expressions against versioned TZDB and leap-second releases. Invoke schedule expansion and timeline validation through the Task API. Carry TimeInstant authority bindings and uncertainty into Map and Optimization calls.".to_owned());
        info
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = self.tool_router.list_all();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let page = mcp_page(tools, request.as_ref())?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
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
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let templates = vec![
            template(
                uris::ZONE_TEMPLATE,
                "IANA time zone",
                "Zone interpretation under active TZDB.",
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
        ];
        let page = mcp_page(templates, request.as_ref())?;
        Ok(ListResourceTemplatesResult {
            resource_templates: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let identity = require_scope(&context, "time:read")?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let engine = self.state.engine(&scope).await.map_err(internal)?;
        let uri = request.uri.as_str();
        match uri {
            uris::CLOCK_QUALITY_URI => {
                return json_resource(uri, &self.state.clock.quality().await.map_err(internal)?);
            }
            uris::CLOCK_CURRENT_URI => {
                let quality = self.state.clock.quality().await.map_err(internal)?;
                let time = engine
                    .resolve(&ResolveTimeRequest {
                        expression: crate::contract::TimeExpression::Rfc3339 {
                            value: chrono::Utc::now().to_rfc3339(),
                        },
                        additional_uncertainty_nanoseconds: quality.error_bound_nanoseconds,
                    })
                    .map_err(invalid_params)?;
                return json_resource(uri, &json!({"time": time, "clock_quality": quality}));
            }
            uris::AUTHORITIES_CURRENT_URI => {
                return json_resource(
                    uri,
                    &json!({"binding": engine.authority().binding, "releases": self.state.catalog.active_releases(&scope).await.map_err(internal)?}),
                );
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
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        TimePrompt::by_name(&request.name)
            .ok_or_else(|| McpError::invalid_params("unknown Time prompt", None))?
            .render(request.arguments)
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

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = require_scope(&context, "time:read")?;
        if !is_subscribable(&request.uri) {
            return Err(McpError::invalid_params(
                "resource is immutable or not subscribable",
                None,
            ));
        }
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        if request.uri == uris::EVENTS_URI {
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
        } else if let Some(event_id) = uris::parse_event(&request.uri) {
            let event_id = TemporalEventId::new(event_id).map_err(invalid_params)?;
            if let Some(event) = self
                .state
                .catalog
                .event(&scope, &event_id)
                .await
                .map_err(internal)?
            {
                self.state
                    .schedule_event(scope, event)
                    .await
                    .map_err(internal)?;
            }
        }
        self.state
            .subscriptions
            .subscribe(request.uri, identity.principal.id, context.peer.clone())
            .await;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = require_scope(&context, "time:read")?;
        if !is_subscribable(&request.uri) {
            return Err(McpError::invalid_params(
                "resource is immutable or not subscribable",
                None,
            ));
        }
        self.state
            .subscriptions
            .unsubscribe(&request.uri, &identity.principal.id)
            .await;
        Ok(())
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
        .principal
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
    })
    .collect()
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
