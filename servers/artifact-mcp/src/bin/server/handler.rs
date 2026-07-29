use std::{
    num::NonZeroU64,
    sync::{Arc, LazyLock},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
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
    tool_handler, tool_router,
};
use serde::Serialize;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_artifact_mcp::{
    ARTIFACT_TEMPLATE, ArtifactGrantsOutput, ArtifactMetadataOutput, ArtifactMutationOutput,
    ArtifactReference, ArtifactShareOutput, CONTRACT_URI, CreateArtifactShareRequest, DOC_TEMPLATE,
    DOCS_URI, GRANTS_TEMPLATE, GrantArtifactRequest, INDEX_URI, METADATA_TEMPLATE,
    RevokeArtifactGrantRequest, RevokeArtifactShareRequest, SetArtifactReleaseRequest, doc_uri,
    parse_doc_uri, parse_grants_uri, parse_metadata_uri,
};
use veoveo_mcp_contract::tool;
use veoveo_mcp_contract::{
    AccessLevel, ArtifactId, ArtifactMetadata, ArtifactPlane, ArtifactPlaneError,
    CreateArtifactShareLinkRequest, ListArtifactsRequest, Page, PlaneCaller,
    docs::{CapabilityInventory, ContractDeclaration, ServerDocs},
    paginate, parse_artifact_plane_uri,
};

use super::{
    auth,
    prompts::ArtifactPrompt,
    subscriptions::{ArtifactSubscriptions, SubscriptionKind, visible_ids},
};

const LIST_PAGE_SIZE: usize = 100;

/// The crate documents embedded at build time and served under the well-known
/// surface: `artifact://docs`, `artifact://docs/{doc_id}`,
/// `artifact://contract`, and the administrative `admin/docs` routes
/// (contract C18-C21).
pub(super) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("artifact"));

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
            self.public_base_url, caller.identity.profile, artifact.artifact_id
        ));
        artifact
    }
}

#[derive(Clone)]
pub(super) struct ArtifactMcp {
    state: Arc<AppState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<ArtifactMcp>,
}

