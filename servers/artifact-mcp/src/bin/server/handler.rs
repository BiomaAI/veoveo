use std::{num::NonZeroU64, sync::Arc};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo,
        GetPromptRequestParams, GetPromptResult, Prompt, ReadResourceRequestParams,
        ReadResourceResult, Reference, Resource, ResourceContents, SubscriptionFilter, Tool,
    },
    service::{RequestContext, SubscriptionContext},
    tool_router,
};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_artifact_contract::{ArtifactId, ArtifactMetadata, parse_artifact_plane_uri};
use veoveo_artifact_contract::{CreateArtifactShareLinkRequest, ListArtifactsRequest};
use veoveo_artifact_mcp::contract::{
    ARTIFACT_TEMPLATE, ArtifactGrantsOutput, ArtifactIndexPage, ArtifactMetadataOutput,
    ArtifactMutationOutput, ArtifactReference, ArtifactResource, ArtifactShareOutput,
    CreateArtifactShareRequest, GRANTS_TEMPLATE, GrantArtifactRequest, INDEX_URI, LIBRARY_APP_URI,
    METADATA_TEMPLATE, RevokeArtifactGrantRequest, RevokeArtifactShareRequest,
    SetArtifactReleaseRequest, parse_grants_uri, parse_metadata_uri,
};
use veoveo_mcp_contract::{
    ArtifactPlane, ArtifactPlaneError, PlaneCaller,
    hosting::{
        CATALOG_PAGE_SIZE, DomainAddress, DomainRead, DomainServer, Listing, SubscriptionListener,
        json_read, plane_caller, structured_result, unknown_prompt,
    },
    server_contract::McpServerSetup,
};
use veoveo_types::AccessLevel;

#[cfg(test)]
use super::setup::SERVER_DOCS;
use super::{
    prompts::ArtifactPrompt,
    setup::{ArtifactContract, SERVER_SETUP},
    subscriptions::ArtifactSubscriptions,
};

const LIBRARY_TOOLS: &[&str] = &[
    "create_share_link",
    "grant_access",
    "metadata",
    "revoke_access",
    "revoke_share_link",
    "set_release_state",
];

#[derive(Clone)]
pub(super) struct AppState {
    pub(super) plane: HttpArtifactPlane,
    pub(super) subscriptions: ArtifactSubscriptions,
    pub(super) public_base_url: String,
}

impl AppState {
    fn expose_download(
        &self,
        caller: &PlaneCaller,
        mut artifact: ArtifactMetadata,
    ) -> ArtifactMetadata {
        artifact.download_url = Some(format!(
            "{}/artifacts/{}/{}/download",
            self.public_base_url,
            caller.identity.profile,
            artifact.artifact_id()
        ));
        artifact
    }
}

#[derive(Clone)]
pub(super) struct ArtifactMcp {
    state: Arc<AppState>,
    tool_router: ToolRouter<ArtifactMcp>,
}

