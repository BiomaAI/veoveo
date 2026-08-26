use std::{
    collections::BTreeSet,
    num::NonZeroU64,
    sync::{Arc, LazyLock},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::tool;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CacheScope, CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo,
        ContentBlock, GetPromptRequestParams, GetPromptResponse, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, ListToolsResult, PaginatedRequestParams,
        Prompt, ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Reference,
        Resource, ResourceContents, ResourceTemplate, ServerCapabilities, ServerInfo,
        SubscriptionFilter,
    },
    service::{RequestContext, SubscriptionContext},
    tool_handler, tool_router,
};
use serde::Serialize;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_artifact_mcp::{
    ARTIFACT_TEMPLATE, ArtifactGrantsOutput, ArtifactMetadataOutput, ArtifactMutationOutput,
    ArtifactReference, ArtifactShareOutput, CONTRACT_URI, CreateArtifactShareRequest, DOC_TEMPLATE,
    DOCS_URI, GRANTS_TEMPLATE, GrantArtifactRequest, INDEX_URI, LIBRARY_APP_URI, METADATA_TEMPLATE,
    RevokeArtifactGrantRequest, RevokeArtifactShareRequest, SetArtifactReleaseRequest, doc_uri,
    parse_doc_uri, parse_grants_uri, parse_metadata_uri,
};
use veoveo_mcp_contract::{
    AccessLevel, ArtifactId, ArtifactMetadata, ArtifactPlane, ArtifactPlaneError,
    CreateArtifactShareLinkRequest, ListArtifactsRequest, Page, PlaneCaller, docs::ServerDocs,
    paginate, parse_artifact_plane_uri,
};

use super::{
    auth,
    prompts::ArtifactPrompt,
    subscriptions::{ArtifactSubscriptions, SubscriptionKind, visible_ids},
};

