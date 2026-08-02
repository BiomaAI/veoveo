use std::sync::{Arc, LazyLock};

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, ContentBlock, ListResourceTemplatesResult, ListResourcesResult,
        ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult,
        Resource, ResourceContents, ResourceTemplate, ServerCapabilities, ServerInfo,
        SubscribeRequestParams, UnsubscribeRequestParams,
    },
    service::RequestContext,
    tool_handler, tool_router,
};
use serde::Serialize;
use veoveo_mcp_contract::{
    GatewayInternalIdentity, LiveViewOwner, Page, PlaneCaller, ResourceListObservers,
    SubscriptionHub,
    docs::{CapabilityInventory, ServerDocs},
    paginate, tool,
};

use crate::{
    artifacts::SceneArtifactMaterializer,
    contract::{
        AuthorizePoseProducerRequest, BindSceneRequest, CameraAdmission, CapacityState,
        CloseCameraRequest, CloseLiveViewRequest, CloseResult, CloseSessionRequest,
        CreateCameraRequest, CreateSessionRequest, GetCapacityRequest, GetSessionStateRequest,
        OpenLiveViewRequest, OpenLiveViewResult, PoseSourceState, RenewLiveViewRequest,
        RevokePoseProducerRequest, SetCameraRequest, SimulationViewError, SimulationViewSession,
    },
    durability::SimulationViewRepository,
    runtime::RuntimeClients,
    state::SimulationViewService,
    uris,
};

const LIST_PAGE_SIZE: usize = 100;
const LIVE_APP_TOOLS: &[&str] = &[
    "get_capacity",
    "get_session_state",
    "create_camera",
    "set_camera",
    "close_camera",
    "open_live_view",
    "renew_live_view",
    "close_live_view",
];
const LIVE_APP_ICON: &str = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSIyNCIgaGVpZ2h0PSIyNCIgdmlld0JveD0iMCAwIDI0IDI0IiBmaWxsPSJub25lIiBzdHJva2U9IiM2NmU0ZmYiIHN0cm9rZS13aWR0aD0iMiI+PHJlY3QgeD0iMiIgeT0iNSIgd2lkdGg9IjIwIiBoZWlnaHQ9IjE0IiByeD0iMiIvPjxwYXRoIGQ9Im04IDlsNiAzLTYgM3oiLz48L3N2Zz4=";

pub(crate) static SERVER_DOCS: LazyLock<ServerDocs> =
    LazyLock::new(|| veoveo_mcp_contract::server_docs!("simulation-view"));

pub(crate) struct SimulationViewMcpState {
    pub service: Arc<SimulationViewService>,
    pub subscriptions: Arc<SubscriptionHub>,
    pub list_observers: ResourceListObservers,
    runtimes: Arc<RuntimeClients>,
    artifacts: Arc<SceneArtifactMaterializer>,
    repository: Arc<SimulationViewRepository>,
    app_connect_origin: String,
}

impl SimulationViewMcpState {
    pub(crate) fn new(
        service: Arc<SimulationViewService>,
        runtimes: Arc<RuntimeClients>,
        artifacts: Arc<SceneArtifactMaterializer>,
        repository: Arc<SimulationViewRepository>,
        signaling_url: &str,
    ) -> anyhow::Result<Arc<Self>> {
        let signaling_url = url::Url::parse(signaling_url)?;
        let app_connect_origin = signaling_url.origin().ascii_serialization();
        anyhow::ensure!(
            app_connect_origin != "null",
            "public signaling URL must have a CSP origin"
        );
        Ok(Arc::new(Self {
            service,
            subscriptions: Arc::new(SubscriptionHub::new()),
            list_observers: ResourceListObservers::new(),
            runtimes,
            artifacts,
            repository,
            app_connect_origin,
        }))
    }
}

#[derive(Clone)]
pub(crate) struct SimulationViewMcp {
    state: Arc<SimulationViewMcpState>,
    #[allow(dead_code)]
    tool_router: ToolRouter<SimulationViewMcp>,
}

#[tool_router]
impl SimulationViewMcp {
    pub(crate) fn new(state: Arc<SimulationViewMcpState>) -> Self {
        Self {
            state,
            tool_router: Self::tool_router(),
        }
    }

    async fn persist(
        &self,
        session_id: &veoveo_mcp_contract::LiveSessionId,
    ) -> Result<(), McpError> {
        self.state
            .repository
            .persist(&self.state.service, session_id)
            .await
            .map_err(|error| {
                tracing::error!(%error, %session_id, "failed to persist Simulation View desired state");
                McpError::internal_error(
                    "Simulation View desired state could not be committed",
                    None,
                )
            })
    }

