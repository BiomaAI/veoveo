//! Frames MCP server.
//!
//! MCP surface:
//!   tools for complete world publication and bounded coordinate conversion
//!   task-required `batch_transform`
//!   templates for frames, CRS metadata, operations, artifacts, and usage

use std::{
    collections::BTreeSet,
    net::SocketAddr,
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
    time::Duration,
};

use axum::{Router, middleware, routing::get};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{DateTime, TimeDelta, Utc};
use clap::Parser;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock,
        GetPromptRequestParams, GetPromptResult, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParams, Prompt,
        ReadResourceRequestParams, ReadResourceResult, Reference, Resource, ResourceContents,
        ResourceTemplate, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_frames_mcp::{
    artifacts::ArtifactRepository,
    contract::{
        BatchTransformOutput, BatchTransformRequest, ConvertFrameOutput, ConvertFrameRequest,
        CreateWorldOutput, CreateWorldRequest, PublishWorldOutput, PublishWorldRequest,
    },
    engine,
    state::{FrameScope, FramesState},
    uris,
};
use veoveo_mcp_contract::tool;
use veoveo_mcp_contract::{
    CoordinateOperationId, CoordinateSpace, GATEWAY_INTERNAL_TOKEN_ISSUER,
    GatewayInternalTokenVerifier, GatewayInternalTrustBundle, IssueArtifactWriteCapabilityRequest,
    IssuedArtifactWriteCapability, Page, ServerSlug, TelemetryGuard, TokenIssuer, UsageReport,
    WorldFrameUri, init_server_telemetry, paginate, public_allowed_hosts,
};
use veoveo_mcp_task_extension::{
    Implementation as TaskExtensionImplementation, ServerDiscovery, TaskExtensionAdapter,
    task_extension_middleware,
};
use veoveo_task_runtime::{
    CreateTask as DurableCreateTask, RecoveryClass, TaskError, TaskFailure, TaskId,
    TaskRetentionPin, TaskRuntime, TaskRuntimeConfig, TaskSnapshot, TaskTransition,
};

#[path = "server/app_state.rs"]
mod app_state;
#[path = "server/config.rs"]
mod config;
#[path = "server/host.rs"]
mod host;
#[path = "server/internal_auth.rs"]
mod internal_auth;
#[path = "server/outputs.rs"]
mod outputs;
#[path = "server/ownership.rs"]
mod ownership;
#[path = "server/prompts.rs"]
mod prompts;
#[path = "server/task_extension.rs"]
mod task_extension;

use app_state::{AppState, update_task};
use config::Cli;
use host::validate_host;
use internal_auth::{InternalMcpAuthState, authenticate_internal_mcp};
use outputs::usage_record;
use ownership::{
    frame_scope_from_identity, frame_scope_from_runtime, internal_caller, internal_identity,
    optional_task_owner, require_task_owner, runtime_owner, task_owner_allows,
};
use prompts::FramesPrompt;
use task_extension::FramesTaskExtension;

const MCP_TASK_POLL_INTERVAL_MS: u64 = 3_000;
const MCP_TASK_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1_000;
const TASK_LEASE_DURATION: Duration = Duration::from_secs(120);
const TASK_LEASE_HEARTBEAT: Duration = Duration::from_secs(40);
const ARTIFACT_CAPABILITY_TTL: TimeDelta = TimeDelta::hours(24);
const SERVER_SLUG: &str = "frames";
const LIST_PAGE_SIZE: usize = 100;
const BATCH_ARTIFACT_MIME: &str = "application/json";

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[derive(Clone)]
struct FramesMcp {
    state: Arc<AppState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<FramesMcp>,
}