const LIST_PAGE_SIZE: usize = 100;
const LIBRARY_TOOLS: &[&str] = &[
    "create_share_link",
    "grant_access",
    "metadata",
    "revoke_access",
    "revoke_share_link",
    "set_release_state",
];

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
    fn supported_protocol_versions(
        &self,
    ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
        veoveo_mcp_contract::final_protocol_versions()
    }

    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .enable_completions()
            .build();
        veoveo_mcp_apps_extension::extend_capabilities(&mut capabilities);
        info.capabilities = capabilities;
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
        tools = tools
            .into_iter()
            .map(|tool| {
                if LIBRARY_TOOLS.contains(&tool.name.as_ref()) {
                    veoveo_mcp_apps_extension::link_tool_to_app(
                        tool,
                        LIBRARY_APP_URI,
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
        let page = static_page(tools, request.as_ref())?;
        let mut result = ListToolsResult::with_all_items(page.items)
            .with_ttl_ms(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS)
            .with_cache_scope(CacheScope::Private);
        result.next_cursor = page.next_cursor;
        Ok(result)
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
        let mut result = ListPromptsResult::with_all_items(page.items)
            .with_ttl_ms(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS)
            .with_cache_scope(CacheScope::Private);
        result.next_cursor = page.next_cursor;
        Ok(result)
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResponse, McpError> {
        ArtifactPrompt::by_name(&request.name)
            .ok_or_else(|| {
                McpError::invalid_params(format!("unknown prompt '{}'", request.name), None)
            })?
            .render(request.arguments)
            .map(Into::into)
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
        let mut result = ListResourcesResult::with_all_items(resources)
            .with_ttl_ms(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS)
            .with_cache_scope(CacheScope::Private);
        result.next_cursor = page.next_cursor.map(|cursor| cursor.to_string());
        Ok(result)
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let page = static_page(resource_templates(), request.as_ref())?;
        let mut result = ListResourceTemplatesResult::with_all_items(page.items)
            .with_ttl_ms(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS)
            .with_cache_scope(CacheScope::Private);
        result.next_cursor = page.next_cursor;
        Ok(result)
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, McpError> {
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
                .ok_or_else(|| McpError::invalid_params("unknown server document", None))?;
            return Ok(private_resource(vec![
                ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
            ])
            .into());
        }
        if uri == CONTRACT_URI {
            return json_resource(uri, SERVER_DOCS.contract_declaration());
        }
        if uri == LIBRARY_APP_URI {
            let html = veoveo_mcp_apps_extension::workbench_app_html(
                &veoveo_mcp_apps_extension::WorkbenchApp {
                    app_id: "artifact-library",
                    title: "Library",
                    subtitle: "Discover, inspect, release, and share governed artifacts",
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
                },
            );
            return Ok(
                private_resource(vec![veoveo_mcp_apps_extension::app_html_contents(
                    uri, &html,
                )])
                .into(),
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
            return Ok(private_resource(vec![contents]).into());
        }
        Err(McpError::invalid_params(
            format!("unknown resource uri: {uri}"),
            None,
        ))
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        veoveo_mcp_contract::accepted_subscription_filter(requested)
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        let caller = auth::caller(context.request_context())?;
        let accepted = context.accepted().clone();
        let mut subscriptions = Vec::new();
        for uri in accepted.resource_subscriptions.iter().flatten() {
            let kind = if uri == INDEX_URI {
                SubscriptionKind::Index
            } else if let Some(id) = parse_metadata_uri(uri) {
                self.state
                    .plane
                    .head(&caller, &id)
                    .await
                    .map_err(plane_error)?;
                SubscriptionKind::Metadata(id)
            } else if let Some(id) = parse_grants_uri(uri) {
                self.state
                    .plane
                    .list_grants(&caller, &id)
                    .await
                    .map_err(plane_error)?;
                SubscriptionKind::Grants(id)
            } else if let Some(id) = parse_artifact_plane_uri(uri) {
                self.state
                    .plane
                    .head(&caller, &id)
                    .await
                    .map_err(plane_error)?;
                SubscriptionKind::Content(id)
            } else {
                return Err(McpError::invalid_params(
                    "resource is not subscribable",
                    None,
                ));
            };
            subscriptions.push((uri.clone(), kind));
        }
        let tracks_list = accepted.resources_list_changed == Some(true)
            || subscriptions
                .iter()
                .any(|(_, kind)| *kind == SubscriptionKind::Index);
        let mut visible = if tracks_list {
            visible_ids(&self.state.plane, &caller)
                .await
                .map_err(plane_error)?
        } else {
            BTreeSet::new()
        };
        let mut updates = self.state.subscriptions.listen();
        loop {
            let artifact_id = tokio::select! {
                () = context.cancelled() => return Ok(()),
                update = updates.recv() => match update {
                    Ok(artifact_id) => artifact_id,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "artifact subscription updates lagged");
                        if accepted.resources_list_changed == Some(true) {
                            context.sink().notify_resource_list_changed().await.map_err(subscription_error)?;
                        }
                        continue;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return Ok(()),
                }
            };
            let current = if tracks_list {
                Some(
                    visible_ids(&self.state.plane, &caller)
                        .await
                        .map_err(plane_error)?,
                )
            } else {
                None
            };
            let list_changed = current.as_ref().is_some_and(|current| current != &visible);
            if let Some(current) = current {
                visible = current;
            }
            if list_changed && accepted.resources_list_changed == Some(true) {
                context
                    .sink()
                    .notify_resource_list_changed()
                    .await
                    .map_err(subscription_error)?;
            }
            for (uri, kind) in &subscriptions {
                let notify = match kind {
                    SubscriptionKind::Index => list_changed || visible.contains(&artifact_id),
                    SubscriptionKind::Content(id) | SubscriptionKind::Metadata(id)
                        if *id == artifact_id =>
                    {
                        self.state.plane.head(&caller, id).await.is_ok()
                    }
                    SubscriptionKind::Grants(id) if *id == artifact_id => {
                        self.state.plane.list_grants(&caller, id).await.is_ok()
                    }
                    _ => false,
                };
                if notify {
                    context
                        .sink()
                        .notify_resource_updated(uri.clone())
                        .await
                        .map_err(subscription_error)?;
                }
            }
        }
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
/// `list_resources` page serves these for every authenticated identity.
fn well_known_resources() -> Vec<Resource> {
    let mut resources = vec![
        veoveo_mcp_apps_extension::app_resource(LIBRARY_APP_URI, "library")
            .with_title("Library")
            .with_description("Governed artifact discovery, inspection, release, and sharing."),
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

/// Templates served by `list_resource_templates`.
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

fn private_resource(contents: Vec<ResourceContents>) -> ReadResourceResult {
    ReadResourceResult::new(contents)
        .with_ttl_ms(veoveo_mcp_contract::PRIVATE_RESOURCE_TTL_MS)
        .with_cache_scope(CacheScope::Private)
}

fn json_resource<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResponse, McpError> {
    let text = serde_json::to_string(value)
        .map_err(|error| McpError::internal_error(error.to_string(), None))?;
    Ok(private_resource(vec![
        ResourceContents::text(text, uri).with_mime_type("application/json"),
    ])
    .into())
}

fn resource_error(error: ArtifactPlaneError) -> McpError {
    match error {
        ArtifactPlaneError::NotFound | ArtifactPlaneError::Denied(_) => {
            McpError::invalid_params("artifact is unavailable", None)
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

fn subscription_error(error: rmcp::service::SubscriptionSendError) -> McpError {
    McpError::internal_error(error.to_string(), None)
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
    fn contract_declaration_defers_runtime_surface_to_discover() {
        let declaration = veoveo_mcp_contract::docs::ContractDeclaration::from_docs(&SERVER_DOCS);
        let json = serde_json::to_value(declaration).unwrap();
        assert!(json.get("capabilities").is_none());
    }
}
