use std::{borrow::Cow, collections::BTreeMap, fmt, sync::Arc};

use rmcp::{
    ClientHandler, ServiceExt,
    handler::server::ServerHandler,
    model::{
        CallToolRequest, CallToolRequestParams, CallToolResult, CancelTaskParams,
        CancelTaskRequest, CancelTaskResult, ClientInfo, ClientRequest, CompleteRequestParams,
        CompleteResult, CreateTaskResult, ErrorData as McpError, GetPromptRequestParams,
        GetPromptResult, GetTaskParams, GetTaskPayloadParams, GetTaskPayloadRequest,
        GetTaskPayloadResult, GetTaskRequest, GetTaskResult, Implementation,
        InitializeRequestParams, InitializeResult, JsonObject, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, ListTasksRequest, ListTasksResult,
        ListToolsResult, Notification, PaginatedRequestParams, ReadResourceRequestParams,
        ReadResourceResult, Reference, ServerCapabilities, ServerInfo, ServerNotification,
        ServerResult, SubscribeRequestParams, TaskStatusNotificationParam, TasksCapability,
        UnsubscribeRequestParams,
    },
    service::{NotificationContext, Peer, RequestContext, RoleClient, RoleServer, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use tokio::sync::RwLock;
use veoveo_mcp_contract::{
    CompletionExposure, GatewayAction, GatewayProfileId, GatewayTaskId, GatewayToolName,
    LocalToolName, PolicyEffect, PolicyTarget, PromptName, ResourceUri, ServerSlug,
    TaskExposure as ContractTaskExposure, UpstreamTransport, paginate,
};

use crate::{AuthenticatedSubject, GatewayCatalog, PolicyRequest};

const GATEWAY_PAGE_SIZE: usize = 100;

#[derive(Debug)]
pub struct GatewayMcp {
    catalog: Arc<GatewayCatalog>,
    profile_id: GatewayProfileId,
    upstreams: RwLock<BTreeMap<ServerSlug, RunningService<RoleClient, GatewayUpstreamHandler>>>,
}

impl GatewayMcp {
    pub fn new(catalog: Arc<GatewayCatalog>, profile_id: GatewayProfileId) -> Self {
        Self {
            catalog,
            profile_id,
            upstreams: RwLock::new(BTreeMap::new()),
        }
    }

    fn profile_servers(&self) -> Vec<ServerSlug> {
        self.catalog
            .profile_servers(&self.profile_id)
            .into_iter()
            .map(|(_, server)| server.slug.clone())
            .collect()
    }

    fn profile_task_servers(&self) -> Vec<ServerSlug> {
        self.catalog
            .profile_servers(&self.profile_id)
            .into_iter()
            .filter(|(exposure, server)| {
                exposure.tasks == ContractTaskExposure::Enabled && server.capabilities.tasks
            })
            .map(|(_, server)| server.slug.clone())
            .collect()
    }

    async fn upstream(
        &self,
        server_slug: &ServerSlug,
        downstream: Peer<RoleServer>,
    ) -> Result<Peer<RoleClient>, McpError> {
        {
            let upstreams = self.upstreams.read().await;
            if let Some(running) = upstreams.get(server_slug)
                && !running.is_closed()
            {
                return Ok(running.peer().clone());
            }
        }

        let server = self
            .catalog
            .server(server_slug)
            .ok_or_else(|| mcp_invalid_params(format!("unknown upstream server `{server_slug}`")))?
            .clone();
        if server.upstream.transport != UpstreamTransport::StreamableHttp {
            return Err(mcp_internal(format!(
                "unsupported upstream transport for server `{server_slug}`"
            )));
        }

        let mut upstreams = self.upstreams.write().await;
        if let Some(running) = upstreams.get(server_slug)
            && !running.is_closed()
        {
            return Ok(running.peer().clone());
        }

        let transport = StreamableHttpClientTransport::from_config(
            StreamableHttpClientTransportConfig::with_uri(server.upstream.url.clone())
                .reinit_on_expired_session(false),
        );
        let handler = GatewayUpstreamHandler {
            upstream_server: server_slug.clone(),
            downstream,
        };
        let running = handler
            .serve(transport)
            .await
            .map_err(|err| mcp_internal(format!("failed to initialize upstream MCP: {err}")))?;
        let peer = running.peer().clone();
        upstreams.insert(server_slug.clone(), running);
        Ok(peer)
    }

    fn authenticated(
        &self,
        context: &RequestContext<RoleServer>,
    ) -> Result<AuthenticatedSubject, McpError> {
        let parts = context
            .extensions
            .get::<axum::http::request::Parts>()
            .ok_or_else(|| mcp_invalid_request("authenticated HTTP context missing"))?;
        parts
            .extensions
            .get::<AuthenticatedSubject>()
            .cloned()
            .ok_or_else(|| mcp_invalid_request("authenticated subject missing"))
    }

    fn authorize(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        target: PolicyTarget,
    ) -> Result<AuthenticatedSubject, McpError> {
        let subject = self.authenticated(context)?;
        let trace_id = veoveo_mcp_contract::TraceId::new(uuid::Uuid::new_v4().to_string())
            .map_err(|err| mcp_internal(format!("failed to create trace id: {err}")))?;
        let decision = self.catalog.decide(PolicyRequest {
            principal: &subject.principal,
            profile: &self.profile_id,
            action,
            target: &target,
            trace_id: &trace_id,
        });
        if decision.effect == PolicyEffect::Allow {
            Ok(subject)
        } else {
            tracing::warn!(
                profile = %self.profile_id,
                principal = %subject.principal.id,
                action = ?action,
                reason = ?decision.reason,
                "gateway policy denied MCP request"
            );
            Err(mcp_invalid_request(format!(
                "gateway policy denied request: {:?}",
                decision.reason
            )))
        }
    }

    fn authorize_tool(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        tool: LocalToolName,
    ) -> Result<AuthenticatedSubject, McpError> {
        self.authorize(context, action, PolicyTarget::Tool { server, tool })
    }

    fn authorize_resource(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        uri: &str,
    ) -> Result<AuthenticatedSubject, McpError> {
        let uri = ResourceUri::new(uri.to_string())
            .map_err(|err| mcp_invalid_params(format!("invalid resource URI: {err}")))?;
        let target = match resource_read_kind(uri.as_str()) {
            ResourceReadKind::Artifact => PolicyTarget::Artifact {
                server,
                artifact_uri: uri,
            },
            ResourceReadKind::Usage => PolicyTarget::Usage {
                server,
                usage_uri: uri,
            },
            ResourceReadKind::General => PolicyTarget::Resource { server, uri },
        };
        self.authorize(context, action, target)
    }

    fn authorize_prompt(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        prompt: PromptName,
    ) -> Result<AuthenticatedSubject, McpError> {
        self.authorize(context, action, PolicyTarget::Prompt { server, prompt })
    }

    fn authorize_task(
        &self,
        context: &RequestContext<RoleServer>,
        action: GatewayAction,
        server: ServerSlug,
        task_id: impl Into<String>,
    ) -> Result<AuthenticatedSubject, McpError> {
        let gateway_task_id = GatewayTaskId::new(task_id.into())
            .map_err(|err| mcp_invalid_params(format!("invalid task id: {err}")))?;
        self.authorize(
            context,
            action,
            PolicyTarget::Task {
                server,
                gateway_task_id,
            },
        )
    }

    fn server_for_resource(&self, uri: &str) -> Result<ServerSlug, McpError> {
        self.catalog
            .server_for_resource_uri(&self.profile_id, uri)
            .map(|(_, server)| server.slug.clone())
            .ok_or_else(|| mcp_invalid_params(format!("resource URI is not exposed: {uri}")))
    }

    fn server_for_prompt(&self, prompt: &str) -> Result<ServerSlug, McpError> {
        let prompt = PromptName::new(prompt.to_string())
            .map_err(|err| mcp_invalid_params(format!("invalid prompt name: {err}")))?;
        let matches = self.catalog.prompt_servers(&self.profile_id, &prompt);
        match matches.as_slice() {
            [(_, server)] => Ok(server.slug.clone()),
            [] => Err(mcp_invalid_params(format!(
                "prompt is not exposed: {prompt}"
            ))),
            _ => Err(mcp_internal(format!(
                "prompt `{prompt}` is ambiguous across profile servers"
            ))),
        }
    }

    fn single_task_server(&self) -> Result<ServerSlug, McpError> {
        let servers = self.profile_task_servers();
        match servers.as_slice() {
            [server] => Ok(server.clone()),
            [] => Err(mcp_invalid_request("profile does not expose MCP tasks")),
            _ => Err(mcp_invalid_request(
                "task routing requires durable gateway task mappings for multi-server profiles",
            )),
        }
    }
}

impl ServerHandler for GatewayMcp {
    fn get_info(&self) -> ServerInfo {
        let mut capabilities = ServerCapabilities::default();
        for (_, server) in self.catalog.profile_servers(&self.profile_id) {
            if server.capabilities.tools {
                capabilities.tools.get_or_insert_default();
            }
            if server.capabilities.prompts {
                capabilities.prompts.get_or_insert_default();
            }
            if server.capabilities.resources || server.capabilities.resource_templates {
                let resources = capabilities.resources.get_or_insert_default();
                if server.capabilities.resource_subscriptions {
                    resources.subscribe = Some(true);
                }
                if server.capabilities.notifications {
                    resources.list_changed = Some(true);
                }
            }
            if server.capabilities.completions {
                capabilities.completions.get_or_insert_with(JsonObject::new);
            }
            if server.capabilities.tasks {
                capabilities
                    .tasks
                    .get_or_insert_with(TasksCapability::server_default);
            }
        }

        let mut info = ServerInfo::default();
        info.capabilities = capabilities;
        info.server_info = Implementation::new("veoveo-gateway", env!("CARGO_PKG_VERSION"));
        info.instructions = Some(
            "Gateway profile for hosted Veoveo MCP servers. Tool names are gateway namespaced; resource URIs remain server-owned."
                .to_string(),
        );
        info
    }

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        self.authenticated(&context)?;
        context.peer.set_peer_info(request);
        Ok(self.get_info())
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = Vec::new();
        for server_slug in self.profile_servers() {
            let upstream = self.upstream(&server_slug, context.peer.clone()).await?;
            for mut tool in upstream.list_all_tools().await.map_err(upstream_error)? {
                let local_tool =
                    LocalToolName::new(tool.name.as_ref().to_string()).map_err(|err| {
                        mcp_internal(format!("upstream exposed invalid tool name: {err}"))
                    })?;
                if self
                    .authorize_tool(
                        &context,
                        GatewayAction::ToolsList,
                        server_slug.clone(),
                        local_tool.clone(),
                    )
                    .is_err()
                {
                    continue;
                }
                let gateway_name = self
                    .catalog
                    .project_tool_name(&server_slug, &local_tool)
                    .map_err(|err| mcp_internal(format!("failed to project tool name: {err}")))?;
                tool.name = Cow::Owned(gateway_name.to_string());
                tools.push(tool);
            }
        }
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        let page = paginate(tools, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListToolsResult {
            tools: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let projection = parse_gateway_tool(&self.catalog, &request.name)?;
        self.authorize_tool(
            &context,
            GatewayAction::ToolsCall,
            projection.server.clone(),
            projection.tool.clone(),
        )?;
        request.name = Cow::Owned(projection.tool.to_string());
        let upstream = self
            .upstream(&projection.server, context.peer.clone())
            .await?;
        upstream.call_tool(request).await.map_err(upstream_error)
    }

    async fn enqueue_task(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, McpError> {
        let projection = parse_gateway_tool(&self.catalog, &request.name)?;
        self.authorize_tool(
            &context,
            GatewayAction::ToolsCall,
            projection.server.clone(),
            projection.tool.clone(),
        )?;
        request.name = Cow::Owned(projection.tool.to_string());
        let upstream = self
            .upstream(&projection.server, context.peer.clone())
            .await?;
        let result = upstream
            .send_request(ClientRequest::CallToolRequest(CallToolRequest::new(
                request,
            )))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::CreateTaskResult(result) => Ok(result),
            other => Err(unexpected_upstream_response("tools/call task", other)),
        }
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let mut resources = Vec::new();
        for server_slug in self.profile_servers() {
            let upstream = self.upstream(&server_slug, context.peer.clone()).await?;
            for resource in upstream
                .list_all_resources()
                .await
                .map_err(upstream_error)?
            {
                if self
                    .authorize_resource(
                        &context,
                        GatewayAction::ResourcesList,
                        server_slug.clone(),
                        &resource.uri,
                    )
                    .is_err()
                {
                    continue;
                }
                resources.push(resource);
            }
        }
        resources.sort_by(|left, right| left.uri.cmp(&right.uri));
        let page = paginate(resources, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListResourcesResult {
            resources: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        let mut templates = Vec::new();
        for server_slug in self.profile_servers() {
            let upstream = self.upstream(&server_slug, context.peer.clone()).await?;
            for template in upstream
                .list_all_resource_templates()
                .await
                .map_err(upstream_error)?
            {
                if self
                    .authorize_resource(
                        &context,
                        GatewayAction::ResourcesTemplatesList,
                        server_slug.clone(),
                        &template.uri_template,
                    )
                    .is_err()
                {
                    continue;
                }
                templates.push(template);
            }
        }
        templates.sort_by(|left, right| left.uri_template.cmp(&right.uri_template));
        let page = paginate(templates, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
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
        let server = self.server_for_resource(&request.uri)?;
        self.authorize_resource(
            &context,
            resource_read_action(&request.uri),
            server.clone(),
            &request.uri,
        )?;
        let upstream = self.upstream(&server, context.peer.clone()).await?;
        upstream
            .read_resource(request)
            .await
            .map_err(upstream_error)
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let server = self.server_for_resource(&request.uri)?;
        self.authorize_resource(
            &context,
            GatewayAction::ResourcesSubscribe,
            server.clone(),
            &request.uri,
        )?;
        let upstream = self.upstream(&server, context.peer.clone()).await?;
        upstream.subscribe(request).await.map_err(upstream_error)
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let server = self.server_for_resource(&request.uri)?;
        self.authorize_resource(
            &context,
            GatewayAction::ResourcesSubscribe,
            server.clone(),
            &request.uri,
        )?;
        let upstream = self.upstream(&server, context.peer.clone()).await?;
        upstream.unsubscribe(request).await.map_err(upstream_error)
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let mut prompts = Vec::new();
        for server_slug in self.profile_servers() {
            let upstream = self.upstream(&server_slug, context.peer.clone()).await?;
            for prompt in upstream.list_all_prompts().await.map_err(upstream_error)? {
                let prompt_name = PromptName::new(prompt.name.clone()).map_err(|err| {
                    mcp_internal(format!("upstream exposed invalid prompt name: {err}"))
                })?;
                if self
                    .authorize_prompt(
                        &context,
                        GatewayAction::PromptsList,
                        server_slug.clone(),
                        prompt_name,
                    )
                    .is_err()
                {
                    continue;
                }
                prompts.push(prompt);
            }
        }
        ensure_unique_prompts(&prompts)?;
        prompts.sort_by(|left, right| left.name.cmp(&right.name));
        let page = paginate(prompts, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListPromptsResult {
            prompts: page.items,
            next_cursor: page.next_cursor,
            meta: None,
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        let server = self.server_for_prompt(&request.name)?;
        let prompt = PromptName::new(request.name.clone())
            .map_err(|err| mcp_invalid_params(format!("invalid prompt name: {err}")))?;
        self.authorize_prompt(&context, GatewayAction::PromptsGet, server.clone(), prompt)?;
        let upstream = self.upstream(&server, context.peer.clone()).await?;
        upstream.get_prompt(request).await.map_err(upstream_error)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let server = match &request.r#ref {
            Reference::Resource(reference) => self.server_for_resource(&reference.uri)?,
            Reference::Prompt(reference) => self.server_for_prompt(&reference.name)?,
            _ => return Err(mcp_invalid_params("unsupported completion reference kind")),
        };
        let (_profile, exposure, manifest) = self
            .catalog
            .profile_server(&self.profile_id, &server)
            .ok_or_else(|| mcp_invalid_params(format!("server `{server}` is not exposed")))?;
        if exposure.completions != CompletionExposure::Enabled || !manifest.capabilities.completions
        {
            return Err(mcp_invalid_request("profile does not expose completions"));
        }
        let target = match &request.r#ref {
            Reference::Resource(reference) => {
                let uri = ResourceUri::new(reference.uri.clone())
                    .map_err(|err| mcp_invalid_params(format!("invalid completion URI: {err}")))?;
                PolicyTarget::Resource {
                    server: server.clone(),
                    uri,
                }
            }
            Reference::Prompt(reference) => {
                let prompt = PromptName::new(reference.name.clone()).map_err(|err| {
                    mcp_invalid_params(format!("invalid completion prompt: {err}"))
                })?;
                PolicyTarget::Prompt {
                    server: server.clone(),
                    prompt,
                }
            }
            _ => return Err(mcp_invalid_params("unsupported completion reference kind")),
        };
        self.authorize(&context, GatewayAction::CompletionComplete, target)?;
        let upstream = self.upstream(&server, context.peer.clone()).await?;
        upstream.complete(request).await.map_err(upstream_error)
    }

    async fn list_tasks(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, McpError> {
        let server = self.single_task_server()?;
        self.authorize_task(&context, GatewayAction::TasksList, server.clone(), "list")?;
        let upstream = self.upstream(&server, context.peer.clone()).await?;
        let result = upstream
            .send_request(ClientRequest::ListTasksRequest(match request {
                Some(params) => ListTasksRequest::with_param(params),
                None => ListTasksRequest::default(),
            }))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::ListTasksResult(result) => Ok(result),
            other => Err(unexpected_upstream_response("tasks/list", other)),
        }
    }

    async fn get_task_info(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let server = self.single_task_server()?;
        self.authorize_task(
            &context,
            GatewayAction::TasksGet,
            server.clone(),
            request.task_id.clone(),
        )?;
        let upstream = self.upstream(&server, context.peer.clone()).await?;
        let result = upstream
            .send_request(ClientRequest::GetTaskRequest(GetTaskRequest::new(request)))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::GetTaskResult(result) => Ok(result),
            other => Err(unexpected_upstream_response("tasks/get", other)),
        }
    }

    async fn get_task_result(
        &self,
        request: GetTaskPayloadParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, McpError> {
        let server = self.single_task_server()?;
        self.authorize_task(
            &context,
            GatewayAction::TasksResult,
            server.clone(),
            request.task_id.clone(),
        )?;
        let upstream = self.upstream(&server, context.peer.clone()).await?;
        let result = upstream
            .send_request(ClientRequest::GetTaskPayloadRequest(
                GetTaskPayloadRequest::new(request),
            ))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::GetTaskPayloadResult(result) => Ok(result),
            ServerResult::CustomResult(result) => Ok(GetTaskPayloadResult::new(result.0)),
            other => Err(unexpected_upstream_response("tasks/result", other)),
        }
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, McpError> {
        let server = self.single_task_server()?;
        self.authorize_task(
            &context,
            GatewayAction::TasksCancel,
            server.clone(),
            request.task_id.clone(),
        )?;
        let upstream = self.upstream(&server, context.peer.clone()).await?;
        let result = upstream
            .send_request(ClientRequest::CancelTaskRequest(CancelTaskRequest::new(
                request,
            )))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::CancelTaskResult(result) => Ok(result),
            other => Err(unexpected_upstream_response("tasks/cancel", other)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GatewayUpstreamHandler {
    upstream_server: ServerSlug,
    downstream: Peer<RoleServer>,
}

impl ClientHandler for GatewayUpstreamHandler {
    fn get_info(&self) -> ClientInfo {
        let mut info = ClientInfo::default();
        info.client_info =
            Implementation::new("veoveo-gateway-upstream", env!("CARGO_PKG_VERSION"));
        info
    }

    async fn on_progress(
        &self,
        params: rmcp::model::ProgressNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        if let Err(err) = self.downstream.notify_progress(params).await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward progress notification: {err}"
            );
        }
    }

    async fn on_resource_updated(
        &self,
        params: rmcp::model::ResourceUpdatedNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        if let Err(err) = self.downstream.notify_resource_updated(params).await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward resource update notification: {err}"
            );
        }
    }

    async fn on_resource_list_changed(&self, _context: NotificationContext<RoleClient>) {
        if let Err(err) = self.downstream.notify_resource_list_changed().await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward resource list notification: {err}"
            );
        }
    }

    async fn on_tool_list_changed(&self, _context: NotificationContext<RoleClient>) {
        if let Err(err) = self.downstream.notify_tool_list_changed().await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward tool list notification: {err}"
            );
        }
    }

    async fn on_prompt_list_changed(&self, _context: NotificationContext<RoleClient>) {
        if let Err(err) = self.downstream.notify_prompt_list_changed().await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward prompt list notification: {err}"
            );
        }
    }

    async fn on_task_status(
        &self,
        params: TaskStatusNotificationParam,
        _context: NotificationContext<RoleClient>,
    ) {
        let notification = ServerNotification::TaskStatusNotification(Notification::new(params));
        if let Err(err) = self.downstream.send_notification(notification).await {
            tracing::warn!(
                upstream_server = %self.upstream_server,
                "failed to forward task status notification: {err}"
            );
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResourceReadKind {
    General,
    Artifact,
    Usage,
}

fn resource_read_kind(uri: &str) -> ResourceReadKind {
    let Some((_, path)) = uri.split_once("://") else {
        return ResourceReadKind::General;
    };
    if path.starts_with("artifact/") {
        ResourceReadKind::Artifact
    } else if path.starts_with("usage/") {
        ResourceReadKind::Usage
    } else {
        ResourceReadKind::General
    }
}

fn resource_read_action(uri: &str) -> GatewayAction {
    match resource_read_kind(uri) {
        ResourceReadKind::Artifact => GatewayAction::ArtifactRead,
        ResourceReadKind::Usage => GatewayAction::UsageRead,
        ResourceReadKind::General => GatewayAction::ResourcesRead,
    }
}

fn parse_gateway_tool(
    catalog: &GatewayCatalog,
    name: &str,
) -> Result<crate::GatewayToolProjection, McpError> {
    let gateway_name = GatewayToolName::new(name.to_string())
        .map_err(|err| mcp_invalid_params(format!("invalid gateway tool name: {err}")))?;
    catalog
        .parse_tool_name(&gateway_name)
        .map_err(|err| mcp_invalid_params(err.to_string()))
}

fn ensure_unique_prompts(prompts: &[rmcp::model::Prompt]) -> Result<(), McpError> {
    let mut seen = std::collections::BTreeSet::<&str>::new();
    for prompt in prompts {
        if !seen.insert(prompt.name.as_str()) {
            return Err(mcp_internal(format!(
                "prompt `{}` is ambiguous across profile servers",
                prompt.name
            )));
        }
    }
    Ok(())
}

fn upstream_error(err: impl fmt::Display) -> McpError {
    mcp_internal(format!("upstream MCP request failed: {err}"))
}

fn unexpected_upstream_response(method: &str, response: ServerResult) -> McpError {
    mcp_internal(format!(
        "upstream returned unexpected response for {method}: {}",
        server_result_name(&response)
    ))
}

fn server_result_name(result: &ServerResult) -> &'static str {
    match result {
        ServerResult::InitializeResult(_) => "initialize",
        ServerResult::CompleteResult(_) => "complete",
        ServerResult::GetPromptResult(_) => "get_prompt",
        ServerResult::ListPromptsResult(_) => "list_prompts",
        ServerResult::ListResourcesResult(_) => "list_resources",
        ServerResult::ListResourceTemplatesResult(_) => "list_resource_templates",
        ServerResult::ReadResourceResult(_) => "read_resource",
        ServerResult::ListToolsResult(_) => "list_tools",
        ServerResult::ElicitResult(_) => "elicit",
        ServerResult::CreateTaskResult(_) => "create_task",
        ServerResult::ListTasksResult(_) => "list_tasks",
        ServerResult::GetTaskResult(_) => "get_task",
        ServerResult::CancelTaskResult(_) => "cancel_task",
        ServerResult::CallToolResult(_) => "call_tool",
        ServerResult::GetTaskPayloadResult(_) => "get_task_payload",
        ServerResult::EmptyResult(_) => "empty",
        ServerResult::CustomResult(_) => "custom",
    }
}

fn mcp_invalid_request(message: impl Into<String>) -> McpError {
    McpError::invalid_request(message.into(), None)
}

fn mcp_invalid_params(message: impl Into<String>) -> McpError {
    McpError::invalid_params(message.into(), None)
}

fn mcp_internal(message: impl Into<String>) -> McpError {
    McpError::internal_error(message.into(), None)
}