#[tool_router]
impl ArtifactMcp {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        std::sync::LazyLock::force(&SERVER_SETUP);
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Read artifact metadata",
        description = "Read an artifact's metadata. Fields your access doesn't allow are left out.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ArtifactMetadataOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn metadata(
        &self,
        Parameters(request): Parameters<ArtifactReference>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let caller = plane_caller(&context)?;
        let artifact = self
            .state
            .plane
            .head(&caller, &request.artifact_id)
            .await
            .map_err(plane_error)?;
        let artifact = self.state.expose_download(&caller, artifact);
        structured_result(
            format!("artifact {} metadata", request.artifact_id),
            &ArtifactMetadataOutput { artifact },
        )
    }

    #[tool(
        title = "Grant artifact access",
        description = "Grant read, write, or admin access to one user or group. The caller must be an artifact administrator.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ArtifactGrantsOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn grant_access(
        &self,
        Parameters(request): Parameters<GrantArtifactRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let caller = plane_caller(&context)?;
        self.state
            .plane
            .grant(
                &caller,
                &request.artifact_id,
                request.subject,
                request.level,
            )
            .await
            .map_err(plane_error)?;
        let grants = self
            .state
            .plane
            .list_grants(&caller, &request.artifact_id)
            .await
            .map_err(plane_error)?;
        structured_result(
            format!("updated grants for artifact {}", request.artifact_id),
            &ArtifactGrantsOutput {
                artifact_id: request.artifact_id,
                grants,
            },
        )
    }

    #[tool(
        title = "Revoke artifact access",
        description = "Remove one user or group grant. The immutable owner admin grant cannot be removed.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ArtifactGrantsOutput>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn revoke_access(
        &self,
        Parameters(request): Parameters<RevokeArtifactGrantRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let caller = plane_caller(&context)?;
        self.state
            .plane
            .revoke(&caller, &request.artifact_id, &request.subject)
            .await
            .map_err(plane_error)?;
        let grants = self
            .state
            .plane
            .list_grants(&caller, &request.artifact_id)
            .await
            .map_err(plane_error)?;
        structured_result(
            format!("updated grants for artifact {}", request.artifact_id),
            &ArtifactGrantsOutput {
                artifact_id: request.artifact_id,
                grants,
            },
        )
    }

    #[tool(
        title = "Set artifact release state",
        description = "Set whether an artifact is private, releasable, or released. Public bearer links require releasable or released state.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ArtifactMetadataOutput>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn set_release_state(
        &self,
        Parameters(request): Parameters<SetArtifactReleaseRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let caller = plane_caller(&context)?;
        let artifact = self
            .state
            .plane
            .set_release_state(&caller, &request.artifact_id, request.release_state)
            .await
            .map_err(plane_error)?;
        let artifact = self.state.expose_download(&caller, artifact);
        structured_result(
            format!("updated release state for artifact {}", request.artifact_id),
            &ArtifactMetadataOutput { artifact },
        )
    }

    #[tool(
        title = "Create anyone-with-link share",
        description = "Create a revocable, read-only bearer link for an explicitly releasable artifact. Default expiry is seven days and maximum expiry is thirty days.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ArtifactShareOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true)
    )]
    async fn create_share_link(
        &self,
        Parameters(request): Parameters<CreateArtifactShareRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let max_downloads = request
            .options
            .max_downloads
            .map(|value| {
                NonZeroU64::new(value).ok_or_else(|| {
                    McpError::invalid_params("max_downloads must be greater than zero", None)
                })
            })
            .transpose()?;
        let share_link = self
            .state
            .plane
            .create_share_link(
                &plane_caller(&context)?,
                &request.artifact_id,
                CreateArtifactShareLinkRequest {
                    expires_at: request.options.expires_at,
                    max_downloads,
                },
            )
            .await
            .map_err(plane_error)?;
        structured_result(
            format!("created share link for artifact {}", request.artifact_id),
            &ArtifactShareOutput { share_link },
        )
    }

    #[tool(
        title = "Revoke anyone-with-link share",
        description = "Revoke one artifact bearer link immediately.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ArtifactMutationOutput>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn revoke_share_link(
        &self,
        Parameters(request): Parameters<RevokeArtifactShareRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.state
            .plane
            .revoke_share_link(
                &plane_caller(&context)?,
                &request.artifact_id,
                &request.link_id,
            )
            .await
            .map_err(plane_error)?;
        structured_result(
            format!("revoked share link for artifact {}", request.artifact_id),
            &ArtifactMutationOutput {
                artifact_id: request.artifact_id,
                changed: true,
            },
        )
    }
}

impl DomainServer for ArtifactMcp {
    type Contract = ArtifactContract;