#[tool_router]
impl FramesMcp {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Convert coordinate frame",
        description = "Convert WGS84, ECEF, ENU, and NED coordinates using registered frame definitions.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ConvertFrameOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn convert_frame(
        &self,
        Parameters(args): Parameters<ConvertFrameRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = internal_identity(&context)?;
        let scope = frame_scope_from_identity(&self.state, &identity).await?;
        let worlds = resolve_worlds(&self.state, &scope, &args).await?;
        let output = engine::convert_frame(args, &worlds).map_err(invalid_params)?;
        record_direct_operation(&self.state, &scope, &output.provenance).await?;
        structured_result(
            format!("converted {} point(s)", output.points.len()),
            &output,
        )
    }

    #[tool(
        title = "Create frame world",
        description = "Create an empty authored frame world. Publish its complete rooted tree separately.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CreateWorldOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn create_world(
        &self,
        Parameters(args): Parameters<CreateWorldRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = internal_identity(&context)?;
        let scope = frame_scope_from_identity(&self.state, &identity).await?;
        let world = self
            .state
            .frames
            .create_world(&scope, args)
            .await
            .map_err(invalid_params)?;
        let output = CreateWorldOutput { world };
        structured_result(
            format!("created frame world {}", output.world.world_id),
            &output,
        )
    }

    #[tool(
        title = "Publish frame world",
        description = "Validate and atomically publish a complete rooted frame tree as a new immutable world revision.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<PublishWorldOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn publish_world(
        &self,
        Parameters(args): Parameters<PublishWorldRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let identity = internal_identity(&context)?;
        let scope = frame_scope_from_identity(&self.state, &identity).await?;
        let output = self
            .state
            .frames
            .publish_world(&scope, args)
            .await
            .map_err(invalid_params)?;
        let message = if output.created {
            format!(
                "published frame world revision {}",
                output.revision.revision_uri
            )
        } else {
            format!("frame world already at {}", output.revision.revision_uri)
        };
        structured_result(message, &output)
    }

    #[tool(
        title = "Batch transform",
        description = "Run a batch frame conversion as an MCP task and optionally store the JSON output through the shared artifact plane.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<BatchTransformOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn batch_transform(
        &self,
        Parameters(_args): Parameters<BatchTransformRequest>,
    ) -> Result<CallToolResult, McpError> {
        Err(McpError::invalid_request(
            "batch_transform requires task-based invocation",
            None,
        ))
    }
}

fn structured_result<T: Serialize>(text: String, value: &T) -> Result<CallToolResult, McpError> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(
        serde_json::to_value(value)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?,
    );
    Ok(result)
}

fn invalid_params(err: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(err.to_string(), None)
}

async fn record_direct_operation(
    state: &AppState,
    scope: &FrameScope,
    provenance: &veoveo_mcp_contract::CoordinateOperationProvenance,
) -> Result<(), McpError> {
    state
        .frames
        .record_operation(scope, None, provenance)
        .await
        .map_err(|error| McpError::internal_error(error.to_string(), None))
}

async fn resolve_worlds(
    state: &AppState,
    scope: &FrameScope,
    request: &ConvertFrameRequest,
) -> Result<engine::ResolvedWorlds, McpError> {
    let mut frame_uris = BTreeSet::new();
    for point in &request.points {
        if let veoveo_frames_mcp::contract::CoordinatePoint::WorldFrame(point) = point {
            frame_uris.insert(point.frame_uri.clone());
        }
    }
    if let CoordinateSpace::WorldFrame { frame_uri } = &request.target {
        frame_uris.insert(frame_uri.clone());
    }
    let mut revisions = BTreeSet::new();
    for frame_uri in frame_uris {
        revisions.insert(frame_uri.revision_uri());
    }
    let mut resolved = engine::ResolvedWorlds::default();
    for revision_uri in revisions {
        let revision = state
            .frames
            .require_revision(scope, &revision_uri)
            .await
            .map_err(invalid_params)?;
        resolved.insert(revision).map_err(invalid_params)?;
    }
    Ok(resolved)
}

fn mcp_page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    paginate(items, request, LIST_PAGE_SIZE)
        .map_err(|err| McpError::invalid_params(err.to_string(), None))
}

