use super::{
    auth::{artifact_caller_from_context, identity},
    prompts::RecordingPrompt,
    resources,
    state::AppState,
};
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock,
        GetPromptRequestParams, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParams, Prompt,
        ReadResourceRequestParams, ReadResourceResult, Reference, ResourceContents, ServerConfig,
        SubscriptionFilter,
    },
    service::{RequestContext, SubscriptionContext},
    tool, tool_handler, tool_router,
};
use serde::Serialize;
use std::sync::Arc;
use veoveo_mcp_contract::{Page, paginate};
use veoveo_platform_store::RecordingId;
use veoveo_recording_mcp::{
    contract::{
        CreateRecordingProjectionRequest, RecordingProjectionHandle, RecordingResource,
        SealRecordingOutput, SealRecordingRequest,
    },
    mcp_setup::SERVER_SETUP,
    uris,
};

const LIST_PAGE_SIZE: usize = 100;
const EXPLORER_TOOLS: &[&str] = &["create_recording_projection", "seal_recording"];

#[derive(Clone)]
pub(super) struct RecordingMcp {
    state: Arc<AppState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<RecordingMcp>,
}

#[tool_router]
impl RecordingMcp {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Seal recording",
        description = "Check a recording's finished layers, publish its v9 manifest as an artifact, and seal the recording in one step. Requires the recording:seal scope.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SealRecordingOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn seal_recording(
        &self,
        Parameters(request): Parameters<SealRecordingRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let recording_id = RecordingId::from_uuid(request.recording_id.as_uuid());
        let identity = identity(&context)?;
        let output = self
            .state
            .recordings
            .seal(&identity, recording_id)
            .await
            .map_err(invalid_params)?;
        self.state
            .subscribers
            .notify_resource_updated(uris::recording_uri(request.recording_id))
            .await;
        self.state
            .subscribers
            .notify_resource_updated(uris::layers_uri(request.recording_id))
            .await;
        self.state
            .subscribers
            .notify_resource_updated(uris::CATALOG_URI)
            .await;
        structured_result("recording sealed".to_owned(), &output)
    }

    #[tool(
        title = "Create recording projection",
        description = "Extract selected entities and components from one recording as an Apache Arrow stream. The same selection always returns the same data. The returned handle contains no credentials.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<RecordingProjectionHandle>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = true
        )
    )]
    async fn create_recording_projection(
        &self,
        Parameters(request): Parameters<CreateRecordingProjectionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = identity(&context)?;
        let artifact_caller = artifact_caller_from_context(&context, identity.clone())?;
        let cancellation = context.ct.clone();
        self.state.playback.prune_catalogs();
        match self
            .state
            .recordings
            .create_projection(&identity, &artifact_caller, request, cancellation)
            .await
        {
            Ok(handle) => structured_result("recording projection ready".to_owned(), &handle),
            Err(error) => {
                tracing::warn!(%error, "recording projection request failed");
                Err(McpError::invalid_params(
                    "recording projection request was rejected",
                    None,
                ))
            }
        }
    }
}

#[tool_handler]
impl ServerHandler for RecordingMcp {
    fn supported_protocol_versions(
        &self,
    ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
        veoveo_mcp_contract::final_protocol_versions()
    }

    fn get_info(&self) -> ServerConfig {
        SERVER_SETUP.server_config().clone()
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
                if EXPLORER_TOOLS.contains(&tool.name.as_ref()) {
                    veoveo_mcp_apps_extension::link_tool_to_app(
                        tool,
                        uris::EXPLORER_APP_URI,
                        &[
                            veoveo_mcp_apps_extension::UiVisibility::Model,
                            veoveo_mcp_apps_extension::UiVisibility::App,
                        ],
                    )
                } else {
                    tool
                }
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
        identity(&context)?;
        let resources = SERVER_SETUP
            .resources()
            .iter()
            .map(|item| item.descriptor().clone())
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
        let templates = SERVER_SETUP
            .resource_templates()
            .iter()
            .map(|item| item.descriptor().clone())
            .collect();
        let page = mcp_page(templates, request.as_ref())?;
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
        if let Some(result) = SERVER_SETUP.read_documents(&request, &context)? {
            return Ok(result);
        }
        let cacheable = request.request_state.is_none() && request.input_responses.is_none();
        let identity = identity(&context)?;
        let resource = RecordingResource::parse(&request.uri).map_err(invalid_params)?;
        resources::read(&self.state, &identity, &request.uri, resource)
            .await
            .map(|result| veoveo_mcp_contract::private_resource_response(result, cacheable))
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let prompts: Vec<Prompt> = RecordingPrompt::ALL
            .into_iter()
            .map(RecordingPrompt::definition)
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
            RecordingPrompt::by_name(&request.name)
                .ok_or_else(|| McpError::invalid_params("unknown recording prompt", None))?
                .render(request.arguments)
        }
        .await
        .map(Into::into)
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        accepted_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        let request_context = context.request_context().clone();
        let identity = identity(&request_context)?;
        for uri in context.accepted().resource_subscriptions.iter().flatten() {
            let SubscriptionResource::Recording(recording_id) = subscription_resource(uri)? else {
                // The catalog itself is readable by every authenticated caller;
                // its contents are filtered by current recording visibility.
                continue;
            };
            if self
                .state
                .recordings
                .recording_view(&identity, recording_id)
                .await
                .map_err(internal)?
                .is_none()
            {
                return Err(McpError::resource_not_found(
                    format!("Recording `{recording_id}` was not found."),
                    None,
                ));
            }
        }
        veoveo_mcp_contract::listen_resources(context, &self.state.subscribers, None).await
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        if !matches!(
            reference.uri.as_str(),
            uris::RECORDING_TEMPLATE | uris::LAYERS_TEMPLATE
        ) || request.argument.name != "recording_id"
        {
            return Ok(CompleteResult::default());
        }
        let identity = identity(&context)?;
        let mut values = self
            .state
            .recordings
            .complete_recording_ids(&identity, &request.argument.value)
            .await
            .map_err(resources::query_error)?;
        let has_more = values.len() > CompletionInfo::MAX_VALUES;
        values.truncate(CompletionInfo::MAX_VALUES);
        let total = (!has_more).then_some(values.len() as u32);
        let completion =
            CompletionInfo::with_pagination(values, total, has_more).map_err(internal)?;
        Ok(CompleteResult::new(completion))
    }
}