    async fn refresh_pose_status(
        &self,
        owner: &LiveViewOwner,
        session_id: &veoveo_mcp_contract::LiveSessionId,
    ) -> Result<(), McpError> {
        let session = self
            .state
            .service
            .get_session(owner, session_id)
            .map_err(service_error)?;
        if session.geospatial_layer.is_some() {
            match self.state.runtimes.layer_status(session_id).await {
                Ok(Some(status)) => self
                    .state
                    .service
                    .apply_layer_status(session_id, status)
                    .map_err(service_error)?,
                Ok(None) | Err(_) => {
                    self.state.service.mark_layer_unavailable(session_id);
                }
            }
        }
        if session.pose_source.is_none() {
            return Ok(());
        }
        match self.state.runtimes.pose_status(session_id).await {
            Ok(status) => self
                .state
                .service
                .apply_pose_status(session_id, &status)
                .map_err(service_error),
            Err(_) => {
                self.state.service.mark_pose_stale(session_id);
                Ok(())
            }
        }
    }

    async fn refresh_camera_statuses(
        &self,
        owner: &LiveViewOwner,
        session_id: &veoveo_mcp_contract::LiveSessionId,
    ) {
        let cameras = self.state.service.list_cameras(owner, session_id);
        let statuses = futures::future::join_all(cameras.iter().map(|camera| {
            self.state
                .runtimes
                .camera_status(session_id, &camera.camera_id)
        }))
        .await;
        for (camera, status) in cameras.iter().zip(statuses) {
            match status {
                Ok(status) => self.state.service.refresh_camera_status(
                    &camera.camera_id,
                    status.ready,
                    status.last_pose_sequence,
                    status.last_frame_at,
                ),
                Err(_) => self.state.service.mark_camera_stale(&camera.camera_id),
            }
        }
    }

    fn capability_inventory() -> CapabilityInventory {
        CapabilityInventory {
            tools: Self::tool_router()
                .list_all()
                .into_iter()
                .map(|tool| tool.name.to_string())
                .collect(),
            resources: vec![
                uris::SESSIONS.to_owned(),
                uris::CAPACITY.to_owned(),
                uris::DOCS.to_owned(),
                uris::CONTRACT.to_owned(),
                uris::LIVE_APP_URI.to_owned(),
            ],
            resource_templates: vec![
                uris::SESSION_TEMPLATE.to_owned(),
                uris::SCENE_TEMPLATE.to_owned(),
                uris::POSE_SOURCE_TEMPLATE.to_owned(),
                uris::RECONCILIATION_TEMPLATE.to_owned(),
                uris::CAMERAS_TEMPLATE.to_owned(),
                uris::CAMERA_TEMPLATE.to_owned(),
                uris::STREAMS_TEMPLATE.to_owned(),
                uris::STREAM_TEMPLATE.to_owned(),
                uris::DOC_TEMPLATE.to_owned(),
            ],
            prompts: Vec::new(),
            tasks: Vec::new(),
        }
    }