#[tool_handler]
impl ServerHandler for FramesMcp {
    fn get_info(&self) -> ServerInfo {
        let caps: ServerCapabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_completions()
            .build();
        let mut info = ServerInfo::default();
        info.capabilities = caps;
        info.server_info = rmcp::model::Implementation::new("frames", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Frames world-graph server. Create worlds at runtime, publish complete rooted frame \
             trees as immutable revisions, and pin revision-scoped frame URIs in sessions and \
             recordings. Use direct tools for bounded transforms; use \
             batch_transform as an MCP task for bulk conversion and artifact output."
                .into(),
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
        let identity = internal_identity(&context)?;
        let scope = frame_scope_from_identity(&self.state, &identity).await?;
        let mut resources = vec![
            Resource::new(uris::WORLDS_URI, "worlds")
                .with_title("Frame worlds")
                .with_description("Visible authored frame worlds and their current revisions.")
                .with_mime_type("application/json"),
            Resource::new(uris::USAGE_ROOT_URI, "usage")
                .with_title("Frames usage ledger")
                .with_description("Index of task usage resources.")
                .with_mime_type("application/json"),
        ];
        for world in self
            .state
            .frames
            .list_worlds(&scope)
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?
        {
            resources.push(
                Resource::new(
                    world.world_uri.to_string(),
                    format!("world {}", world.world_id),
                )
                .with_description(
                    world
                        .description
                        .unwrap_or_else(|| "Coordinate frame.".into()),
                )
                .with_mime_type("application/json"),
            );
            if let Some(revision) = self
                .state
                .frames
                .get_head_revision(&scope, &world.world_id)
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?
            {
                resources.push(
                    Resource::new(
                        revision.revision_uri.to_string(),
                        format!("world {} revision {}", revision.world_id, revision.revision),
                    )
                    .with_description("Immutable complete frame-world tree.")
                    .with_mime_type("application/json"),
                );
                for frame in &revision.tree.frames {
                    let frame_uri = WorldFrameUri::new(&revision.revision_uri, &frame.frame_id);
                    resources.push(
                        Resource::new(frame_uri.to_string(), format!("frame {}", frame.frame_id))
                            .with_description(
                                frame
                                    .description
                                    .clone()
                                    .unwrap_or_else(|| "World frame.".into()),
                            )
                            .with_mime_type("application/json"),
                    );
                }
            }
        }
        for task_id in self
            .state
            .tasks
            .platform_store()
            .domain_usage_task_ids(SERVER_SLUG)
            .await
            .map_err(|error| McpError::internal_error(error.to_string(), None))?
        {
            let task_id = task_id.to_string();
            let Some(owner) = optional_task_owner(&self.state, &task_id).await? else {
                continue;
            };
            if !task_owner_allows(&owner, &identity) {
                continue;
            }
            resources.push(
                Resource::new(
                    uris::usage_task_uri(&task_id),
                    format!("usage for task {task_id}"),
                )
                .with_description("Usage rows for one Frames task.")
                .with_mime_type("application/json"),
            );
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
            ResourceTemplate::new(uris::WORLD_TEMPLATE, "world")
                .with_title("Frame world")
                .with_description("Mutable world head and authoring metadata.")
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::WORLD_REVISION_TEMPLATE, "world revision")
                .with_title("Frame world revision")
                .with_description("Immutable complete rooted frame tree.")
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::WORLD_FRAME_TEMPLATE, "world frame")
                .with_title("Revision-scoped world frame")
                .with_description("Typed frame node inside one immutable world revision.")
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::OPERATION_TEMPLATE, "operation")
                .with_title("Coordinate operation")
                .with_description("Recorded operation provenance.")
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::ARTIFACT_TEMPLATE, "artifact")
                .with_title("Frames artifact")
                .with_description("Shared-plane immutable Frames artifact.")
                .with_mime_type(BATCH_ARTIFACT_MIME),
            ResourceTemplate::new(uris::USAGE_TASK_TEMPLATE, "usage")
                .with_title("Frames task usage")
                .with_description("Usage rows for one Frames task.")
                .with_mime_type("application/json"),
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
        let uri = request.uri.as_str();
        let identity = internal_identity(&context)?;
        let scope = frame_scope_from_identity(&self.state, &identity).await?;
        if uri == uris::WORLDS_URI {
            let worlds = self
                .state
                .frames
                .list_worlds(&scope)
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?;
            return json_resource(uri, &worlds);
        }
        if uri == uris::USAGE_ROOT_URI {
            let mut entries = Vec::new();
            for task_id in self
                .state
                .tasks
                .platform_store()
                .domain_usage_task_ids(SERVER_SLUG)
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?
            {
                let task_id = task_id.to_string();
                let Some(owner) = optional_task_owner(&self.state, &task_id).await? else {
                    continue;
                };
                if task_owner_allows(&owner, &identity) {
                    entries.push(json!({
                        "task_id": task_id,
                        "usage_uri": uris::usage_task_uri(&task_id),
                    }));
                }
            }
            return json_resource(uri, &entries);
        }
        if let Some(frame_uri) = uris::parse_world_frame_uri(uri) {
            let revision = self
                .state
                .frames
                .get_revision(&scope, &frame_uri.revision_uri())
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(format!("unknown world frame `{frame_uri}`"), None)
                })?;
            let frame = revision.frame(&frame_uri).ok_or_else(|| {
                McpError::resource_not_found(format!("unknown world frame `{frame_uri}`"), None)
            })?;
            return json_resource(uri, &frame);
        }
        if let Some(revision_uri) = uris::parse_world_revision_uri(uri) {
            let revision = self
                .state
                .frames
                .get_revision(&scope, &revision_uri)
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(
                        format!("unknown frame world revision `{revision_uri}`"),
                        None,
                    )
                })?;
            return json_resource(uri, &revision);
        }
        if let Some(world_uri) = uris::parse_world_uri(uri) {
            let world_id = world_uri.world_id();
            let world = self
                .state
                .frames
                .get_world(&scope, &world_id)
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(format!("unknown frame world `{world_id}`"), None)
                })?;
            return json_resource(uri, &world);
        }
        if let Some(operation_id) = uris::parse_operation_uri(uri) {
            let operation_id = CoordinateOperationId::new(operation_id)
                .map_err(|err| McpError::invalid_params(err.to_string(), None))?;
            let operation = self
                .state
                .frames
                .get_operation(&scope, &operation_id)
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(
                        format!("unknown operation `{operation_id}`"),
                        None,
                    )
                })?;
            return json_resource(uri, &operation);
        }
        if let Some(task_id) = uris::parse_usage_task_uri(uri) {
            require_task_owner(&self.state, &context, task_id).await?;
            let task_uuid = task_id
                .parse::<veoveo_platform_store::TaskId>()
                .map_err(|error| McpError::invalid_params(error.to_string(), None))?;
            let records = self
                .state
                .tasks
                .platform_store()
                .domain_usage_for_task(SERVER_SLUG, task_uuid)
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?;
            let report: UsageReport = UsageReport::new(task_id, uris::usage_task_uri(task_id))
                .with_records(
                    records
                        .into_iter()
                        .map(|record| usage_record(task_id, record))
                        .collect(),
                );
            if report.records.is_empty() {
                return Err(McpError::resource_not_found(
                    format!("unknown usage task `{task_id}`"),
                    None,
                ));
            }
            return json_resource(uri, &report);
        }
        if let Some(artifact_id) = uris::parse_artifact_uri(uri) {
            let caller = internal_caller(&context)?;
            let artifact = self
                .state
                .artifacts
                .get(&caller, &artifact_id)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(format!("unknown artifact `{artifact_id}`"), None)
                })?;
            let blob = BASE64_STANDARD.encode(&artifact.bytes);
            let mut content = ResourceContents::blob(blob, uri);
            content = content.with_mime_type(
                artifact
                    .metadata
                    .mime_type
                    .unwrap_or_else(|| BATCH_ARTIFACT_MIME.to_string()),
            );
            return Ok(ReadResourceResult::new(vec![content]));
        }
        Err(McpError::resource_not_found(
            format!("unknown resource uri: {uri}"),
            None,
        ))
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let prompts: Vec<Prompt> = FramesPrompt::ALL
            .into_iter()
            .map(FramesPrompt::prompt)
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
        let prompt = FramesPrompt::by_name(&request.name).ok_or_else(|| {
            McpError::invalid_params(format!("unknown prompt `{}`", request.name), None)
        })?;
        prompt.render(request.arguments)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(res_ref) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let values = if res_ref.uri == uris::WORLD_TEMPLATE && request.argument.name == "world_id" {
            let needle = request.argument.value.to_lowercase();
            let identity = internal_identity(&context)?;
            let scope = frame_scope_from_identity(&self.state, &identity).await?;
            self.state
                .frames
                .list_worlds(&scope)
                .await
                .map_err(|error| McpError::internal_error(error.to_string(), None))?
                .into_iter()
                .map(|world| world.world_id.to_string())
                .filter(|world| world.to_lowercase().contains(&needle))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let total = values.len() as u32;
        let values = values
            .into_iter()
            .take(CompletionInfo::MAX_VALUES)
            .collect::<Vec<_>>();
        let has_more = (values.len() as u32) < total;
        let completion = CompletionInfo::with_pagination(values, Some(total), has_more)
            .map_err(|err| McpError::internal_error(err.to_string(), None))?;
        Ok(CompleteResult::new(completion))
    }
}

