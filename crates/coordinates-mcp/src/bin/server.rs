//! Coordinates MCP server.
//!
//! MCP surface:
//!   tools for frame conversion, CRS transforms, geodesics, geofence validation
//!   task-required `batch_transform`
//!   templates for frames, CRS metadata, operations, artifacts, and usage

use std::{collections::HashMap, net::SocketAddr, sync::Arc};

use axum::{
    Router,
    extract::{Path as AxumPath, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    },
    middleware,
    response::IntoResponse,
    routing::get,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use clap::Parser;
use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResult, CancelTaskParams, CancelTaskResult,
        CompleteRequestParams, CompleteResult, CompletionInfo, ContentBlock, CreateTaskResult,
        GetPromptRequestParams, GetPromptResult, GetTaskParams, GetTaskPayloadParams,
        GetTaskPayloadResult, GetTaskResult, ListPromptsResult, ListResourceTemplatesResult,
        ListResourcesResult, ListTasksResult, ListToolsResult, PaginatedRequestParams, Prompt,
        ReadResourceRequestParams, ReadResourceResult, Reference, Resource, ResourceContents,
        ResourceTemplate, ServerCapabilities, ServerInfo, Task, TaskStatus, TasksCapability,
    },
    service::RequestContext,
    tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use veoveo_coordinates_mcp::{
    artifacts::ArtifactRepository,
    contract::{
        BatchTransformOutput, BatchTransformRequest, ConvertFrameOutput, ConvertFrameRequest,
        CoordinatePoint, DeriveLocalFrameOutput, DeriveLocalFrameRequest, GeodesicDirectOutput,
        GeodesicDirectRequest, GeodesicInverseOutput, GeodesicInverseRequest, TransformCrsOutput,
        TransformCrsRequest, ValidateGeofenceOutput, ValidateGeofenceRequest, Wgs84Position,
    },
    engine,
    state::CoordinatesState,
    uris,
};
use veoveo_mcp_contract::{
    FrameId, GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalTokenVerifier, InternalTokenSecret,
    Page, ServerSlug, TaskPayloadState, TaskStore, TelemetryGuard, TokenIssuer, UsageReport,
    init_server_telemetry, is_sha256, now_iso, paginate, public_allowed_hosts, related_task_meta,
};
use veoveo_rrd::RrdFrameDefinition;

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

use app_state::{AppState, update_task};
use config::Args;
use host::validate_host;
use internal_auth::{
    InternalMcpAuthState, authenticate_internal_mcp, verify_internal_authorization,
};
use ownership::{
    caller_from, internal_caller, internal_identity, require_task_owner, task_owner_allows,
    task_owner_from_identity,
};
use prompts::CoordinatesPrompt;

const MCP_TASK_POLL_INTERVAL_MS: u64 = 1000;
const SERVER_SLUG: &str = "coordinates";
const LIST_PAGE_SIZE: usize = 100;
const BATCH_ARTIFACT_MIME: &str = "application/json";

fn install_rustls_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[derive(Clone)]
struct CoordinatesMcp {
    state: Arc<AppState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<CoordinatesMcp>,
}

#[tool_router]
impl CoordinatesMcp {
    fn new(state: Arc<AppState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        title = "Convert coordinate frame",
        description = "Convert WGS84, ECEF, ENU, and NED coordinates with explicit target frame and origin. Use transform_crs for projected CRS conversion.",
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
    ) -> Result<CallToolResult, McpError> {
        let target = self
            .state
            .coordinates
            .require_frame(&args.target_frame)
            .await
            .map_err(invalid_params)?;
        let origin = resolve_origin(&self.state, &args, &target).await?;
        let output = engine::convert_frame(args, &target, origin).map_err(invalid_params)?;
        self.state
            .coordinates
            .record_operation(output.provenance.clone())
            .await;
        structured_result(
            format!("converted {} point(s)", output.points.len()),
            &output,
        )
    }