    #[tool(
        title = "Create simulation-view session",
        description = "Create an owner-scoped renderer session for one simulation epoch. This does not start or control simulation dynamics.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SimulationViewSession>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn create_session(
        &self,
        Parameters(request): Parameters<CreateSessionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let owner = require_owner(&context, "simulation-view:write")?;
        let session = self
            .state
            .service
            .create_session(owner, request)
            .map_err(service_error)?;
        self.persist(&session.session_id).await?;
        if let Err(error) = self.state.runtimes.create_session(&session).await {
            return Err(runtime_error(error));
        }
        self.notify_session_created(&session, &context).await;
        structured_result(
            format!("created {}", uris::session(&session.session_id)),
            &session,
        )
    }

    #[tool(
        title = "Get simulation-view session state",
        description = "Read one owner-scoped renderer session, including its immutable scene and pose-producer health.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SimulationViewSession>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn get_session_state(
        &self,
        Parameters(request): Parameters<GetSessionStateRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let owner = require_owner(&context, "simulation-view:read")?;
        self.refresh_pose_status(&owner, &request.session_id)
            .await?;
        let session = self
            .state
            .service
            .get_session(&owner, &request.session_id)
            .map_err(service_error)?;
        structured_result(
            format!("read {}", uris::session(&session.session_id)),
            &session,
        )
    }

    #[tool(
        title = "Bind governed simulation scene",
        description = "Bind one immutable, digest-verified scene declaration to a session. Only artifact-plane content is admitted.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<SimulationViewSession>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn bind_scene(
        &self,
        Parameters(request): Parameters<BindSceneRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let caller = require_caller(&context, "simulation-view:write")?;
        let owner = LiveViewOwner::from_identity(&caller.identity);
        self.state
            .service
            .validate_bind_scene(&owner, &request)
            .map_err(service_error)?;
        self.state
            .artifacts
            .materialize(&caller, &request.scene)
            .await
            .map_err(runtime_error)?;
        let session = self
            .state
            .service
            .bind_scene(&owner, request)
            .map_err(service_error)?;
        self.persist(&session.session_id).await?;
        if let Err(error) = self.state.runtimes.bind_scene(&session).await {
            return Err(runtime_error(error));
        }
        self.notify_session(&session.session_id).await;
        structured_result(
            format!("bound {}", uris::scene(&session.session_id)),
            &session,
        )
    }

    #[tool(
        title = "Authorize simulation pose producer",
        description = "Authorize one SPIFFE-identified producer to publish bounded latest-pose snapshots through the private pose data plane.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<PoseSourceState>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn authorize_pose_producer(
        &self,
        Parameters(request): Parameters<AuthorizePoseProducerRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let owner = require_owner(&context, "simulation-view:write")?;
        let session_id = request.session_id.clone();
        let result = self
            .state
            .service
            .authorize_pose_producer(&owner, request)
            .map_err(service_error)?;
        let session = self
            .state
            .service
            .get_session(&owner, &session_id)
            .map_err(service_error)?;
        self.persist(&session_id).await?;
        if let Err(error) = self.state.runtimes.bind_pose(&session, &result).await {
            return Err(runtime_error(error));
        }
        self.notify_session(&session_id).await;
        structured_result(
            format!("authorized {}", uris::pose_source(&session_id)),
            &result,
        )
    }

    #[tool(
        title = "Revoke simulation pose producer",
        description = "Revoke the current pose producer immediately and mark its source stale.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<PoseSourceState>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn revoke_pose_producer(
        &self,
        Parameters(request): Parameters<RevokePoseProducerRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let owner = require_owner(&context, "simulation-view:write")?;
        let session_id = request.session_id.clone();
        let result = self
            .state
            .service
            .revoke_pose_producer(&owner, request)
            .map_err(service_error)?;
        let session = self
            .state
            .service
            .get_session(&owner, &session_id)
            .map_err(service_error)?;
        self.persist(&session_id).await?;
        if let Err(error) = self.state.runtimes.revoke_pose(&session, &result).await {
            return Err(runtime_error(error));
        }
        self.notify_session(&session_id).await;
        structured_result(
            format!("revoked {}", uris::pose_source(&session_id)),
            &result,
        )
    }

    #[tool(
        title = "Create logical simulation camera",
        description = "Admit an owner-scoped fixed, look-at, orbit, follow, chase, mounted, or formation camera without silently reducing its requested quality.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CameraAdmission>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn create_camera(
        &self,
        Parameters(request): Parameters<CreateCameraRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let owner = require_owner(&context, "simulation-view:write")?;
        let session_id = request.session_id.clone();
        let mut result = self
            .state
            .service
            .create_camera(&owner, request)
            .map_err(service_error)?;
        if let CameraAdmission::Admitted { camera } = &mut result {
            self.persist(&session_id).await?;
            let render_slot = self
                .state
                .service
                .render_slot(&camera.camera_id)
                .map_err(service_error)?;
            match self.state.runtimes.upsert_camera(camera, render_slot).await {
                Ok(status) if status.ready => {
                    self.state.service.apply_camera_status(
                        &camera.camera_id,
                        status.last_pose_sequence,
                        status.last_frame_at,
                    );
                    camera.health = veoveo_mcp_contract::LiveCameraHealth::Healthy;
                    camera.last_pose_sequence = status.last_pose_sequence;
                    camera.last_frame_at = status.last_frame_at;
                }
                Ok(_) => {}
                Err(error) => {
                    self.state.service.fail_camera(&camera.camera_id);
                    return Err(runtime_error(error));
                }
            }
        }
        if matches!(result, CameraAdmission::Admitted { .. }) {
            self.notify_cameras(&session_id, &context).await;
        }
        structured_result("camera admission completed".to_owned(), &result)
    }

    #[tool(
        title = "Set logical simulation camera",
        description = "Replace one camera definition under optimistic revision control. Existing stream leases close when the camera revision changes.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CameraAdmission>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false)
    )]
    async fn set_camera(
        &self,
        Parameters(request): Parameters<SetCameraRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let owner = require_owner(&context, "simulation-view:write")?;
        let session_id = request.session_id.clone();
        let camera_id = request.camera_id.clone();
        let mut result = self
            .state
            .service
            .set_camera(&owner, request)
            .map_err(service_error)?;
        if let CameraAdmission::Admitted { camera } = &mut result {
            self.persist(&session_id).await?;
            let render_slot = self
                .state
                .service
                .render_slot(&camera.camera_id)
                .map_err(service_error)?;
            match self.state.runtimes.upsert_camera(camera, render_slot).await {
                Ok(status) if status.ready => {
                    self.state.service.apply_camera_status(
                        &camera.camera_id,
                        status.last_pose_sequence,
                        status.last_frame_at,
                    );
                    camera.health = veoveo_mcp_contract::LiveCameraHealth::Healthy;
                    camera.last_pose_sequence = status.last_pose_sequence;
                    camera.last_frame_at = status.last_frame_at;
                }
                Ok(_) => {}
                Err(error) => {
                    self.state.service.fail_camera(&camera.camera_id);
                    return Err(runtime_error(error));
                }
            }
        }
        if matches!(result, CameraAdmission::Admitted { .. }) {
            self.state
                .subscriptions
                .notify_resource_updated(uris::camera(&session_id, &camera_id))
                .await;
            self.notify_cameras(&session_id, &context).await;
            self.state
                .subscriptions
                .notify_resource_updated(uris::streams(&session_id))
                .await;
        }
        structured_result("camera admission completed".to_owned(), &result)
    }

    #[tool(
        title = "Close logical simulation camera",
        description = "Close one owner-scoped camera and every associated stream lease.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CloseResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn close_camera(
        &self,
        Parameters(request): Parameters<CloseCameraRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let owner = require_owner(&context, "simulation-view:write")?;
        let session_id = request.session_id.clone();
        let camera_id = request.camera_id.clone();
        let result = self
            .state
            .service
            .close_camera(&owner, request)
            .map_err(service_error)?;
        self.persist(&session_id).await?;
        if let Err(error) = self
            .state
            .runtimes
            .close_camera(&session_id, &camera_id)
            .await
        {
            return Err(runtime_error(error));
        }
        self.notify_cameras(&session_id, &context).await;
        self.state
            .subscriptions
            .notify_resource_updated(uris::streams(&session_id))
            .await;
        structured_result("closed camera".to_owned(), &result)
    }

    #[tool(
        title = "Open simulation live view",
        description = "Open or rotate an owner-scoped H.264 NVENC WebRTC lease for one admitted logical camera.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<OpenLiveViewResult>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true)
    )]
    async fn open_live_view(
        &self,
        Parameters(request): Parameters<OpenLiveViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let owner = require_owner(&context, "simulation-view:stream")?;
        let session_id = request.session_id.clone();
        ensure_pose_authorization_current(
            &self
                .state
                .service
                .get_session(&owner, &session_id)
                .map_err(service_error)?,
        )?;
        let mut result = self
            .state
            .service
            .open_live_view(&owner, request)
            .map_err(service_error)?;
        self.persist(&session_id).await?;
        let render_slot = self
            .state
            .service
            .render_slot(&result.stream.camera_id)
            .map_err(service_error)?;
        let status = match self
            .state
            .runtimes
            .open_stream(&result.stream, render_slot)
            .await
        {
            Ok(status) => status,
            Err(error) => {
                self.state.service.abort_stream(&result.stream.live_view_id);
                return Err(runtime_error(error));
            }
        };
        self.state.service.apply_camera_status(
            &result.stream.camera_id,
            status.last_pose_sequence,
            status.last_frame_at,
        );
        self.state
            .service
            .mark_stream_ready(&result.stream.live_view_id);
        result.stream.lifecycle = veoveo_mcp_contract::LiveViewLifecycle::Ready;
        result.stream.camera_health = veoveo_mcp_contract::LiveCameraHealth::Healthy;
        result.stream.last_frame_at = status.last_frame_at;
        self.notify_streams(&session_id, &context).await;
        structured_result(
            format!("opened {}", result.stream.resource_uri.as_str()),
            &result,
        )
    }

    #[tool(
        title = "Renew simulation live view",
        description = "Rotate the secret access token and renew an unexpired owner-scoped live-view lease.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<OpenLiveViewResult>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true)
    )]
    async fn renew_live_view(
        &self,
        Parameters(request): Parameters<RenewLiveViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let owner = require_owner(&context, "simulation-view:stream")?;
        let session_id = request.session_id.clone();
        let result = self
            .state
            .service
            .renew_live_view(&owner, request)
            .map_err(service_error)?;
        self.persist(&session_id).await?;
        self.state
            .subscriptions
            .notify_resource_updated(result.stream.resource_uri.as_str())
            .await;
        self.state
            .subscriptions
            .notify_resource_updated(uris::streams(&session_id))
            .await;
        structured_result(
            format!("renewed {}", result.stream.resource_uri.as_str()),
            &result,
        )
    }

    #[tool(
        title = "Close simulation live view",
        description = "Revoke one owner-scoped live-view lease and its signaling token.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CloseResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn close_live_view(
        &self,
        Parameters(request): Parameters<CloseLiveViewRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let owner = require_owner(&context, "simulation-view:stream")?;
        let session_id = request.session_id.clone();
        let stream_id = request.live_view_id.clone();
        let result = self
            .state
            .service
            .close_live_view(&owner, request)
            .map_err(service_error)?;
        self.persist(&session_id).await?;
        if let Err(error) = self
            .state
            .runtimes
            .close_stream(&session_id, &stream_id)
            .await
        {
            return Err(runtime_error(error));
        }
        self.state
            .subscriptions
            .notify_resource_updated(&result.resource_uri)
            .await;
        self.state
            .subscriptions
            .notify_resource_updated(uris::streams(&session_id))
            .await;
        structured_result("closed live view".to_owned(), &result)
    }

    #[tool(
        title = "Get simulation-view capacity",
        description = "Inspect the exact camera, render-pixel, NVENC, GPU-memory, entity, owner, and Work Context admission budgets and current usage.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CapacityState>(),
        annotations(read_only_hint = true, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn get_capacity(
        &self,
        Parameters(_request): Parameters<GetCapacityRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        require_owner(&context, "simulation-view:read")?;
        structured_result(
            "read simulation-view capacity".to_owned(),
            &self.state.service.capacity(),
        )
    }

    #[tool(
        title = "Close simulation-view session",
        description = "Close one renderer session, revoke its pose producer and streams, and remove its logical cameras.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<CloseResult>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = false, open_world_hint = false)
    )]
    async fn close_session(
        &self,
        Parameters(request): Parameters<CloseSessionRequest>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let owner = require_owner(&context, "simulation-view:write")?;
        let session_id = request.session_id.clone();
        let result = self
            .state
            .service
            .close_session(&owner, request)
            .map_err(service_error)?;
        self.persist(&session_id).await?;
        let session = self
            .state
            .service
            .get_session(&owner, &session_id)
            .map_err(service_error)?;
        if let Some(source) = session.pose_source.as_ref() {
            if let Err(error) = self.state.runtimes.revoke_pose(&session, source).await {
                return Err(runtime_error(error));
            }
        }
        if let Err(error) = self.state.runtimes.close_session(&session_id).await {
            return Err(runtime_error(error));
        }
        self.notify_session(&session_id).await;
        self.notify_cameras(&session_id, &context).await;
        self.notify_streams(&session_id, &context).await;
        structured_result("closed simulation-view session".to_owned(), &result)
    }

    async fn notify_session_created(
        &self,
        session: &SimulationViewSession,
        context: &RequestContext<RoleServer>,
    ) {
        self.state
            .subscriptions
            .notify_resource_updated(uris::SESSIONS)
            .await;
        self.state.list_observers.notify_changed().await;
        veoveo_mcp_contract::notify_resource_list_changed(&context.peer).await;
        self.notify_session(&session.session_id).await;
    }

    async fn notify_session(&self, session_id: &veoveo_mcp_contract::LiveSessionId) {
        for uri in [
            uris::session(session_id),
            uris::scene(session_id),
            uris::pose_source(session_id),
            uris::reconciliation(session_id),
        ] {
            self.state.subscriptions.notify_resource_updated(uri).await;
        }
    }

    async fn notify_cameras(
        &self,
        session_id: &veoveo_mcp_contract::LiveSessionId,
        context: &RequestContext<RoleServer>,
    ) {
        self.state
            .subscriptions
            .notify_resource_updated(uris::cameras(session_id))
            .await;
        self.state
            .subscriptions
            .notify_resource_updated(uris::CAPACITY)
            .await;
        self.state.list_observers.notify_changed().await;
        veoveo_mcp_contract::notify_resource_list_changed(&context.peer).await;
    }

    async fn notify_streams(
        &self,
        session_id: &veoveo_mcp_contract::LiveSessionId,
        context: &RequestContext<RoleServer>,
    ) {
        self.state
            .subscriptions
            .notify_resource_updated(uris::streams(session_id))
            .await;
        self.state
            .subscriptions
            .notify_resource_updated(uris::CAPACITY)
            .await;
        self.state.list_observers.notify_changed().await;
        veoveo_mcp_contract::notify_resource_list_changed(&context.peer).await;
    }
}