fn json_resource<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResult, McpError> {
    let text = serde_json::to_string(value)
        .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(text, uri).with_mime_type("application/json"),
    ]))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BatchTaskRequest {
    args: BatchTransformRequest,
    operation_id: CoordinateOperationId,
    operation_created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    artifact_write_capability: Option<IssuedArtifactWriteCapability>,
}

fn stamp_batch_provenance(
    result: &mut ConvertFrameOutput,
    operation_id: CoordinateOperationId,
    created_at: DateTime<Utc>,
) {
    result.provenance.operation.operation_id = operation_id;
    result.provenance.operation.operation_uri =
        uris::operation_uri(result.provenance.operation.operation_id.as_str());
    result.provenance.operation.created_at = created_at;
}

async fn start_batch_task(
    state: Arc<AppState>,
    identity: veoveo_mcp_contract::GatewayInternalIdentity,
    caller: veoveo_mcp_contract::PlaneCaller,
    args: BatchTransformRequest,
    retention_pins: BTreeSet<TaskRetentionPin>,
) -> Result<TaskSnapshot, String> {
    let task_id = TaskId::new();
    let artifact_write_capability = if args.artifact {
        Some(
            state
                .artifacts
                .issue_write_capability(
                    &caller,
                    &IssueArtifactWriteCapabilityRequest {
                        task_id: task_id.to_string(),
                        expires_at: Utc::now() + ARTIFACT_CAPABILITY_TTL,
                        max_artifact_count: NonZeroU32::new(1).expect("one artifact is non-zero"),
                        max_total_bytes: NonZeroU64::new(state.max_artifact_bytes)
                            .ok_or_else(|| "max artifact bytes must be non-zero".to_owned())?,
                    },
                )
                .await
                .map_err(|error| error.to_string())?,
        )
    } else {
        None
    };
    let request = BatchTaskRequest {
        args,
        operation_id: CoordinateOperationId::new(format!("op-{}", uuid::Uuid::now_v7()))
            .map_err(|error| error.to_string())?,
        operation_created_at: Utc::now(),
        artifact_write_capability,
    };
    let created = state
        .tasks
        .create(DurableCreateTask {
            task_id,
            owner: runtime_owner(&identity),
            server: SERVER_SLUG.to_owned(),
            task_type: "batch_transform".to_owned(),
            request: serde_json::to_value(&request).map_err(|error| error.to_string())?,
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(MCP_TASK_TTL_MS),
            poll_interval_ms: Some(MCP_TASK_POLL_INTERVAL_MS),
            retention_pins,
        })
        .await
        .map_err(|error| error.to_string())?;
    schedule_batch_task(state, created.snapshot, request)
        .await
        .map_err(|error| error.to_string())
}