fn structured_result<T: Serialize>(text: String, value: &T) -> Result<CallToolResult, McpError> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(serde_json::to_value(value).map_err(internal)?);
    Ok(result)
}

pub(super) fn json_resource<T: Serialize>(
    uri: &str,
    value: &T,
) -> Result<ReadResourceResult, McpError> {
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

#[derive(Debug, PartialEq, Eq)]
enum SubscriptionResource {
    Catalog,
    Recording(RecordingId),
}

fn subscription_resource(uri: &str) -> Result<SubscriptionResource, McpError> {
    match RecordingResource::parse(uri).map_err(invalid_params)? {
        RecordingResource::Catalog(None) => Ok(SubscriptionResource::Catalog),
        RecordingResource::Recording(uri) => Ok(SubscriptionResource::Recording(
            RecordingId::from_uuid(uri.id().as_uuid()),
        )),
        RecordingResource::Layers(uri) => Ok(SubscriptionResource::Recording(
            RecordingId::from_uuid(uri.id().as_uuid()),
        )),
        _ => Err(McpError::invalid_params(
            "resource is not subscribable",
            None,
        )),
    }
}

fn invalid_params(error: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}

pub(super) fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

fn accepted_subscription_filter(requested: &SubscriptionFilter) -> Option<SubscriptionFilter> {
    let resources = requested
        .resource_subscriptions
        .as_ref()?
        .iter()
        .filter(|uri| subscription_resource(uri).is_ok())
        .cloned()
        .collect::<Vec<_>>();
    (!resources.is_empty()).then(|| {
        SubscriptionFilter::builder()
            .resource_subscriptions(resources)
            .build()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_recording_mcp::admin::SERVER_DOCS;

    #[test]
    fn listener_does_not_accept_static_lists_documents_or_foreign_tasks() {
        let requested = SubscriptionFilter::builder()
            .resources_list_changed()
            .task_ids(["foreign-task".to_owned()])
            .resource_subscriptions([
                uris::DOCS_URI.to_owned(),
                "recording://catalog?extra=1".into(),
            ])
            .build();
        assert!(accepted_subscription_filter(&requested).is_none());
        let recording =
            uris::recording_uri(veoveo_recording_mcp::contract::RecordingId::new()).to_string();
        let requested = SubscriptionFilter::builder()
            .resources_list_changed()
            .task_ids(["foreign-task".to_owned()])
            .resource_subscriptions([
                uris::CATALOG_URI.to_owned(),
                recording.clone(),
                uris::DOCS_URI.into(),
            ])
            .build();
        let accepted = accepted_subscription_filter(&requested).unwrap();
        assert_eq!(
            accepted.resource_subscriptions,
            Some(vec![uris::CATALOG_URI.into(), recording])
        );
        assert_ne!(accepted.resources_list_changed, Some(true));
        assert!(accepted.task_ids.is_none());
    }

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!RecordingMcp::tool_router().list_all().is_empty());
    }

    #[test]
    fn tools_publish_safety_annotations() {
        let tools = RecordingMcp::tool_router();
        let seal = tools
            .list_all()
            .into_iter()
            .find(|tool| tool.name == "seal_recording")
            .unwrap();
        let annotations = seal.annotations.unwrap();
        assert_eq!(annotations.read_only_hint, Some(false));
        assert_eq!(annotations.idempotent_hint, Some(true));
        assert_eq!(annotations.open_world_hint, Some(true));

        let projection = RecordingMcp::tool_router()
            .list_all()
            .into_iter()
            .find(|tool| tool.name == "create_recording_projection")
            .unwrap();
        let annotations = projection.annotations.unwrap();
        assert_eq!(annotations.read_only_hint, Some(true));
        assert_eq!(annotations.idempotent_hint, Some(true));
        assert_eq!(annotations.open_world_hint, Some(true));
    }

    #[test]
    fn subscriptions_accept_catalog_and_recording_resources() {
        let id = uuid::Uuid::now_v7();
        for uri in [
            uris::recording_uri(veoveo_recording_mcp::contract::RecordingId::try_from(id).unwrap())
                .to_string(),
            uris::layers_uri(veoveo_recording_mcp::contract::RecordingId::try_from(id).unwrap())
                .to_string(),
        ] {
            assert_eq!(
                subscription_resource(&uri).unwrap(),
                SubscriptionResource::Recording(RecordingId::from_uuid(id))
            );
        }
        assert_eq!(
            subscription_resource(uris::CATALOG_URI).unwrap(),
            SubscriptionResource::Catalog
        );
        for uri in [
            uris::DOCS_URI,
            "recording://catalog/extra",
            "recording://recordings/not-a-uuid",
            "recording://recordings/00000000-0000-0000-0000-000000000000",
        ] {
            assert!(subscription_resource(uri).is_err(), "{uri}");
        }
    }

    #[test]
    fn contract_declaration_resolves_from_the_embedded_manual() {
        use veoveo_mcp_contract::docs::{CONTRACT_REVISION, ComplianceStatus};

        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        assert_eq!(declaration.server, "recording");
        assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance
                .iter()
                .find(|item| item.id == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}
