use std::sync::{Arc, LazyLock};

use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo,
        GetPromptRequestParams, GetPromptResult, Prompt, ReadResourceRequestParams, Reference,
        Resource, Tool,
    },
    service::RequestContext,
    tool_router,
};
use uuid::Uuid;
use veoveo_mcp_contract::{
    GatewayInternalIdentity,
    docs::ServerDocs,
    hosting::{
        DomainAddress, DomainRead, DomainServer, Listing, completion, gateway_identity,
        rank_completions, structured_result, unknown_prompt,
    },
    server_contract::McpServerSetup,
};

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
    state::TimeApplication,
    uris,
};

mod resources;
mod setup;
mod subscriptions;
pub(crate) use setup::SERVER_SETUP;
use setup::TimeContract;
pub(crate) use subscriptions::TimeSubscriptions;

/// The crate documents embedded at build time and served under the well-known
/// surface: `time://docs`, `time://docs/{doc_id}`, `time://contract`, and the
/// administrative `admin/docs` routes (contract C18-C21).
pub(crate) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("time"));

#[derive(Clone)]
pub struct TimeMcp {
    state: Arc<TimeApplication>,
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
        structured_result(
            format!("resolved {}", output.projection().utc_rfc3339),
            &output,
        )
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
            .map_err(internal)?
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
        let output = assess_clock(self.state.clock.quality().await.map_err(internal)?, policy)
            .map_err(internal)?;
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
            event_id: TemporalEventId::parse(format!("event-{}", Uuid::now_v7()))
                .map_err(invalid_params)?,
            name: request.name,
            due: request.due,
            state: TemporalEventState::Scheduled,
            record_version: crate::TimeVersion::new(1).unwrap(),
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

impl DomainServer for TimeMcp {
    type Contract = TimeContract;

    fn setup() -> &'static McpServerSetup<TimeContract> {
        &SERVER_SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    fn describe_tool(&self, tool: Tool) -> Tool {
        veoveo_mcp_apps_extension::link_tool_to_app(
            tool,
            uris::TIMELINE_APP_URI,
            &[
                veoveo_mcp_apps_extension::UiVisibility::Model,
                veoveo_mcp_apps_extension::UiVisibility::App,
            ],
        )
    }

    async fn list_resources(
        &self,
        declared: Vec<Resource>,
        _cursor: Option<&str>,
        context: &RequestContext<RoleServer>,
    ) -> Result<Listing<Resource>, McpError> {
        require_scope(context, TimeScope::Read)?;
        Ok(Listing::all(declared))
    }

    async fn read(
        &self,
        address: DomainAddress<TimeContract>,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, McpError> {
        self.read_time_resource(address, &request.uri, context)
            .await
            .map(DomainRead::private)
    }

    fn prompts(&self) -> Vec<Prompt> {
        TimePrompt::ALL
            .into_iter()
            .map(TimePrompt::definition)
            .collect()
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        TimePrompt::by_name(&request.name)
            .ok_or_else(|| unknown_prompt(&request.name))?
            .render(request.arguments)
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
        let domain = match (reference.uri.as_str(), request.argument.name.as_str()) {
            (uris::CALENDAR_TEMPLATE, "calendar_id") => Some(TimeCompletion::CalendarId),
            (uris::CALENDAR_TEMPLATE, "version") => Some(TimeCompletion::CalendarVersion {
                calendar_key: request
                    .context
                    .as_ref()
                    .and_then(|context| context.get_argument("calendar_id"))
                    .map(|key| crate::contract::CalendarId::parse(key.clone()))
                    .transpose()
                    .map_err(invalid_params)?,
            }),
            (uris::EPOCH_TEMPLATE | uris::EPOCH_VERSION_TEMPLATE, "epoch_id") => {
                Some(TimeCompletion::EpochId)
            }
            (uris::EPOCH_VERSION_TEMPLATE, "version") => Some(TimeCompletion::EpochVersion {
                epoch_key: request
                    .context
                    .as_ref()
                    .and_then(|context| context.get_argument("epoch_id"))
                    .map(|key| crate::MissionEpochId::parse(key.clone()))
                    .transpose()
                    .map_err(invalid_params)?,
            }),
            (uris::AUTHORITY_RELEASE_TEMPLATE, "release_id") => {
                Some(TimeCompletion::AuthorityReleaseId)
            }
            (uris::EVENT_TEMPLATE, "event_id") => Some(TimeCompletion::EventId),
            _ => None,
        };
        if let Some(domain) = domain {
            // One value beyond a page shows that more matches exist.
            let values = self
                .state
                .catalog
                .complete_values(
                    &scope,
                    domain,
                    &request.argument.value,
                    u32::try_from(CompletionInfo::MAX_VALUES + 1).unwrap_or(u32::MAX),
                )
                .await
                .map_err(internal)?;
            return completion(values);
        }
        // These catalogs are packaged with the server's TZDB and bootstrap data.
        let mut values: Vec<String> = match (reference.uri.as_str(), request.argument.name.as_str())
        {
            (uris::BOOTSTRAP_AUTHORITY_TEMPLATE, "release_id") => self
                .state
                .authorities
                .bootstrap_references()
                .into_iter()
                .map(|reference| reference.release_id().to_string())
                .collect(),
            (uris::ZONE_TEMPLATE, "zone_id") => self
                .state
                .engine(&scope)
                .await
                .map_err(internal)?
                .authority()
                .tzdb()
                .available()
                .map(|name| name.to_string())
                .collect(),
            _ => Vec::new(),
        };
        values.sort();
        values.dedup();
        completion(rank_completions(
            values.iter().map(String::as_str),
            &request.argument.value,
        ))
    }
}

fn require_scope(
    context: &RequestContext<RoleServer>,
    required: TimeScope,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = gateway_identity(context)?;
    crate::server::auth::require_scope(&identity.actor.scopes, required)?;
    Ok(identity)
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
    fn knowledge_templates_declare_each_owned_collection() {
        use veoveo_mcp_knowledge_extension::{CollectionDescriptor, EXTENSION_ID};
        let mut collections = std::collections::BTreeSet::new();
        for template in SERVER_SETUP.resource_templates() {
            if let Some(collection) =
                crate::TimeKnowledgeCollection::for_template(template.template().as_str())
            {
                let descriptor = template.descriptor();
                let wire = descriptor.meta.as_ref().unwrap().get(EXTENSION_ID).unwrap();
                let declared: CollectionDescriptor = serde_json::from_value(wire.clone()).unwrap();
                assert_eq!(declared, collection.descriptor());
                assert!(collections.insert(declared.collection().clone()));
            }
        }
        assert_eq!(collections.len(), 5);
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
            veoveo_types::ResourceScheme::parse("time").expect("declared resource scheme"),
        );
        assert_eq!(uris::DOCS_URI, conventions.docs_root_uri());
        assert_eq!(uris::CONTRACT_URI, conventions.contract_uri().as_str());
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