#[tool_handler]
impl ServerHandler for SimulationViewMcp {
    fn get_info(&self) -> ServerInfo {
        let mut capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_resources_subscribe()
            .enable_resources_list_changed()
            .build();
        veoveo_mcp_apps_extension::extend_capabilities(&mut capabilities);
        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info =
            rmcp::model::Implementation::new("simulation-view", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Bind one governed render-only scene, authorize a private pose producer, create \
             capacity-admitted logical cameras, and open owner-scoped NVENC WebRTC leases. MCP \
             carries control and resources only; continuous poses and media use private data \
             planes. The ui://simulation-view/live.html App drives the same camera and lease tools."
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
        let tools = tools
            .into_iter()
            .map(|tool| {
                if LIVE_APP_TOOLS.contains(&tool.name.as_ref()) {
                    veoveo_mcp_apps_extension::link_tool_to_app(
                        tool,
                        uris::LIVE_APP_URI,
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
            meta: None,
        })
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let identity = require_identity(&context, "simulation-view:read")?;
        let owner = LiveViewOwner::from_identity(&identity);
        let sessions = self.state.service.list_sessions(&owner);
        let mut resources = vec![
            json_descriptor(
                uris::SESSIONS,
                "Simulation-view sessions",
                "Owner-scoped renderer sessions.",
            ),
            json_descriptor(
                uris::CAPACITY,
                "Simulation-view capacity",
                "Current explicit renderer and encoder capacity.",
            ),
            json_descriptor(
                uris::DOCS,
                "Simulation View documents",
                "Embedded server documents for this deployed version.",
            ),
            json_descriptor(
                uris::CONTRACT,
                "Simulation View contract",
                "Machine-readable hosted MCP contract declaration.",
            ),
        ];
        if identity_has_scope(&identity, "simulation-view:stream") {
            resources.push(
                veoveo_mcp_apps_extension::app_resource_with_meta(
                    uris::LIVE_APP_URI,
                    "simulation-view-live-app",
                    veoveo_mcp_apps_extension::ResourceUiMeta {
                        csp: Some(veoveo_mcp_apps_extension::UiCsp {
                            connect_domains: vec![self.state.app_connect_origin.clone()],
                            ..Default::default()
                        }),
                        prefers_border: Some(true),
                    },
                )
                .with_title("Simulation live views")
                .with_description(
                    "Generic owner-scoped camera collection and WebRTC player for Simulation View.",
                )
                .with_icons(vec![rmcp::model::Icon::new(LIVE_APP_ICON)]),
            );
        }
        for session in sessions {
            resources.extend([
                json_descriptor(
                    &uris::session(&session.session_id),
                    "Simulation-view session",
                    "Renderer session state.",
                ),
                json_descriptor(
                    &uris::scene(&session.session_id),
                    "Simulation scene",
                    "Immutable governed scene declaration.",
                ),
                json_descriptor(
                    &uris::pose_source(&session.session_id),
                    "Pose source",
                    "Authorized private pose producer health.",
                ),
                json_descriptor(
                    &uris::reconciliation(&session.session_id),
                    "Simulation reconciliation",
                    "Desired and realized runtime revisions, bounded authorization renewal, and typed recovery diagnostics.",
                ),
                json_descriptor(
                    &uris::cameras(&session.session_id),
                    "Simulation cameras",
                    "Capacity-admitted logical cameras.",
                ),
                json_descriptor(
                    &uris::streams(&session.session_id),
                    "Simulation streams",
                    "Owner-scoped live-view lease states without tokens.",
                ),
            ]);
            resources.extend(
                self.state
                    .service
                    .list_cameras(&owner, &session.session_id)
                    .into_iter()
                    .map(|camera| {
                        json_descriptor(
                            &uris::camera(&session.session_id, &camera.camera_id),
                            "Simulation camera",
                            "Logical camera state.",
                        )
                    }),
            );
            resources.extend(
                self.state
                    .service
                    .list_streams(&owner, &session.session_id)
                    .into_iter()
                    .map(|stream| {
                        json_descriptor(
                            stream.resource_uri.as_str(),
                            "Simulation live view",
                            "Live-view lease state without its secret token.",
                        )
                    }),
            );
        }
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
        self.state.list_observers.observe(context.peer).await;
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
                uris::SESSION_TEMPLATE,
                "Simulation-view session",
                "Renderer session state.",
            ),
            template(
                uris::SCENE_TEMPLATE,
                "Simulation scene",
                "Immutable governed scene declaration.",
            ),
            template(
                uris::POSE_SOURCE_TEMPLATE,
                "Pose source",
                "Authorized pose producer health.",
            ),
            template(
                uris::RECONCILIATION_TEMPLATE,
                "Simulation reconciliation",
                "Managed runtime reconciliation status.",
            ),
            template(
                uris::CAMERAS_TEMPLATE,
                "Simulation cameras",
                "Logical camera collection.",
            ),
            template(
                uris::CAMERA_TEMPLATE,
                "Simulation camera",
                "Logical camera state.",
            ),
            template(
                uris::STREAMS_TEMPLATE,
                "Simulation streams",
                "Live-view lease collection.",
            ),
            template(
                uris::STREAM_TEMPLATE,
                "Simulation live view",
                "Live-view state without its secret token.",
            ),
            template(
                uris::DOC_TEMPLATE,
                "Simulation View document",
                "Embedded server document.",
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
        let uri = request.uri.as_str();
        if uri == uris::LIVE_APP_URI {
            require_identity(&context, "simulation-view:stream")?;
            return Ok(ReadResourceResult::new(vec![
                veoveo_mcp_apps_extension::app_html_contents(uri, crate::app::live_app_html()),
            ]));
        }
        let identity = require_identity(&context, "simulation-view:read")?;
        let owner = LiveViewOwner::from_identity(&identity);
        match uri {
            uris::SESSIONS => {
                let session_ids = self
                    .state
                    .service
                    .list_sessions(&owner)
                    .into_iter()
                    .map(|session| session.session_id)
                    .collect::<Vec<_>>();
                for session_id in session_ids {
                    self.refresh_pose_status(&owner, &session_id).await?;
                }
                return json_resource(uri, &self.state.service.list_sessions(&owner));
            }
            uris::CAPACITY => return json_resource(uri, &self.state.service.capacity()),
            uris::DOCS => return json_resource(uri, &SERVER_DOCS.iter().collect::<Vec<_>>()),
            uris::CONTRACT => {
                return json_resource(
                    uri,
                    SERVER_DOCS.contract_declaration(Self::capability_inventory),
                );
            }
            _ => {}
        }
        if let Some(doc_id) = uris::parse_doc(uri) {
            let doc = SERVER_DOCS.doc(doc_id).ok_or_else(not_found)?;
            return Ok(ReadResourceResult::new(vec![
                ResourceContents::text(doc.body, uri).with_mime_type("text/markdown"),
            ]));
        }
        if let Some(session_id) = uris::parse_session(uri) {
            self.refresh_pose_status(&owner, &session_id).await?;
            return json_resource(
                uri,
                &self
                    .state
                    .service
                    .get_session(&owner, &session_id)
                    .map_err(resource_error)?,
            );
        }
        if let Some(session_id) = uris::parse_scene(uri) {
            let session = self
                .state
                .service
                .get_session(&owner, &session_id)
                .map_err(resource_error)?;
            return json_resource(uri, session.scene.as_ref().ok_or_else(not_found)?);
        }
        if let Some(session_id) = uris::parse_pose_source(uri) {
            self.refresh_pose_status(&owner, &session_id).await?;
            let session = self
                .state
                .service
                .get_session(&owner, &session_id)
                .map_err(resource_error)?;
            return json_resource(uri, session.pose_source.as_ref().ok_or_else(not_found)?);
        }
        if let Some(session_id) = uris::parse_reconciliation(uri) {
            let session = self
                .state
                .service
                .get_session(&owner, &session_id)
                .map_err(resource_error)?;
            return json_resource(uri, &session.reconciliation);
        }
        if let Some(session_id) = uris::parse_cameras(uri) {
            self.state
                .service
                .get_session(&owner, &session_id)
                .map_err(resource_error)?;
            self.refresh_camera_statuses(&owner, &session_id).await;
            return json_resource(uri, &self.state.service.list_cameras(&owner, &session_id));
        }
        if let Some((session_id, camera_id)) = uris::parse_camera(uri) {
            self.refresh_camera_statuses(&owner, &session_id).await;
            let camera = self
                .state
                .service
                .get_camera(&owner, &camera_id)
                .map_err(resource_error)?;
            if camera.session_id != session_id {
                return Err(not_found());
            }
            return json_resource(uri, &camera);
        }
        if let Some(session_id) = uris::parse_streams(uri) {
            self.state
                .service
                .get_session(&owner, &session_id)
                .map_err(resource_error)?;
            self.refresh_camera_statuses(&owner, &session_id).await;
            return json_resource(uri, &self.state.service.list_streams(&owner, &session_id));
        }
        if let Some((session_id, stream_id)) = uris::parse_stream(uri) {
            self.refresh_camera_statuses(&owner, &session_id).await;
            let stream = self
                .state
                .service
                .get_stream(&owner, &stream_id)
                .map_err(resource_error)?;
            if stream.session_id != session_id {
                return Err(not_found());
            }
            return json_resource(uri, &stream);
        }
        Err(not_found())
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = require_identity(&context, "simulation-view:read")?;
        if !is_subscribable(&request.uri) {
            return Err(McpError::invalid_params(
                "resource is immutable or not subscribable",
                None,
            ));
        }
        let owner = LiveViewOwner::from_identity(&identity);
        authorize_resource(&self.state.service, &owner, &request.uri)?;
        self.state
            .subscriptions
            .subscribe(request.uri, identity.actor.id, context.peer)
            .await;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let identity = require_identity(&context, "simulation-view:read")?;
        self.state
            .subscriptions
            .unsubscribe(&request.uri, &identity.actor.id)
            .await;
        Ok(())
    }
}

fn authorize_resource(
    service: &SimulationViewService,
    owner: &LiveViewOwner,
    uri: &str,
) -> Result<(), McpError> {
    if matches!(uri, uris::SESSIONS | uris::CAPACITY) {
        return Ok(());
    }
    let session_id = uris::parse_session(uri)
        .or_else(|| uris::parse_scene(uri))
        .or_else(|| uris::parse_pose_source(uri))
        .or_else(|| uris::parse_reconciliation(uri))
        .or_else(|| uris::parse_cameras(uri))
        .or_else(|| uris::parse_streams(uri))
        .or_else(|| uris::parse_camera(uri).map(|(session_id, _)| session_id))
        .or_else(|| uris::parse_stream(uri).map(|(session_id, _)| session_id))
        .ok_or_else(not_found)?;
    service
        .get_session(owner, &session_id)
        .map(|_| ())
        .map_err(resource_error)
}

fn require_identity(
    context: &RequestContext<RoleServer>,
    required: &str,
) -> Result<GatewayInternalIdentity, McpError> {
    let identity = context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<GatewayInternalIdentity>())
        .cloned()
        .ok_or_else(|| McpError::invalid_request("gateway identity missing", None))?;
    identity_has_scope(&identity, required)
        .then_some(identity)
        .ok_or_else(|| McpError::invalid_request(format!("scope `{required}` is required"), None))
}

fn require_owner(
    context: &RequestContext<RoleServer>,
    required: &str,
) -> Result<LiveViewOwner, McpError> {
    require_identity(context, required).map(|identity| LiveViewOwner::from_identity(&identity))
}

fn require_caller(
    context: &RequestContext<RoleServer>,
    required: &str,
) -> Result<PlaneCaller, McpError> {
    let identity = require_identity(context, required)?;
    let bearer_token = context
        .extensions
        .get::<axum::http::request::Parts>()
        .and_then(|parts| parts.extensions.get::<crate::server::ForwardedBearer>())
        .map(|bearer| bearer.0.clone())
        .ok_or_else(|| McpError::invalid_request("forwarded bearer missing", None))?;
    Ok(PlaneCaller {
        memberships: identity.actor.group_memberships(),
        identity,
        bearer_token,
    })
}

fn identity_has_scope(identity: &GatewayInternalIdentity, required: &str) -> bool {
    identity
        .actor
        .scopes
        .iter()
        .any(|scope| scope.as_str() == required)
}

fn is_subscribable(uri: &str) -> bool {
    matches!(uri, uris::SESSIONS | uris::CAPACITY)
        || uris::parse_session(uri).is_some()
        || uris::parse_pose_source(uri).is_some()
        || uris::parse_reconciliation(uri).is_some()
        || uris::parse_cameras(uri).is_some()
        || uris::parse_camera(uri).is_some()
        || uris::parse_streams(uri).is_some()
        || uris::parse_stream(uri).is_some()
}

fn structured_result<T: Serialize>(text: String, value: &T) -> Result<CallToolResult, McpError> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(text)]);
    result.structured_content = Some(serde_json::to_value(value).map_err(internal)?);
    Ok(result)
}