async fn schedule_batch_task(
    state: Arc<AppState>,
    snapshot: TaskSnapshot,
    request: BatchTaskRequest,
) -> anyhow::Result<TaskSnapshot> {
    let task_id = snapshot.task_id.to_string();
    let claimed = state.tasks.claim(&task_id, TASK_LEASE_DURATION).await?;
    let owner = snapshot.owner.clone();
    let cancellation = CancellationToken::new();
    let join = tokio::spawn(run_task(
        state.clone(),
        task_id.clone(),
        owner,
        request,
        cancellation.clone(),
    ));
    state
        .tasks
        .register_worker(&task_id, cancellation, join)
        .await?;
    Ok(claimed.snapshot)
}

async fn resume_batch_task(state: Arc<AppState>, snapshot: TaskSnapshot) -> anyhow::Result<()> {
    let request: BatchTaskRequest = serde_json::from_value(snapshot.request.clone())?;
    schedule_batch_task(state, snapshot, request)
        .await
        .map(|_| ())
}

async fn complete_tool_error(state: &AppState, task_id: &str, message: String) {
    let result = CallToolResult::error(vec![ContentBlock::text(message.clone())]);
    let transition = match serde_json::to_value(result) {
        Ok(result) => TaskTransition::Succeeded { message, result },
        Err(error) => TaskTransition::Failed(TaskFailure::new(
            "result_serialization_failed",
            error.to_string(),
        )),
    };
    update_task(state, task_id, transition).await;
}