    fn setup() -> &'static McpServerSetup<ArtifactContract> {
        &SERVER_SETUP
    }

    fn tool_router(&self) -> &ToolRouter<Self> {
        &self.tool_router
    }

    fn describe_tool(&self, tool: Tool) -> Tool {
        if !LIBRARY_TOOLS.contains(&tool.name.as_ref()) {
            return tool;
        }
        veoveo_mcp_apps_extension::link_tool_to_app(
            tool,
            LIBRARY_APP_URI,
            &[
                veoveo_mcp_apps_extension::UiVisibility::Model,
                veoveo_mcp_apps_extension::UiVisibility::App,
            ],
        )
    }

    /// The well-known resources ride the first page, and the caller's artifacts
    /// continue under the Artifact plane's cursor (contract C18, C19).
    async fn list_resources(
        &self,
        declared: Vec<Resource>,
        cursor: Option<&str>,
        context: &RequestContext<RoleServer>,
    ) -> Result<Listing<Resource>, McpError> {
        let cursor = cursor
            .map(ArtifactId::parse)
            .transpose()
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let page = self
            .state
            .plane
            .list(
                &plane_caller(context)?,
                ListArtifactsRequest {
                    cursor,
                    limit: Some(CATALOG_PAGE_SIZE as u16),
                },
            )
            .await
            .map_err(plane_error)?;
        let mut resources = if cursor.is_none() {
            declared
        } else {
            Vec::new()
        };
        resources.extend(page.artifacts.into_iter().map(|artifact| {
            let artifact_id = artifact.artifact_id();
            Resource::new(
                artifact.artifact_uri,
                artifact
                    .filename
                    .clone()
                    .unwrap_or_else(|| artifact_id.to_string()),
            )
            .with_title(
                artifact
                    .filename
                    .unwrap_or_else(|| format!("Artifact {artifact_id}")),
            )
            .with_mime_type(
                artifact
                    .mime_type
                    .unwrap_or_else(|| "application/octet-stream".to_owned()),
            )
        }));
        Ok(Listing::page(
            resources,
            page.next_cursor.map(|cursor| cursor.to_string()),
        ))
    }

    /// The host serves documents and the contract. Artifact dispatches its
    /// other reads by the admitted URI.
    async fn read(
        &self,
        _address: DomainAddress<ArtifactContract>,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<DomainRead, McpError> {
        let caller = plane_caller(context)?;
        let uri = request.uri.as_str();
        if uri == LIBRARY_APP_URI {
            let html = veoveo_mcp_apps_extension::workbench_app_html(
                &veoveo_mcp_apps_extension::WorkbenchApp {
                    app_id: "artifact-library",
                    title: "Library",
                    subtitle: "Find, inspect, release, and share artifacts",
                    empty_message: "No artifacts are visible to this identity.",
                    resources: &[veoveo_mcp_apps_extension::WorkbenchResource {
                        label: "Artifact index",
                        uri: INDEX_URI,
                    }],
                    tools: &[
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Inspect metadata",
                            name: "metadata",
                            arguments_json: r#"{"artifact_id":""}"#,
                        },
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Set release state",
                            name: "set_release_state",
                            arguments_json: r#"{"artifact_id":"","release_state":"releasable"}"#,
                        },
                        veoveo_mcp_apps_extension::WorkbenchTool {
                            label: "Create share link",
                            name: "create_share_link",
                            arguments_json: r#"{"artifact_id":""}"#,
                        },
                    ],
                    stream_result: None,
                },
            );
            return Ok(DomainRead::private(ReadResourceResult::new(vec![
                veoveo_mcp_apps_extension::app_html_contents(uri, &html),
            ])));
        }
        if let Ok(ArtifactResource::Index { cursor }) = ArtifactResource::parse(uri) {
            let page = self
                .state
                .plane
                .list(
                    &caller,
                    ListArtifactsRequest {
                        cursor: cursor.map(|cursor| cursor.after()),
                        limit: Some(100),
                    },
                )
                .await
                .map_err(plane_error)?;
            let index = ArtifactIndexPage::new(page.artifacts, page.next_cursor)
                .map_err(|error| McpError::internal_error(error.to_string(), None))?;
            return json_read(uri, &index).map(DomainRead::private);
        }
        if let Some(artifact_id) = parse_metadata_uri(uri) {
            let snapshot = self
                .state
                .plane
                .metadata_snapshot(&caller, &artifact_id)
                .await
                .map_err(resource_error)?;
            let (text, observation) =
                veoveo_artifact_mcp::knowledge::metadata_document(&snapshot, chrono::Utc::now())
                    .map_err(|error| McpError::internal_error(error.to_string(), None))?;
            let resource = ArtifactResource::Metadata(artifact_id).to_uri();
            let result = veoveo_mcp_knowledge_extension::server::member_result(
                &resource,
                "application/json",
                text,
                observation,
                &veoveo_artifact_mcp::knowledge::collection(),
                Some(&context.meta),
            )
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
            // Metadata members follow current grants, so no read is reused.
            return Ok(DomainRead::no_store(result));
        }
        if let Some(artifact_id) = parse_grants_uri(uri) {
            let grants = self
                .state
                .plane
                .list_grants(&caller, &artifact_id)
                .await
                .map_err(resource_error)?;
            return json_read(
                uri,
                &ArtifactGrantsOutput {
                    artifact_id,
                    grants,
                },
            )
            .map(DomainRead::private);
        }
        if let Some(artifact_id) = parse_artifact_plane_uri(uri) {
            let artifact = self
                .state
                .plane
                .get(&caller, &artifact_id, AccessLevel::Read)
                .await
                .map_err(resource_error)?;
            let mut contents = ResourceContents::blob(BASE64_STANDARD.encode(artifact.bytes), uri);
            contents = contents.with_mime_type(
                artifact
                    .metadata
                    .mime_type
                    .unwrap_or_else(|| "application/octet-stream".to_owned()),
            );
            return Ok(DomainRead::private(ReadResourceResult::new(vec![contents])));
        }
        Err(McpError::invalid_params(
            format!("unknown resource uri: {uri}"),
            None,
        ))
    }

    fn prompts(&self) -> Vec<Prompt> {
        ArtifactPrompt::ALL
            .into_iter()
            .map(ArtifactPrompt::prompt)
            .collect()
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        ArtifactPrompt::by_name(&request.name)
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
            ARTIFACT_TEMPLATE | METADATA_TEMPLATE | GRANTS_TEMPLATE
        ) || request.argument.name != "artifact_id"
        {
            return Ok(CompleteResult::default());
        }
        let needle = request.argument.value.to_ascii_lowercase();
        let page = self
            .state
            .plane
            .list(
                &plane_caller(&context)?,
                ListArtifactsRequest {
                    cursor: None,
                    limit: Some(100),
                },
            )
            .await
            .map_err(plane_error)?;
        let values: Vec<String> = page
            .artifacts
            .into_iter()
            .map(|artifact| artifact.artifact_id().to_string())
            .filter(|id| id.starts_with(&needle))
            .take(CompletionInfo::MAX_VALUES)
            .collect();
        let completion = CompletionInfo::with_pagination(values, None, page.next_cursor.is_some())
            .map_err(|error| McpError::internal_error(error, None))?;
        Ok(CompleteResult::new(completion))
    }
}