fn json_resource<T: Serialize + ?Sized>(
    uri: &str,
    value: &T,
) -> Result<ReadResourceResult, McpError> {
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(serde_json::to_string(value).map_err(internal)?, uri)
            .with_mime_type("application/json"),
    ]))
}

fn json_descriptor(uri: &str, title: &str, description: &str) -> Resource {
    Resource::new(uri.to_owned(), title.to_owned())
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

fn mcp_page<T>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    paginate(items, request, LIST_PAGE_SIZE).map_err(service_error)
}

fn service_error(error: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}

fn ensure_pose_authorization_current(session: &SimulationViewSession) -> Result<(), McpError> {
    let source = session.pose_source.as_ref().ok_or_else(|| {
        typed_runtime_error(
            "pose_producer_authorization_missing",
            "pose producer authorization is missing",
        )
    })?;
    if source.revoked {
        return Err(typed_runtime_error(
            "pose_producer_revoked",
            "pose producer authorization is revoked",
        ));
    }
    if source.expires_at <= chrono::Utc::now() {
        return Err(typed_runtime_error(
            "pose_producer_authorization_expired",
            "pose producer authorization expired",
        ));
    }
    if session.reconciliation.phase == crate::contract::ReconciliationPhase::Blocked {
        let code = session
            .reconciliation
            .failure_code
            .and_then(|code| serde_json::to_value(code).ok())
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_else(|| "reconciliation_blocked".to_owned());
        return Err(typed_runtime_error(
            &code,
            session
                .reconciliation
                .diagnostic
                .as_deref()
                .unwrap_or("Simulation View automatic reconciliation is blocked"),
        ));
    }
    Ok(())
}