async fn run_task(
    state: Arc<AppState>,
    task_id: String,
    owner: veoveo_task_runtime::TaskOwner,
    request: BatchTaskRequest,
    cancellation: CancellationToken,
) {
    let work = run_task_inner(
        state.clone(),
        task_id.clone(),
        owner,
        request,
        cancellation.clone(),
    );
    tokio::pin!(work);
    let mut heartbeat = tokio::time::interval(TASK_LEASE_HEARTBEAT);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await;
    loop {
        tokio::select! {
            () = &mut work => break,
            _ = heartbeat.tick() => {
                if let Err(error) = state.tasks.renew_lease(&task_id, TASK_LEASE_DURATION).await {
                    tracing::warn!(task_id, "task lease heartbeat failed: {error}");
                    cancellation.cancel();
                    break;
                }
            }
        }
    }
}

async fn run_task_inner(
    state: Arc<AppState>,
    task_id: String,
    owner: veoveo_task_runtime::TaskOwner,
    request: BatchTaskRequest,
    cancellation: CancellationToken,
) {
    macro_rules! fail {
        ($msg:expr) => {{
            let msg: String = $msg;
            tracing::warn!(task_id, "Frames task failed: {msg}");
            complete_tool_error(&state, &task_id, msg).await;
            return;
        }};
    }
    update_task(
        &state,
        &task_id,
        TaskTransition::Running {
            message: "running batch coordinate transform".to_owned(),
            progress: 0.1,
        },
    )
    .await;
    let scope = match frame_scope_from_runtime(&state, &owner).await {
        Ok(scope) => scope,
        Err(error) => fail!(format!("coordinate identity failed: {error}")),
    };
    let worlds = match resolve_worlds(&state, &scope, &request.args.convert).await {
        Ok(worlds) => worlds,
        Err(error) => fail!(format!("world resolution failed: {error}")),
    };
    let convert_args = request.args.convert.clone();
    let mut converted =
        match tokio::task::spawn_blocking(move || engine::convert_frame(convert_args, &worlds))
            .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => fail!(format!("batch transform failed: {error}")),
            Err(error) => fail!(format!("batch worker failed: {error}")),
        };
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    stamp_batch_provenance(
        &mut converted,
        request.operation_id,
        request.operation_created_at,
    );
    let platform_task_id = match task_id.parse::<veoveo_platform_store::TaskId>() {
        Ok(task_id) => task_id,
        Err(error) => fail!(format!("invalid durable task id: {error}")),
    };
    if let Err(error) = state
        .frames
        .record_operation(&scope, Some(platform_task_id), &converted.provenance)
        .await
    {
        fail!(format!("operation provenance write failed: {error}"));
    }
    let output = BatchTransformOutput {
        result: converted,
        artifact: None,
    };
    let result = match outputs::batch_result(
        &state,
        request.artifact_write_capability.as_ref(),
        &task_id,
        &owner,
        output,
        request.args.artifact,
    )
    .await
    {
        Ok(result) => result,
        Err(error) => fail!(format!("batch output failed: {error}")),
    };
    if cancellation.is_cancelled() {
        update_task(&state, &task_id, TaskTransition::Cancelled).await;
        return;
    }
    let payload = match serde_json::to_value(&result) {
        Ok(payload) => payload,
        Err(error) => fail!(format!("serializing batch result failed: {error}")),
    };
    update_task(
        &state,
        &task_id,
        TaskTransition::Succeeded {
            message: "batch coordinate transform completed".to_owned(),
            result: payload,
        },
    )
    .await;
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard =
        init_server_telemetry("veoveo-frames-mcp", "info,veoveo_frames_mcp=debug")?;
    let args = match Cli::parse() {
        Cli::Serve(args) => *args,
    };
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let internal_token_verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?,
    );
    let tasks = TaskRuntime::connect(
        TaskRuntimeConfig::new(
            args.surreal_endpoint.clone(),
            args.surreal_namespace.clone(),
            args.surreal_database.clone(),
            args.surreal_auth_level,
            args.surreal_username.clone(),
            args.surreal_password.clone(),
        ),
        SERVER_SLUG,
        format!("{SERVER_SLUG}-{}", uuid::Uuid::now_v7()),
    )
    .await?;
    let recovery = tasks.recover().await?;
    let frames = FramesState::new(tasks.platform_store().clone());
    let state = Arc::new(AppState {
        tasks,
        frames,
        artifacts: ArtifactRepository::new(args.artifact_service_url.clone()),
        max_artifact_bytes: args.max_artifact_bytes,
    });
    for snapshot in recovery.resumable {
        if let Err(error) = resume_batch_task(state.clone(), snapshot).await {
            match error.downcast_ref::<TaskError>() {
                Some(TaskError::LeaseHeld(task_id) | TaskError::Conflict(task_id)) => {
                    tracing::info!(task_id, "another replica claimed recovered Frames task");
                }
                _ => return Err(error),
            }
        }
    }

    let ct = tokio_util::sync::CancellationToken::new();
    let mut allowed_hosts = public_allowed_hosts(&public_deployment, args.allow_loopback_hosts);
    allowed_hosts.extend(args.allowed_hosts.iter().cloned());
    let allowed_hosts = Arc::new(allowed_hosts);
    let internal_auth_state = InternalMcpAuthState {
        verifier: internal_token_verifier,
    };
    let mcp_service = StreamableHttpService::new(
        {
            let state = state.clone();
            move || Ok(FramesMcp::new(state.clone()))
        },
        LocalSessionManager::default().into(),
        veoveo_mcp_contract::canonical_streamable_http_server_config()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(ct.child_token()),
    );
    let task_extension = Arc::new(TaskExtensionAdapter::new(
        Arc::new(FramesTaskExtension::new(state.clone())),
        ServerDiscovery::new(
            std::collections::BTreeMap::from([
                ("tools".to_owned(), json!({})),
                ("resources".to_owned(), json!({})),
                ("prompts".to_owned(), json!({})),
            ]),
            TaskExtensionImplementation {
                name: "frames".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            Some(
                "Durable coordinate frames, provenance, batch transforms, and shared artifacts."
                    .to_owned(),
            ),
        ),
    ));
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            task_extension,
            task_extension_middleware::<FramesTaskExtension>,
        ))
        .layer(middleware::from_fn_with_state(
            internal_auth_state,
            authenticate_internal_mcp,
        ));
    let server_router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .with_state(state.clone())
        .nest("/mcp", mcp_router);
    let router = Router::new()
        .nest(public_endpoint.mount_path(), server_router)
        .layer(middleware::from_fn_with_state(
            allowed_hosts.clone(),
            validate_host,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(tracing::Level::INFO)),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    tracing::info!(
        service = "veoveo-frames-mcp",
        address = %addr,
        mcp_path = public_endpoint.path("mcp"),
        public_url = public_endpoint.public_url(),
        "listening"
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            ct.cancel();
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod task_tests {
    use super::*;

    #[test]
    fn tool_input_schemas_use_the_canonical_profile() {
        assert!(!FramesMcp::tool_router().list_all().is_empty());
    }
    #[test]
    fn resumed_batch_output_is_byte_deterministic() {
        let request = ConvertFrameRequest {
            target: CoordinateSpace::EcefWgs84,
            points: vec![veoveo_frames_mcp::contract::CoordinatePoint::Wgs84(
                veoveo_mcp_contract::Wgs84Position {
                    latitude_degrees: 37.421_999_9,
                    longitude_degrees: -122.084_057_5,
                    ellipsoid_height_m: 10.0,
                },
            )],
            allow_approximation: false,
        };
        let mut first =
            engine::convert_frame(request.clone(), &engine::ResolvedWorlds::default()).unwrap();
        let mut replay =
            engine::convert_frame(request, &engine::ResolvedWorlds::default()).unwrap();
        assert_ne!(
            first.provenance.operation.operation_id,
            replay.provenance.operation.operation_id
        );

        let operation_id =
            CoordinateOperationId::new(format!("op-{}", uuid::Uuid::now_v7())).unwrap();
        let created_at = Utc::now();
        stamp_batch_provenance(&mut first, operation_id.clone(), created_at);
        stamp_batch_provenance(&mut replay, operation_id, created_at);
        let first = BatchTransformOutput {
            result: first,
            artifact: None,
        };
        let replay = BatchTransformOutput {
            result: replay,
            artifact: None,
        };
        assert_eq!(
            serde_json::to_vec_pretty(&first).unwrap(),
            serde_json::to_vec_pretty(&replay).unwrap()
        );
    }
}
