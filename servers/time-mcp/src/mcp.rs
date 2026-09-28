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
        Prompt, ReadResourceRequestParams, ReadResourceResult, Reference, ResourceContents,
        ServerConfig, SubscriptionFilter, UpdateTaskParams,
    },
    service::{RequestContext, SubscriptionContext},
    tool_handler, tool_router,
};
use serde::Serialize;
use uuid::Uuid;
use veoveo_mcp_contract::{GatewayInternalIdentity, Page, docs::ServerDocs, paginate};

use crate::{
    clock::assess_clock,
    contract::{
        AssessClockRequest, CancelTemporalEventRequest, ClockAssessment, ClockQualityPolicy,
        ConvertTimeOutput, ConvertTimeRequest, CreateTemporalEventRequest, EvaluateWindowsOutput,
        EvaluateWindowsRequest, ExpandScheduleOutput, ExpandScheduleRequest, ResolveTimeOutput,
        ResolveTimeRequest, TemporalEvent, TemporalEventId, TemporalEventState, TimeScope,
        ValidateTimelineOutput, ValidateTimelineRequest,
    },
    prompts::TimePrompt,
    server::tasks::TimeTaskExtension,
    state::TimeApplication,
    uris,
};

mod resources;
mod setup;
pub(crate) use setup::SERVER_SETUP;

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
        description = "Resolve a time given as RFC 3339/9557, civil time, a military DTG, Unix time, TAI, GPS time, Julian TAI, or mission-relative time, using the active time-authority releases.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ResolveTimeOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn resolve_time(
        &self,
        Parameters(request): Parameters<ResolveTimeRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, TimeScope::Read)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .engine_for_expressions(&scope, [&request.expression])
            .await
            .map_err(internal)?
            .resolve(&request)
            .map_err(invalid_params)?;
        structured_result(format!("resolved {}", output.utc_rfc3339), &output)
    }

    #[tool(
        title = "Convert operational time",
        description = "Convert a resolved TimeInstant to UTC, chosen IANA time zones, TAI, TT, TDB, GPS time (GPST), and Galileo time (GST).",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ConvertTimeOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn convert_time(
        &self,
        Parameters(request): Parameters<ConvertTimeRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, TimeScope::Read)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .engine(&scope)
            .await
            .convert(&request)
            .map_err(invalid_params)?;
        structured_result("converted authority-bound time".to_owned(), &output)
    }

    #[tool(
        title = "Evaluate time windows",
        description = "Compute the union, intersection, or difference of half-open time windows.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<EvaluateWindowsOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn evaluate_windows(
        &self,
        Parameters(request): Parameters<EvaluateWindowsRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, TimeScope::Schedule)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let output = self
            .state
            .engine(&scope)
            .await
            .evaluate_windows(&request)
            .map_err(invalid_params)?;
        structured_result(
            format!("calculated {} window(s)", output.windows.len()),
            &output,
        )
    }

    #[tool(
        title = "Assess clock quality",
        description = "Check the host clock's measured offset, error bound, stratum, source diversity, holdover, and traceability against a policy you pass or the tenant's policy.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ClockAssessment>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn assess_clock(
        &self,
        Parameters(request): Parameters<AssessClockRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, TimeScope::Read)?;
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
        description = "Expand a versioned civil-time operational calendar into half-open time windows. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ExpandScheduleOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn expand_schedule(
        &self,
        Parameters(_request): Parameters<ExpandScheduleRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`expand_schedule` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }

    #[tool(
        title = "Validate mission timeline",
        description = "Resolve named time points and check their ordering and minimum and maximum separation constraints. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ValidateTimelineOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn validate_timeline(
        &self,
        Parameters(_request): Parameters<ValidateTimelineRequest>,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "`validate_timeline` must be called as an MCP Task. Resend the call with task parameters.",
            None,
        ))
    }

    #[tool(
        title = "Create temporal event",
        description = "Create an event at a resolved time instant. Subscribers receive a resource update when it becomes due. Only its owner can change it.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TemporalEvent>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn create_temporal_event(
        &self,
        Parameters(request): Parameters<CreateTemporalEventRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, TimeScope::EventWrite)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let engine = self.state.engine(&scope).await;
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
        description = "Cancel an event you own. Pass the revision you last read; the call fails if it has changed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TemporalEvent>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn cancel_temporal_event(
        &self,
        Parameters(request): Parameters<CancelTemporalEventRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = require_scope(&context, TimeScope::EventWrite)?;
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
            .notify_resource_updated(
                crate::contract::TimeResource::Event(event.event_id.clone()).to_string(),
            )
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

    fn get_info(&self) -> ServerConfig {
        SERVER_SETUP.server_config().clone()
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
        require_scope(&context, TimeScope::Read)?;
        let resources = SERVER_SETUP
            .resources()
            .iter()
            .map(|resource| resource.descriptor().clone())
            .collect();
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
        let page = mcp_page(
            SERVER_SETUP
                .resource_templates()
                .iter()
                .map(|template| template.descriptor().clone())
                .collect(),
            request.as_ref(),
        )?;
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
        self.read_time_resource(request, context).await
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
        use crate::catalog::TimeCompletion;

        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let identity = require_scope(&context, TimeScope::Read)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        let domain = match (
            adapt_zone_completion_v1(&reference.uri),
            request.argument.name.as_str(),
        ) {
            (uris::CALENDAR_TEMPLATE, "calendar_id") => Some(TimeCompletion::CalendarId),
            (uris::CALENDAR_TEMPLATE, "version") => Some(TimeCompletion::CalendarVersion {
                calendar_key: request
                    .context
                    .as_ref()
                    .and_then(|context| context.get_argument("calendar_id"))
                    .map(|key| crate::contract::CalendarId::new(key.clone()))
                    .transpose()
                    .map_err(invalid_params)?,
            }),
            (uris::EPOCH_TEMPLATE, "epoch_id") => Some(TimeCompletion::EpochId),
            (uris::EVENT_TEMPLATE, "event_id") => Some(TimeCompletion::EventId),
            _ => None,
        };
        if let Some(domain) = domain {
            let values = self
                .state
                .catalog
                .complete_values(&scope, domain, &request.argument.value, 101)
                .await
                .map_err(internal)?;
            let has_more = values.len() > CompletionInfo::MAX_VALUES;
            return Ok(CompleteResult::new(
                CompletionInfo::with_pagination(
                    values
                        .iter()
                        .take(CompletionInfo::MAX_VALUES)
                        .cloned()
                        .collect(),
                    (!has_more).then_some(values.len() as u32),
                    has_more,
                )
                .map_err(internal)?,
            ));
        }
        // These catalogs are packaged with the server's documents and TZDB.
        let values: Vec<String> = match (
            adapt_zone_completion_v1(&reference.uri),
            request.argument.name.as_str(),
        ) {
            (uris::DOC_TEMPLATE, "doc_id") => {
                SERVER_DOCS.iter().map(|doc| doc.id.to_owned()).collect()
            }
            (uris::ZONE_TEMPLATE, "zone_id") => self
                .state
                .authorities
                .authority_engine(&scope)
                .await
                .authority()
                .tzdb
                .available()
                .map(|name| name.to_string())
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
        let identity = require_scope(&request_context, TimeScope::Read)?;
        let scope = self.state.scope(&identity).await.map_err(internal)?;
        for uri in context.accepted().resource_subscriptions.iter().flatten() {
            let resource = crate::contract::TimeResource::parse(uri).map_err(invalid_params)?;
            if !resource.is_subscribable() {
                return Err(McpError::invalid_params(
                    "resource is immutable or not subscribable",
                    None,
                ));
            }
            if matches!(
                resource,
                crate::contract::TimeResource::Events { cursor: None }
            ) {
                self.state
                    .restore_event_watchers(&scope)
                    .await
                    .map_err(internal)?;
            } else if let crate::contract::TimeResource::Event(event_id) = resource
                && let Some(event) = self
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
        .ok_or_else(|| {
            McpError::invalid_request(veoveo_mcp_contract::GATEWAY_ROUTING_REQUIRED, None)
        })
}
fn require_scope(
    context: &RequestContext<RoleServer>,
    required: TimeScope,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = internal_identity(context)?;
    crate::server::auth::require_scope(&identity.actor.scopes, required)?;
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
// Public Time completion-template v1 adapter. Its support window and retirement
// gate are declared in Time's DESIGN.md; both spellings reach the same handler.
fn adapt_zone_completion_v1(template: &str) -> &str {
    match template {
        "time://zones/{zone_id}" => uris::ZONE_TEMPLATE,
        _ => template,
    }
}

fn default_clock_policy() -> ClockQualityPolicy {
    ClockQualityPolicy::builder()
        .maximum_error_nanoseconds(100_000_000)
        .maximum_stratum(4)
        .minimum_source_diversity(2)
        .maximum_holdover_seconds(300)
        .build()
        .expect("valid built-in Time clock policy")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_completion_v1_adapter_uses_the_current_reserved_expansion_template() {
        assert_eq!(uris::ZONE_TEMPLATE, "time://zones/{+zone_id}");
        assert_eq!(
            adapt_zone_completion_v1("time://zones/{zone_id}"),
            uris::ZONE_TEMPLATE
        );
        assert_eq!(
            adapt_zone_completion_v1(uris::ZONE_TEMPLATE),
            uris::ZONE_TEMPLATE
        );
        for unrelated in [
            uris::EPOCH_TEMPLATE,
            "other://zones/{zone_id}",
            "time://zones/{other}",
        ] {
            assert_eq!(adapt_zone_completion_v1(unrelated), unrelated);
        }
    }

    #[test]
    fn advertised_resources_and_simple_templates_agree_with_typed_addresses() {
        use crate::contract::TimeResource;
        use veoveo_types::{ResourceAddress, ResourceUri};
        for resource in SERVER_SETUP
            .resources()
            .iter()
            .map(|resource| resource.descriptor())
        {
            let address = TimeResource::parse(&resource.uri).unwrap();
            assert_eq!(address.to_uri().unwrap().as_str(), resource.uri);
        }
        for (template, wire) in [
            (uris::DOC_TEMPLATE, "time://docs/agents"),
            (
                uris::AUTHORITY_RELEASE_TEMPLATE,
                "time://authorities/releases/time-release-one",
            ),
            (
                uris::CALENDAR_TEMPLATE,
                "time://calendars/calendar-one/versions/12",
            ),
            (uris::EPOCH_TEMPLATE, "time://epochs/epoch-one"),
            (uris::EVENT_TEMPLATE, "time://events/event-one"),
        ] {
            let address = TimeResource::parse(wire).unwrap();
            assert!(
                veoveo_mcp_contract::ResourceUriTemplate::new(template)
                    .unwrap()
                    .matches_uri(&address.to_uri().unwrap())
            );
        }
        let conventions = veoveo_mcp_contract::ServerResourceUris::new(
            veoveo_types::ResourceScheme::new("time").expect("declared resource scheme"),
        );
        assert_eq!(uris::DOCS_URI, conventions.docs_root_uri());
        assert_eq!(uris::CONTRACT_URI, conventions.contract_uri());
        assert_eq!(uris::DOC_TEMPLATE, conventions.doc_template());
        assert!(
            TimeResource::parse(ResourceUri::new(uris::TIMELINE_APP_URI).unwrap().as_str()).is_ok()
        );
    }

    #[test]
    fn discovery_uses_static_roots_and_templates_without_list_change_notifications() {
        let capabilities = SERVER_SETUP
            .server_config()
            .capabilities
            .resources
            .as_ref()
            .unwrap();
        assert!(!capabilities.list_changed.unwrap_or(false));
        let roots: Vec<_> = SERVER_SETUP
            .resources()
            .iter()
            .map(|resource| resource.descriptor())
            .collect();
        for root in [uris::CALENDARS_URI, uris::EPOCHS_URI, uris::EVENTS_URI] {
            assert!(roots.iter().any(|resource| resource.uri == root));
        }
        let templates = SERVER_SETUP.resource_templates();
        for template in [
            uris::CALENDARS_TEMPLATE,
            uris::EPOCHS_TEMPLATE,
            uris::EVENTS_TEMPLATE,
        ] {
            assert!(
                templates
                    .iter()
                    .any(|resource| resource.template().as_str() == template)
            );
        }
    }

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!TimeMcp::tool_router().list_all().is_empty());
    }

    #[test]
    fn checked_setup_preserves_timeline_app_discovery_metadata() {
        let resource = SERVER_SETUP
            .resources()
            .iter()
            .find(|resource| resource.address() == &crate::contract::TimeResource::TimelineApp)
            .expect("Timeline App is advertised")
            .descriptor();
        assert_eq!(resource.uri, uris::TIMELINE_APP_URI);
        assert_eq!(resource.name, "timeline");
        assert_eq!(resource.title.as_deref(), Some("Timeline"));
        assert_eq!(
            resource.mime_type.as_deref(),
            Some(veoveo_mcp_apps_extension::APP_MIME_TYPE)
        );
        assert!(
            resource
                .meta
                .as_ref()
                .is_some_and(|meta| meta.0.contains_key(veoveo_mcp_apps_extension::UI_META_KEY))
        );
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