    #[tool(
        title = "Transform CRS",
        description = "Transform projected/geodetic coordinate tuples between explicit CRS ids using PROJ with normalized axis order.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<TransformCrsOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn transform_crs(
        &self,
        Parameters(args): Parameters<TransformCrsRequest>,
    ) -> Result<CallToolResult, McpError> {
        let output = engine::transform_crs(args).map_err(invalid_params)?;
        self.state
            .coordinates
            .record_operation(output.provenance.clone())
            .await;
        structured_result(
            format!("transformed {} point(s)", output.points.len()),
            &output,
        )
    }

    #[tool(
        title = "Derive local frame",
        description = "Create an ENU or NED local tangent frame from a WGS84 origin.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<DeriveLocalFrameOutput>(),
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn derive_local_frame(
        &self,
        Parameters(args): Parameters<DeriveLocalFrameRequest>,
    ) -> Result<CallToolResult, McpError> {
        let output = engine::derive_local_frame(args).map_err(invalid_params)?;
        self.state
            .coordinates
            .insert_frame(output.frame.clone())
            .await
            .map_err(invalid_params)?;
        self.state
            .coordinates
            .record_operation(output.provenance.clone())
            .await;
        structured_result(format!("derived frame {}", output.frame.frame_id), &output)
    }

    #[tool(
        title = "Geodesic inverse",
        description = "Compute ellipsoidal WGS84 distance and forward/reverse azimuths between two geodetic positions.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<GeodesicInverseOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn geodesic_inverse(
        &self,
        Parameters(args): Parameters<GeodesicInverseRequest>,
    ) -> Result<CallToolResult, McpError> {
        let output = engine::geodesic_inverse(args).map_err(invalid_params)?;
        self.state
            .coordinates
            .record_operation(output.provenance.clone())
            .await;
        structured_result(format!("distance {:.3} m", output.distance_m), &output)
    }

    #[tool(
        title = "Geodesic direct",
        description = "Compute a destination WGS84 position from start, azimuth, and ellipsoidal distance.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<GeodesicDirectOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn geodesic_direct(
        &self,
        Parameters(args): Parameters<GeodesicDirectRequest>,
    ) -> Result<CallToolResult, McpError> {
        let output = engine::geodesic_direct(args).map_err(invalid_params)?;
        self.state
            .coordinates
            .record_operation(output.provenance.clone())
            .await;
        structured_result(
            format!(
                "destination {:.8}, {:.8}",
                output.end.latitude_deg, output.end.longitude_deg
            ),
            &output,
        )
    }

    #[tool(
        title = "Validate geofence",
        description = "Validate simple 2D path/geofence relationships in one explicit frame.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ValidateGeofenceOutput>(),
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn validate_geofence(
        &self,
        Parameters(args): Parameters<ValidateGeofenceRequest>,
    ) -> Result<CallToolResult, McpError> {
        let output = engine::validate_geofence(args).map_err(invalid_params)?;
        self.state
            .coordinates
            .record_operation(output.provenance.clone())
            .await;
        structured_result(
            if output.valid {
                "geofence valid".to_string()
            } else {
                format!(
                    "geofence invalid with {} violation(s)",
                    output.violations.len()
                )
            },
            &output,
        )
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
        ),
        execution(task_support = "required")
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

async fn resolve_origin(
    state: &AppState,
    request: &ConvertFrameRequest,
    target: &RrdFrameDefinition,
) -> Result<Option<Wgs84Position>, McpError> {
    if let Some(origin) = &request.origin {
        return Ok(Some(origin.clone()));
    }
    if let Some(origin) = &target.origin {
        return Wgs84Position::try_from(origin.clone())
            .map(Some)
            .map_err(|err| McpError::invalid_params(err.to_string(), None));
    }
    for point in &request.points {
        let frame_id = match point {
            CoordinatePoint::Enu(point) => Some(&point.frame_id),
            CoordinatePoint::Ned(point) => Some(&point.frame_id),
            _ => None,
        };
        if let Some(frame_id) = frame_id {
            let frame = state
                .coordinates
                .require_frame(frame_id)
                .await
                .map_err(invalid_params)?;
            if let Some(origin) = frame.origin {
                return Wgs84Position::try_from(origin)
                    .map(Some)
                    .map_err(|err| McpError::invalid_params(err.to_string(), None));
            }
        }
    }
    Ok(None)
}