/// Artifact subscriptions follow the Artifact plane's change stream and end
/// when the caller loses access.
pub(super) struct ArtifactListener {
    pub(super) state: Arc<AppState>,
}

impl SubscriptionListener for ArtifactListener {
    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        veoveo_mcp_contract::accepted_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        super::subscriptions::listen(&self.state.plane, &self.state.subscriptions, context).await
    }
}

fn resource_error(error: ArtifactPlaneError) -> McpError {
    match error {
        ArtifactPlaneError::NotFound | ArtifactPlaneError::Denied(_) => {
            McpError::invalid_params("artifact is unavailable", None)
        }
        other => plane_error(other),
    }
}

pub(super) fn plane_error(error: ArtifactPlaneError) -> McpError {
    match error {
        ArtifactPlaneError::NotFound | ArtifactPlaneError::Denied(_) => {
            McpError::invalid_request("artifact is unavailable", None)
        }
        ArtifactPlaneError::Unauthenticated => {
            McpError::invalid_request("artifact authorization expired", None)
        }
        ArtifactPlaneError::InvalidRequest(message) | ArtifactPlaneError::Conflict(message) => {
            McpError::invalid_params(message, None)
        }
        ArtifactPlaneError::Transport(message) => McpError::internal_error(message, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!ArtifactMcp::tool_router().list_all().is_empty());
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
        assert_eq!(SERVER_DOCS.server(), "artifact");
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
        assert_eq!(declaration.server().as_str(), "artifact");
        assert_eq!(declaration.contract_revision(), CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance()
                .iter()
                .find(|item| item.id.as_str() == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        for item in declaration.compliance() {
            if item.status == ComplianceStatus::Pending {
                assert!(item.note.is_some(), "pending items must state a reason");
            }
        }
    }

    #[test]
    fn contract_declaration_defers_runtime_surface_to_discover() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}