#[tool_router]
impl ArtifactMcp {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    /// The capability inventory declared at `artifact://contract` (contract
    /// C19). Tools and prompts derive from the live registrations; stable
    /// resources and templates come from the lists those surfaces are served
    /// from, so the declaration cannot silently diverge from the handler.
    pub(super) fn capability_inventory() -> CapabilityInventory {
        let mut tools: Vec<String> = Self::tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.into_owned())
            .collect();
        tools.sort();
        let mut resources: Vec<String> = well_known_resources()
            .into_iter()
            .map(|resource| resource.uri.clone())
            .collect();
        resources.push(INDEX_URI.to_owned());
        resources.sort();
        CapabilityInventory {
            tools,
            resources,
            resource_templates: resource_templates()
                .into_iter()
                .map(|template| template.uri_template.clone())
                .collect(),
            prompts: ArtifactPrompt::ALL
                .into_iter()
                .map(|prompt| prompt.prompt().name)
                .collect(),
            tasks: Vec::new(),
        }
    }

    #[tool(
        title = "Read artifact metadata",
        description = "Read policy-filtered metadata for one artifact occurrence.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ArtifactMetadataOutput>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn metadata(
        &self,
        Parameters(request): Parameters<ArtifactReference>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let caller = auth::caller(&context)?;
        let artifact = self
            .state
            .plane
            .head(&caller, &request.artifact_id)
            .await
            .map_err(plane_error)?;
        let artifact = self.state.expose_download(&caller, artifact);
        structured(
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
        let caller = auth::caller(&context)?;
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
        structured(
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
        let caller = auth::caller(&context)?;
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
        structured(
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
        let caller = auth::caller(&context)?;
        let artifact = self
            .state
            .plane
            .set_release_state(&caller, &request.artifact_id, request.release_state)
            .await
            .map_err(plane_error)?;
        let artifact = self.state.expose_download(&caller, artifact);
        structured(
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
                &auth::caller(&context)?,
                &request.artifact_id,
                CreateArtifactShareLinkRequest {
                    expires_at: request.options.expires_at,
                    max_downloads,
                },
            )
            .await
            .map_err(plane_error)?;
        structured(
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
                &auth::caller(&context)?,
                &request.artifact_id,
                &request.link_id,
            )
            .await
            .map_err(plane_error)?;
        structured(
            format!("revoked share link for artifact {}", request.artifact_id),
            &ArtifactMutationOutput {
                artifact_id: request.artifact_id,
                changed: true,
            },
        )
    }
}

#[tool_handler]
impl ServerHandler for ArtifactMcp {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        info.server_info = rmcp::model::Implementation::new("artifact", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Artifact discovery and sharing. Canonical artifact://{artifact_id} resources are immutable occurrence identities. Named user/group grants provide authorized sharing; expiring anyone-with-link bearers require an explicit releasable state."
                .to_owned(),
        );
        info
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = self.tool_router.list_all();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let page = static_page(tools, request.as_ref())?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let prompts: Vec<Prompt> = ArtifactPrompt::ALL
            .into_iter()
            .map(ArtifactPrompt::prompt)
            .collect();
        let page = static_page(prompts, request.as_ref())?;
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
        ArtifactPrompt::by_name(&request.name)
            .ok_or_else(|| {
                McpError::invalid_params(format!("unknown prompt '{}'", request.name), None)
            })?
            .render(request.arguments)
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let cursor = request
            .and_then(|request| request.cursor)
            .map(ArtifactId::parse)
            .transpose()
            .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
        let page = self
            .state
            .plane
            .list(
                &auth::caller(&context)?,
                ListArtifactsRequest {
                    cursor,
                    limit: Some(LIST_PAGE_SIZE as u16),
                },
            )
            .await
            .map_err(plane_error)?;
        // The well-known surface rides the first plane page; artifact pages
        // continue under the plane cursor (contract C18, C19).
        let mut resources = if cursor.is_none() {
            well_known_resources()
        } else {
            Vec::new()
        };
        resources.extend(page.artifacts.into_iter().map(|artifact| {
            Resource::new(
                artifact.artifact_uri,
                artifact
                    .filename
                    .clone()
                    .unwrap_or_else(|| artifact.artifact_id.to_string()),
            )
            .with_title(
                artifact
                    .filename
                    .unwrap_or_else(|| format!("Artifact {}", artifact.artifact_id)),
            )
            .with_mime_type(
                artifact
                    .mime_type
                    .unwrap_or_else(|| "application/octet-stream".to_owned()),
            )
        }));
        Ok(ListResourcesResult {
            resources,
            next_cursor: page.next_cursor.map(|cursor| cursor.to_string()),
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let page = static_page(resource_templates(), request.as_ref())?;
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
        let caller = auth::caller(&context)?;
        let uri = request.uri.as_str();
        // Well-known surface (contract C18, C19): readable by any
        // authenticated identity, like `list_resources`.
        if uri == DOCS_URI {
            return json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>());
        }
        if let Some(doc_id) = parse_doc_uri(uri) {
            let doc = SERVER_DOCS
                .doc(doc_id)
                .ok_or_else(|| McpError::resource_not_found("unknown server document", None))?;
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
            ]));
        }
        if uri == CONTRACT_URI {
            return json_resource(
                uri,
                &ContractDeclaration::from_docs(&SERVER_DOCS, Self::capability_inventory()),
            );
        }
        if uri == INDEX_URI {
            let mut page = self
                .state
                .plane
                .list(
                    &caller,
                    ListArtifactsRequest {
                        cursor: None,
                        limit: Some(100),
                    },
                )
                .await
                .map_err(plane_error)?;
            for artifact in &mut page.artifacts {
                *artifact = self.state.expose_download(&caller, artifact.clone());
            }
            return json_resource(uri, &page);
        }
        if let Some(artifact_id) = parse_metadata_uri(uri) {
            let metadata = self
                .state
                .plane
                .head(&caller, &artifact_id)
                .await
                .map_err(resource_error)?;
            let metadata = self.state.expose_download(&caller, metadata);
            return json_resource(uri, &metadata);
        }
        if let Some(artifact_id) = parse_grants_uri(uri) {
            let grants = self
                .state
                .plane
                .list_grants(&caller, &artifact_id)
                .await
                .map_err(resource_error)?;
            return json_resource(
                uri,
                &ArtifactGrantsOutput {
                    artifact_id,
                    grants,
                },
            );
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
            return Ok(ReadResourceResult::new(vec![contents]));
        }
        Err(McpError::resource_not_found(
            format!("unknown resource uri: {uri}"),
            None,
        ))
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let caller = auth::caller(&context)?;
        let uri = request.uri;
        let (kind, visible) = if uri == INDEX_URI {
            (
                SubscriptionKind::Index,
                Some(
                    visible_ids(&self.state.plane, &caller)
                        .await
                        .map_err(plane_error)?,
                ),
            )
        } else if let Some(id) = parse_metadata_uri(&uri) {
            self.state
                .plane
                .head(&caller, &id)
                .await
                .map_err(plane_error)?;
            (SubscriptionKind::Metadata(id), None)
        } else if let Some(id) = parse_grants_uri(&uri) {
            self.state
                .plane
                .list_grants(&caller, &id)
                .await
                .map_err(plane_error)?;
            (SubscriptionKind::Grants(id), None)
        } else if let Some(id) = parse_artifact_plane_uri(&uri) {
            self.state
                .plane
                .head(&caller, &id)
                .await
                .map_err(plane_error)?;
            (SubscriptionKind::Content(id), None)
        } else {
            return Err(McpError::invalid_params(
                "resource is not subscribable",
                None,
            ));
        };
        self.state
            .subscriptions
            .subscribe(uri, kind, caller, context.peer.clone(), visible)
            .await;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.state
            .subscriptions
            .unsubscribe(&request.uri, &auth::caller(&context)?)
            .await;
        Ok(())
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        if reference.uri.as_str() == DOC_TEMPLATE && request.argument.name == "doc_id" {
            let needle = request.argument.value.to_ascii_lowercase();
            let values: Vec<String> = SERVER_DOCS
                .iter()
                .map(|doc| doc.id.to_owned())
                .filter(|id| id.starts_with(&needle))
                .collect();
            let completion = CompletionInfo::new(values)
                .map_err(|error| McpError::internal_error(error, None))?;
            return Ok(CompleteResult::new(completion));
        }
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
                &auth::caller(&context)?,
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
            .map(|artifact| artifact.artifact_id.to_string())
            .filter(|id| id.starts_with(&needle))
            .take(CompletionInfo::MAX_VALUES)
            .collect();
        let completion = CompletionInfo::with_pagination(values, None, page.next_cursor.is_some())
            .map_err(|error| McpError::internal_error(error, None))?;
        Ok(CompleteResult::new(completion))
    }
}