fn mcp_page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    paginate(items, request, LIST_PAGE_SIZE)
        .map_err(|err| McpError::invalid_params(err.to_string(), None))
}

#[tool_handler]
impl ServerHandler for CoordinatesMcp {
    fn get_info(&self) -> ServerInfo {
        let caps: ServerCapabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .enable_completions()
            .enable_tasks_with(TasksCapability::server_default())
            .build();
        let mut info = ServerInfo::default();
        info.capabilities = caps;
        info.server_info =
            rmcp::model::Implementation::new("coordinates", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Coordinates planning server. Use resources for frames, CRS metadata, operations, \
             artifacts, and usage. Use direct tools for small transforms and geodesics; use \
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

    async fn enqueue_task(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, McpError> {
        let caller = internal_caller(&context)?;
        let identity = internal_identity(&context)?;
        if request.name != "batch_transform" {
            return Err(McpError::method_not_found::<
                rmcp::model::CallToolRequestMethod,
            >());
        }
        let args: BatchTransformRequest =
            serde_json::from_value(Value::Object(request.arguments.clone().unwrap_or_default()))
                .map_err(|err| {
                    McpError::invalid_params(
                        format!("invalid batch_transform arguments: {err}"),
                        None,
                    )
                })?;
        let progress_token = context.meta.get_progress_token();
        let ttl = request.task.as_ref().and_then(|task| task.ttl);
        let task_id = uuid::Uuid::new_v4().to_string();
        let now = now_iso();
        let mut task = Task::new(task_id.clone(), TaskStatus::Working, now.clone(), now)
            .with_status_message("accepted; preparing batch transform")
            .with_poll_interval(MCP_TASK_POLL_INTERVAL_MS);
        task.ttl = ttl;

        self.state.tasks.insert(task.clone(), None).await;
        let owner = task_owner_from_identity(&task_id, &identity);
        self.state
            .task_owners
            .write()
            .await
            .insert(task_id.clone(), owner.clone());

        let join = tokio::spawn(run_task(
            self.state.clone(),
            context.peer.clone(),
            task_id.clone(),
            caller,
            owner,
            args,
            progress_token,
        ));
        self.state.tasks.set_join(&task_id, join).await;
        Ok(CreateTaskResult::new(task).with_meta(related_task_meta(task_id)))
    }

    async fn list_tasks(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, McpError> {
        let identity = internal_identity(&context)?;
        let owners = self.state.task_owners.read().await;
        let tasks = self
            .state
            .tasks
            .list()
            .await
            .into_iter()
            .filter(|task| {
                owners
                    .get(&task.task_id)
                    .map(|owner| task_owner_allows(owner, &identity))
                    .unwrap_or(false)
            })
            .collect();
        let page = mcp_page(tasks, request.as_ref())?;
        let mut result = ListTasksResult::new(page.items);
        result.next_cursor = page.next_cursor;
        Ok(result)
    }

    async fn get_task_info(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        require_task_owner(&self.state, &context, &request.task_id).await?;
        let task = self
            .state
            .tasks
            .get(&request.task_id)
            .await
            .ok_or_else(|| McpError::invalid_params("unknown task id", None))?;
        Ok(GetTaskResult::new(task))
    }

    async fn get_task_result(
        &self,
        request: GetTaskPayloadParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, McpError> {
        require_task_owner(&self.state, &context, &request.task_id).await?;
        match self.state.tasks.await_payload_state(&request.task_id).await {
            TaskPayloadState::Completed(payload) => Ok(GetTaskPayloadResult::new(payload)),
            TaskPayloadState::Failed(error) => Err(McpError::internal_error(error, None)),
            TaskPayloadState::Cancelled => {
                Err(McpError::invalid_request("task was cancelled", None))
            }
            // await_payload_state blocks until terminal per MCP 2025-11-25;
            // Running here means the wait logic itself broke.
            TaskPayloadState::Running => Err(McpError::internal_error(
                "task payload wait ended while still running",
                None,
            )),
            TaskPayloadState::Unknown => Err(McpError::invalid_params("unknown task id", None)),
        }
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, McpError> {
        require_task_owner(&self.state, &context, &request.task_id).await?;
        let task = self
            .state
            .tasks
            .cancel(&request.task_id)
            .await
            .ok_or_else(|| McpError::invalid_params("unknown task id", None))?;
        Ok(CancelTaskResult::new(task))
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let identity = internal_identity(&context)?;
        let mut resources = vec![
            Resource::new(uris::FRAMES_URI, "frames")
                .with_title("Coordinate frames")
                .with_description("Visible coordinate frame definitions.")
                .with_mime_type("application/json"),
            Resource::new(uris::CRS_ROOT_URI, "crs")
                .with_title("Common CRS")
                .with_description("Common CRS identifiers and axis-order notes.")
                .with_mime_type("application/json"),
            Resource::new(uris::USAGE_ROOT_URI, "usage")
                .with_title("Coordinates usage ledger")
                .with_description("Index of task usage resources.")
                .with_mime_type("application/json"),
        ];
        for frame in self.state.coordinates.list_frames().await {
            resources.push(
                Resource::new(
                    uris::frame_uri(frame.frame_id.as_str()),
                    format!("frame {}", frame.frame_id),
                )
                .with_description(
                    frame
                        .description
                        .unwrap_or_else(|| "Coordinate frame.".into()),
                )
                .with_mime_type("application/json"),
            );
        }
        let owners = self.state.task_owners.read().await;
        for task in self.state.tasks.list().await {
            if owners
                .get(&task.task_id)
                .map(|owner| task_owner_allows(owner, &identity))
                .unwrap_or(false)
            {
                resources.push(
                    Resource::new(
                        uris::usage_task_uri(&task.task_id),
                        format!("usage for task {}", task.task_id),
                    )
                    .with_description("Usage rows for one coordinates task.")
                    .with_mime_type("application/json"),
                );
            }
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
            ResourceTemplate::new(uris::FRAME_TEMPLATE, "frame")
                .with_title("Coordinate frame")
                .with_description(
                    "Typed coordinate frame definition. frame_id supports completion.",
                )
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::CRS_TEMPLATE, "crs")
                .with_title("CRS metadata")
                .with_description(
                    "CRS metadata by authority and code. authority/code support completion.",
                )
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::OPERATION_TEMPLATE, "operation")
                .with_title("Coordinate operation")
                .with_description("Recorded operation provenance.")
                .with_mime_type("application/json"),
            ResourceTemplate::new(uris::ARTIFACT_TEMPLATE, "artifact")
                .with_title("Coordinates artifact")
                .with_description("Shared-plane immutable coordinates artifact.")
                .with_mime_type(BATCH_ARTIFACT_MIME),
            ResourceTemplate::new(uris::USAGE_TASK_TEMPLATE, "usage")
                .with_title("Coordinates task usage")
                .with_description("Usage rows for one coordinates task.")
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
        if uri == uris::FRAMES_URI {
            let frames = self.state.coordinates.list_frames().await;
            return json_resource(uri, &frames);
        }
        if uri == uris::CRS_ROOT_URI {
            return json_resource(uri, &engine::builtin_crs_metadata());
        }
        if uri == uris::USAGE_ROOT_URI {
            let identity = internal_identity(&context)?;
            let owners = self.state.task_owners.read().await;
            let mut entries = Vec::new();
            for task in self.state.tasks.list().await {
                if owners
                    .get(&task.task_id)
                    .map(|owner| task_owner_allows(owner, &identity))
                    .unwrap_or(false)
                {
                    entries.push(json!({
                        "task_id": task.task_id,
                        "usage_uri": uris::usage_task_uri(&task.task_id),
                    }));
                }
            }
            return json_resource(uri, &entries);
        }
        if let Some(frame_id) = uris::parse_frame_uri(uri) {
            let frame_id = FrameId::new(frame_id)
                .map_err(|err| McpError::invalid_params(err.to_string(), None))?;
            let frame = self
                .state
                .coordinates
                .get_frame(&frame_id)
                .await
                .ok_or_else(|| {
                    McpError::resource_not_found(format!("unknown frame `{frame_id}`"), None)
                })?;
            return json_resource(uri, &frame);
        }
        if let Some((authority, code)) = uris::parse_crs_uri(uri) {
            return json_resource(uri, &engine::crs_metadata(authority, code));
        }
        if let Some(operation_id) = uris::parse_operation_uri(uri) {
            let operation_id = veoveo_mcp_contract::CoordinateOperationId::new(operation_id)
                .map_err(|err| McpError::invalid_params(err.to_string(), None))?;
            let operation = self
                .state
                .coordinates
                .get_operation(&operation_id)
                .await
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
            let report: UsageReport = self.state.coordinates.usage_report(task_id).await;
            if report.records.is_empty() {
                return Err(McpError::resource_not_found(
                    format!("unknown usage task `{task_id}`"),
                    None,
                ));
            }
            return json_resource(uri, &report);
        }
        if let Some(sha256) = uris::parse_artifact_uri(uri) {
            let caller = internal_caller(&context)?;
            let artifact = self
                .state
                .artifacts
                .get(&caller, sha256)
                .await
                .map_err(|err| McpError::internal_error(err.to_string(), None))?
                .ok_or_else(|| {
                    McpError::resource_not_found(format!("unknown artifact `{sha256}`"), None)
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
        let prompts: Vec<Prompt> = CoordinatesPrompt::ALL
            .into_iter()
            .map(CoordinatesPrompt::prompt)
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
        let prompt = CoordinatesPrompt::by_name(&request.name).ok_or_else(|| {
            McpError::invalid_params(format!("unknown prompt `{}`", request.name), None)
        })?;
        prompt.render(request.arguments)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let Reference::Resource(res_ref) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let values = if res_ref.uri == uris::FRAME_TEMPLATE && request.argument.name == "frame_id" {
            let needle = request.argument.value.to_lowercase();
            self.state
                .coordinates
                .list_frames()
                .await
                .into_iter()
                .map(|frame| frame.frame_id.to_string())
                .filter(|frame| frame.to_lowercase().contains(&needle))
                .collect::<Vec<_>>()
        } else if res_ref.uri == uris::CRS_TEMPLATE && request.argument.name == "authority" {
            vec!["EPSG".to_string()]
        } else if res_ref.uri == uris::CRS_TEMPLATE && request.argument.name == "code" {
            let needle = request.argument.value.to_lowercase();
            ["4326", "4978", "3857", "32610", "32611", "32710", "32711"]
                .into_iter()
                .filter(|code| code.contains(&needle))
                .map(str::to_string)
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

async fn run_task(
    state: Arc<AppState>,
    peer: rmcp::service::Peer<RoleServer>,
    task_id: String,
    caller: veoveo_mcp_contract::PlaneCaller,
    owner: ownership::TaskOwner,
    args: BatchTransformRequest,
    progress_token: Option<rmcp::model::ProgressToken>,
) {
    macro_rules! fail {
        ($msg:expr) => {{
            let msg: String = $msg;
            tracing::warn!(task_id, "coordinates task failed: {msg}");
            update_task(
                &state,
                &peer,
                &task_id,
                TaskStatus::Failed,
                msg.clone(),
                None,
                Some(msg),
            )
            .await;
            return;
        }};
    }

    veoveo_mcp_contract::notify_progress(&peer, &progress_token, 0.1, "resolving frames").await;
    let target = match state
        .coordinates
        .require_frame(&args.convert.target_frame)
        .await
    {
        Ok(target) => target,
        Err(err) => fail!(format!("target frame error: {err}")),
    };
    let origin = match resolve_origin(&state, &args.convert, &target).await {
        Ok(origin) => origin,
        Err(err) => fail!(format!("origin resolution failed: {err}")),
    };
    let convert_args = args.convert.clone();
    veoveo_mcp_contract::notify_progress(&peer, &progress_token, 0.4, "converting coordinates")
        .await;
    let result = match tokio::task::spawn_blocking(move || {
        engine::convert_frame(convert_args, &target, origin)
    })
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(err)) => fail!(format!("batch transform failed: {err}")),
        Err(err) => fail!(format!("batch worker failed: {err}")),
    };
    state
        .coordinates
        .record_operation(result.provenance.clone())
        .await;
    let output = BatchTransformOutput {
        result,
        artifact: None,
    };
    veoveo_mcp_contract::notify_progress(&peer, &progress_token, 0.8, "writing artifacts").await;
    let result =
        match outputs::batch_result(&state, &caller, &task_id, &owner, output, args.artifact).await
        {
            Ok(result) => result,
            Err(err) => fail!(format!("artifact write failed: {err}")),
        };
    veoveo_mcp_contract::notify_progress(&peer, &progress_token, 1.0, "completed").await;
    update_task(
        &state,
        &peer,
        &task_id,
        TaskStatus::Completed,
        "completed; batch transform available",
        serde_json::to_value(&result).ok(),
        None,
    )
    .await;
}

async fn artifact_download(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    AxumPath(sha256): AxumPath<String>,
) -> impl IntoResponse {
    if !is_sha256(&sha256) {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    let identity = match verify_internal_authorization(&state.internal_token_verifier, &headers) {
        Ok(identity) => identity,
        Err(message) => {
            tracing::warn!(
                artifact_sha256 = sha256,
                "rejected coordinates artifact download: {message}"
            );
            return (StatusCode::UNAUTHORIZED, "gateway authorization required").into_response();
        }
    };
    let Some(bearer) = internal_auth::bearer_from_headers(&headers) else {
        return (StatusCode::UNAUTHORIZED, "gateway authorization required").into_response();
    };
    let caller = caller_from(identity, bearer);
    let artifact = match state.artifacts.get(&caller, &sha256).await {
        Ok(Some(artifact)) => artifact,
        Ok(None) => return (StatusCode::NOT_FOUND, "not found").into_response(),
        Err(err) => {
            tracing::warn!(artifact_sha256 = sha256, "artifact download failed: {err}");
            return (StatusCode::NOT_FOUND, "not found").into_response();
        }
    };

    let mut headers = HeaderMap::new();
    let mime = artifact
        .metadata
        .mime_type
        .as_deref()
        .unwrap_or(BATCH_ARTIFACT_MIME);
    if let Ok(value) = HeaderValue::from_str(mime) {
        headers.insert(CONTENT_TYPE, value);
    }
    let filename = artifact
        .metadata
        .filename
        .as_deref()
        .unwrap_or("coordinates.artifact")
        .replace(['"', '\r', '\n'], "_");
    if let Ok(value) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        headers.insert(CONTENT_DISPOSITION, value);
    }
    (StatusCode::OK, headers, artifact.bytes).into_response()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    install_rustls_provider();
    let _ = dotenvy::dotenv();
    let _telemetry: TelemetryGuard = init_server_telemetry(
        "veoveo-coordinates-mcp",
        "info,veoveo_coordinates_mcp=debug",
    )?;
    let args = Args::parse();
    let public_deployment = args.public_deployment()?;
    let public_endpoint = public_deployment.server(SERVER_SLUG)?;
    let internal_token_verifier = GatewayInternalTokenVerifier::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        ServerSlug::new(SERVER_SLUG)?,
        InternalTokenSecret::new(args.internal_token_secret.clone())?,
    );
    let state = Arc::new(AppState {
        tasks: TaskStore::new(),
        coordinates: CoordinatesState::new(),
        artifacts: ArtifactRepository::new(args.artifact_service_url.clone()),
        internal_token_verifier: internal_token_verifier.clone(),
        task_owners: RwLock::new(HashMap::new()),
    });

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
            move || Ok(CoordinatesMcp::new(state.clone()))
        },
        LocalSessionManager::default().into(),
        StreamableHttpServerConfig::default()
            .with_allowed_hosts(allowed_hosts.iter().cloned())
            .with_cancellation_token(ct.child_token()),
    );
    let mcp_router = Router::new()
        .route_service("/", mcp_service.clone())
        .route_service("/{*path}", mcp_service)
        .layer(middleware::from_fn_with_state(
            internal_auth_state,
            authenticate_internal_mcp,
        ));
    let server_router = Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/artifacts/{sha256}", get(artifact_download))
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
        service = "veoveo-coordinates-mcp",
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