fn typed_runtime_error(code: &str, message: &str) -> McpError {
    McpError::invalid_request(
        message.to_owned(),
        Some(serde_json::json!({
            "schemaVersion": "veoveo.io/simulation-view-error/v1",
            "code": code,
        })),
    )
}

fn resource_error(error: SimulationViewError) -> McpError {
    match error {
        SimulationViewError::SessionNotFound(_)
        | SimulationViewError::CameraNotFound(_)
        | SimulationViewError::LiveViewNotFound(_)
        | SimulationViewError::Ownership => not_found(),
        other => McpError::invalid_request(other.to_string(), None),
    }
}

fn not_found() -> McpError {
    McpError::resource_not_found("unknown Simulation View resource", None)
}

fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

fn runtime_error(error: impl std::fmt::Display) -> McpError {
    tracing::error!(%error, "Simulation View runtime transition failed");
    McpError::internal_error("Simulation View runtime transition failed", None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_the_bounded_control_plane() {
        let names = SimulationViewMcp::tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(names.len(), 13);
        for required in [
            "create_session",
            "close_session",
            "bind_scene",
            "authorize_pose_producer",
            "revoke_pose_producer",
            "create_camera",
            "set_camera",
            "close_camera",
            "open_live_view",
            "renew_live_view",
            "close_live_view",
            "get_capacity",
            "get_session_state",
        ] {
            assert!(names.contains(required));
        }
    }
}