/// Well-known surface resources (contract C18, C19). The first
/// `list_resources` page serves these for every authenticated identity and
/// `capability_inventory` declares them at `artifact://contract`, so the two
/// cannot diverge.
fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![
        Resource::new(DOCS_URI, "artifact-docs")
            .with_title("Server documents")
            .with_description("Index of the crate documents embedded at build time.")
            .with_mime_type("application/json"),
    ];
    for doc in SERVER_DOCS.iter() {
        resources.push(
            Resource::new(doc_uri(doc.id), doc.title)
                .with_title(doc.title)
                .with_description("Crate document embedded at build time.")
                .with_mime_type("text/markdown"),
        );
    }
    resources.push(
        Resource::new(CONTRACT_URI, "artifact-contract")
            .with_title("Contract declaration")
            .with_description(
                "Machine-readable contract revision, compliance, and capability inventory.",
            )
            .with_mime_type("application/json"),
    );
    resources
}

/// Templates served by `list_resource_templates` and declared in the
/// `artifact://contract` capability inventory.
fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        ResourceTemplate::new(DOC_TEMPLATE, "artifact-doc")
            .with_title("Server document")
            .with_description("Embedded crate document body (contract C18).")
            .with_mime_type("text/markdown"),
        ResourceTemplate::new(ARTIFACT_TEMPLATE, "artifact")
            .with_title("Artifact content")
            .with_description("Immutable artifact occurrence bytes.")
            .with_mime_type("application/octet-stream"),
        ResourceTemplate::new(METADATA_TEMPLATE, "artifact-metadata")
            .with_title("Artifact metadata")
            .with_description("Policy-filtered artifact metadata and download location.")
            .with_mime_type("application/json"),
        ResourceTemplate::new(GRANTS_TEMPLATE, "artifact-grants")
            .with_title("Artifact grants")
            .with_description("Administrative artifact access-control entries.")
            .with_mime_type("application/json"),
    ]
}

