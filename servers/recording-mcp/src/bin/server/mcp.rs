use super::{
    auth::artifact_caller_from_context, prompts::RecordingPrompt, resources, state::AppState,
};
use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo,
        GetPromptRequestParams, GetPromptResult, Prompt, ReadResourceRequestParams, Reference,
        Tool,
    },
    service::RequestContext,
    tool, tool_router,
};
use std::sync::Arc;
use veoveo_mcp_contract::{
    SubscriptionHub,
    hosting::{
        DomainAddress, DomainRead, DomainServer, ResourceSubscriptions, gateway_identity,
        structured_result, unknown_prompt,
    },
    server_contract::McpServerSetup,
};
use veoveo_recording_mcp::{
    contract::{
        CreateRecordingProjectionRequest, RecordingProjectionHandle, RecordingResource,
        SealRecordingOutput, SealRecordingRequest,
    },
    mcp_setup::{RecordingContract, SERVER_SETUP},
    uris,
};
use veoveo_recording_store::RecordingId;
use veoveo_recording_store::RecordingRepository;

const EXPLORER_TOOLS: &[&str] = &["create_recording_projection", "seal_recording"];

#[derive(Clone)]
pub(super) struct RecordingMcp {
    state: Arc<AppState>,
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
        let identity = gateway_identity(&context)?;
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
        let identity = gateway_identity(&context)?;
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

impl DomainServer for RecordingMcp {
    type Contract = RecordingContract;

    fn setup() -> &'static McpServerSetup<RecordingContract> {
        &SERVER_SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    fn describe_tool(&self, tool: Tool) -> Tool {
        if !EXPLORER_TOOLS.contains(&tool.name.as_ref()) {
            return tool;
        }
        veoveo_mcp_apps_extension::link_tool_to_app(
            tool,
            uris::EXPLORER_APP_URI,
            &[
                veoveo_mcp_apps_extension::UiVisibility::Model,
                veoveo_mcp_apps_extension::UiVisibility::App,
            ],
        )
    }

    async fn read(
        &self,
        address: DomainAddress<RecordingContract>,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, McpError> {
        let identity = gateway_identity(context)?;
        resources::read(&self.state, &identity, &request.uri, address)
            .await
            .map(DomainRead::private)
    }

    fn prompts(&self) -> Vec<Prompt> {
        RecordingPrompt::ALL
            .into_iter()
            .map(RecordingPrompt::definition)
            .collect()
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        RecordingPrompt::by_name(&request.name)
            .ok_or_else(|| unknown_prompt(&request.name))?
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
        if !matches!(
            reference.uri.as_str(),
            uris::RECORDING_TEMPLATE | uris::LAYERS_TEMPLATE
        ) || request.argument.name != "recording_id"
        {
            return Ok(CompleteResult::default());
        }
        let identity = gateway_identity(&context)?;
        let mut values = self
            .state
            .recordings
            .recording_repository()
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

/// The recording catalog and individual recordings. Subscribing to a recording
/// requires that the caller can currently see it.
pub(super) struct RecordingSubscriptions {
    state: Arc<AppState>,
}

impl RecordingSubscriptions {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl ResourceSubscriptions for RecordingSubscriptions {
    type Address = RecordingResource;

    async fn authorize(
        &self,
        addresses: Vec<RecordingResource>,
        context: &RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = gateway_identity(context)?;
        for address in addresses {
            let SubscriptionResource::Recording(recording_id) = subscription_resource(address)?
            else {
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
        Ok(())
    }

    fn hub(&self) -> &SubscriptionHub {
        &self.state.subscribers
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SubscriptionResource {
    Catalog,
    Recording(RecordingId),
}

fn subscription_resource(address: RecordingResource) -> Result<SubscriptionResource, McpError> {
    match address {
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

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_recording_mcp::admin::SERVER_DOCS;

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
                subscription_resource(RecordingResource::parse(&uri).unwrap()).unwrap(),
                SubscriptionResource::Recording(RecordingId::from_uuid(id))
            );
        }
        assert_eq!(
            subscription_resource(RecordingResource::parse(uris::CATALOG_URI).unwrap()).unwrap(),
            SubscriptionResource::Catalog
        );
        for uri in [
            uris::DOCS_URI,
            "recording://catalog/extra",
            "recording://recordings/not-a-uuid",
            "recording://recordings/00000000-0000-0000-0000-000000000000",
        ] {
            assert!(
                RecordingResource::parse(uri)
                    .map_err(invalid_params)
                    .and_then(subscription_resource)
                    .is_err(),
                "{uri}"
            );
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