fn static_page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    paginate(items, request, LIST_PAGE_SIZE)
        .map_err(|error| McpError::invalid_params(error.to_string(), None))
}

fn structured<T: Serialize>(text: String, output: &T) -> Result<CallToolResult, McpError> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(
        serde_json::to_value(output)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?,
    );
    Ok(result)
}

fn json_resource<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResult, McpError> {
    let text = serde_json::to_string(value)
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(text, uri).with_mime_type("application/json"),
    ]))
}

fn resource_error(error: ArtifactPlaneError) -> McpError {
    match error {
        ArtifactPlaneError::NotFound | ArtifactPlaneError::Denied(_) => {
            McpError::resource_not_found("artifact is unavailable", None)
        }
        other => plane_error(other),
    }
}

fn plane_error(error: ArtifactPlaneError) -> McpError {
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
    use veoveo_artifact_mcp::{CONTRACT_URI, DOC_TEMPLATE, DOCS_URI, INDEX_URI, doc_uri};
    use veoveo_mcp_contract::docs::{
        CONTRACT_REVISION, ComplianceStatus, ContractDeclaration, DOC_ID_AGENTS, DOC_ID_DESIGN,
    };

    use super::{ArtifactMcp, SERVER_DOCS, resource_templates};

    #[test]
    fn embedded_documents_carry_the_crate_manual_and_design() {
        assert_eq!(SERVER_DOCS.server(), "artifact");
        let agents = SERVER_DOCS.doc(DOC_ID_AGENTS).expect("agents document");
        assert!(agents.body.contains("## Contract Compliance"));
        let design = SERVER_DOCS.doc(DOC_ID_DESIGN).expect("design document");
        assert!(!design.body.is_empty());
        let index = SERVER_DOCS.llms_txt();
        assert!(index.contains("(docs/agents)"));
        assert!(index.contains("(docs/design)"));
    }

    #[test]
    fn contract_declaration_resolves_from_the_embedded_manual() {
        let declaration =
            ContractDeclaration::from_docs(&SERVER_DOCS, ArtifactMcp::capability_inventory());
        assert_eq!(declaration.server, "artifact");
        assert_eq!(declaration.contract_revision, CONTRACT_REVISION);
        for id in ["C18", "C19", "C20", "C21"] {
            let item = declaration
                .compliance
                .iter()
                .find(|item| item.id == id)
                .expect("declared checklist item");
            assert_eq!(item.status, ComplianceStatus::Met, "{id} must be met");
        }
        for item in &declaration.compliance {
            if item.status == ComplianceStatus::Pending {
                assert!(item.note.is_some(), "pending items must state a reason");
            }
        }
    }

    #[test]
    fn capability_inventory_matches_the_registered_surface() {
        let inventory = ArtifactMcp::capability_inventory();
        for tool in ["metadata", "grant_access", "create_share_link"] {
            assert!(
                inventory.tools.iter().any(|name| name == tool),
                "inventory is missing tool {tool}"
            );
        }
        for uri in [DOCS_URI, CONTRACT_URI, INDEX_URI] {
            assert!(
                inventory.resources.contains(&uri.to_owned()),
                "inventory is missing resource {uri}"
            );
        }
        assert!(inventory.resources.contains(&doc_uri("agents")));
        assert!(
            inventory
                .resource_templates
                .contains(&DOC_TEMPLATE.to_owned())
        );
        assert_eq!(
            resource_templates().len(),
            inventory.resource_templates.len()
        );
        assert!(
            inventory
                .prompts
                .iter()
                .any(|name| name == "artifact-share-review")
        );
        assert!(inventory.tasks.is_empty());
    }
}
