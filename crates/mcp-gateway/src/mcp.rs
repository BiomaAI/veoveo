use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

mod authorization;
mod upstream;
mod upstream_http;

use chrono::{DateTime, TimeDelta, Utc};
use rmcp::{
    ServiceExt,
    handler::server::ServerHandler,
    model::{
        CallToolRequest, CallToolRequestParams, CallToolResult, CancelTaskParams,
        CancelTaskRequest, CancelTaskResult, ClientRequest, CompleteRequestParams, CompleteResult,
        CreateTaskResult, ErrorData as McpError, ExtensionCapabilities, GetPromptRequestParams,
        GetPromptResult, GetTaskParams, GetTaskPayloadParams, GetTaskPayloadRequest,
        GetTaskPayloadResult, GetTaskRequest, GetTaskResult, Implementation,
        InitializeRequestParams, InitializeResult, JsonObject, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, ListTasksResult, ListToolsResult,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult, Reference,
        ServerCapabilities, ServerInfo, ServerResult, SubscribeRequestParams, TasksCapability,
        UnsubscribeRequestParams,
    },
    service::{Peer, RequestContext, RoleClient, RoleServer, RunningService},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use tokio::sync::RwLock;
use veoveo_mcp_contract::{
    CompletionExposure, GatewayAction, GatewayInternalTokenIssuer, GatewayProfileId,
    GatewayResourceSubscription, GatewayTaskId, GatewayTaskMapping, LocalToolName,
    PolicyReasonCode, PolicyTarget, PrincipalId, PromptName, ResourceUri, ServerSlug,
    TaskExposure as ContractTaskExposure, UpstreamTaskId, UpstreamTransport, paginate,
};

use crate::{
    AuthenticatedSubject, GatewayCatalogHandle, GatewayState,
    mcp_support::{
        ensure_unique_prompts, mcp_internal, mcp_invalid_params, mcp_invalid_request,
        parse_gateway_tool, project_listed_resource, project_read_resource_result,
        project_task_payload_result, resource_policy_target, resource_read_action,
        unexpected_upstream_response, upstream_error,
    },
};
use upstream::GatewayUpstreamHandler;
use upstream_http::build_upstream_http_client;

const GATEWAY_PAGE_SIZE: usize = 100;
const INTERNAL_TOKEN_TTL_SECONDS: i64 = 15 * 60;
const INTERNAL_TOKEN_REFRESH_WINDOW_SECONDS: i64 = 30;

#[derive(Debug)]
pub struct GatewayMcp {
    catalog: GatewayCatalogHandle,
    state: GatewayState,
    profile_id: GatewayProfileId,
    internal_token_issuer: GatewayInternalTokenIssuer,
    upstreams: RwLock<BTreeMap<UpstreamCacheKey, UpstreamConnection>>,
}

impl GatewayMcp {
    pub fn new(
        catalog: GatewayCatalogHandle,
        profile_id: GatewayProfileId,
        state: GatewayState,
        internal_token_issuer: GatewayInternalTokenIssuer,
    ) -> Self {
        Self {
            catalog,
            state,
            profile_id,
            internal_token_issuer,
            upstreams: RwLock::new(BTreeMap::new()),
        }
    }

    fn profile_servers(&self) -> Vec<ServerSlug> {
        self.catalog
            .current()
            .profile_servers(&self.profile_id)
            .into_iter()
            .map(|(_, server)| server.slug.clone())
            .collect()
    }

    fn profile_task_servers(&self) -> Vec<ServerSlug> {
        self.catalog
            .current()
            .profile_servers(&self.profile_id)
            .into_iter()
            .filter(|(exposure, server)| {
                exposure.tasks == ContractTaskExposure::Enabled && server.capabilities.tasks
            })
            .map(|(_, server)| server.slug.clone())
            .collect()
    }

    fn auth_extension_capabilities(&self) -> Option<ExtensionCapabilities> {
        let catalog = self.catalog.current();
        let profile = catalog.profile(&self.profile_id)?;
        let extensions: ExtensionCapabilities = profile
            .auth_modes
            .iter()
            .filter_map(|mode| mode.mcp_extension_id())
            .map(|extension| (extension.to_string(), JsonObject::new()))
            .collect();
        if extensions.is_empty() {
            None
        } else {
            Some(extensions)
        }
    }

    async fn upstream(
        &self,
        server_slug: &ServerSlug,
        downstream: Peer<RoleServer>,
        subject: &AuthenticatedSubject,
    ) -> Result<Peer<RoleClient>, McpError> {
        let snapshot = self.catalog.snapshot();
        let catalog_generation = snapshot.generation();
        let key = UpstreamCacheKey {
            server: server_slug.clone(),
            principal: subject.principal.id.clone(),
            catalog_generation,
        };
        let refresh_after = Utc::now() + TimeDelta::seconds(INTERNAL_TOKEN_REFRESH_WINDOW_SECONDS);
        {
            let upstreams = self.upstreams.read().await;
            if let Some(connection) = upstreams.get(&key)
                && !connection.running.is_closed()
                && connection.expires_at > refresh_after
            {
                return Ok(connection.running.peer().clone());
            }
        }

        let server = snapshot
            .catalog()
            .server(server_slug)
            .ok_or_else(|| mcp_invalid_params(format!("unknown upstream server `{server_slug}`")))?
            .clone();
        if server.upstream.transport != UpstreamTransport::StreamableHttp {
            return Err(mcp_internal(format!(
                "unsupported upstream transport for server `{server_slug}`"
            )));
        }

        let mut upstreams = self.upstreams.write().await;
        if let Some(connection) = upstreams.get(&key)
            && !connection.running.is_closed()
            && connection.expires_at > refresh_after
        {
            return Ok(connection.running.peer().clone());
        }
        upstreams.remove(&key);

        let token_expires_at = internal_token_expires_at(subject)?;
        let internal_token = self
            .internal_token_issuer
            .issue(
                self.profile_id.clone(),
                server_slug.clone(),
                subject.principal.clone(),
                token_expires_at,
            )
            .map_err(|err| mcp_internal(format!("failed to issue internal token: {err}")))?;

        let http_client = build_upstream_http_client(snapshot.catalog(), &server).await?;
        let transport = StreamableHttpClientTransport::<reqwest::Client>::with_client(
            http_client,
            StreamableHttpClientTransportConfig::with_uri(server.upstream.url.as_str().to_string())
                .auth_header(internal_token.bearer_token)
                .reinit_on_expired_session(false),
        );
        let handler = GatewayUpstreamHandler::new(
            self.profile_id.clone(),
            subject.principal.id.clone(),
            server_slug.clone(),
            self.state.clone(),
            downstream,
        );
        let running = handler
            .serve(transport)
            .await
            .map_err(|err| mcp_internal(format!("failed to initialize upstream MCP: {err}")))?;
        let peer = running.peer().clone();
        upstreams.insert(
            key,
            UpstreamConnection {
                running,
                expires_at: internal_token.identity.expires_at,
            },
        );
        Ok(peer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct UpstreamCacheKey {
    server: ServerSlug,
    principal: PrincipalId,
    catalog_generation: u64,
}

#[derive(Debug)]
struct UpstreamConnection {
    running: RunningService<RoleClient, GatewayUpstreamHandler>,
    expires_at: DateTime<Utc>,
}

fn internal_token_expires_at(subject: &AuthenticatedSubject) -> Result<DateTime<Utc>, McpError> {
    let now = Utc::now();
    let max_expires_at = now + TimeDelta::seconds(INTERNAL_TOKEN_TTL_SECONDS);
    let expires_at = std::cmp::min(subject.access_token.expires_at, max_expires_at);
    if expires_at <= now {
        return Err(mcp_invalid_request(
            "authenticated access token is already expired",
        ));
    }
    Ok(expires_at)
}

impl ServerHandler for GatewayMcp {
    fn get_info(&self) -> ServerInfo {
        let mut capabilities = ServerCapabilities::default();
        let catalog = self.catalog.current();
        for (_, server) in catalog.profile_servers(&self.profile_id) {
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
        capabilities.extensions = self.auth_extension_capabilities();

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
        let subject = self.authenticated(&context)?;
        let mut tools = Vec::new();
        for server_slug in self.profile_servers() {
            let upstream = self
                .upstream(&server_slug, context.peer.clone(), &subject)
                .await?;
            for mut tool in upstream.list_all_tools().await.map_err(upstream_error)? {
                let local_tool =
                    LocalToolName::new(tool.name.as_ref().to_string()).map_err(|err| {
                        mcp_internal(format!("upstream exposed invalid tool name: {err}"))
                    })?;
                if !self.allows_tool(
                    &context,
                    GatewayAction::ToolsList,
                    server_slug.clone(),
                    local_tool.clone(),
                )? {
                    continue;
                }
                let gateway_name = self
                    .catalog
                    .current()
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
        let catalog = self.catalog.current();
        let projection = parse_gateway_tool(&catalog, &request.name)?;
        let subject = self.authorize_tool(
            &context,
            GatewayAction::ToolsCall,
            projection.server.clone(),
            projection.tool.clone(),
        )?;
        request.name = Cow::Owned(projection.tool.to_string());
        let upstream = self
            .upstream(&projection.server, context.peer.clone(), &subject)
            .await?;
        upstream.call_tool(request).await.map_err(upstream_error)
    }

    async fn enqueue_task(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, McpError> {
        let catalog = self.catalog.current();
        let projection = parse_gateway_tool(&catalog, &request.name)?;
        let subject = self.authorize_tool(
            &context,
            GatewayAction::ToolsCall,
            projection.server.clone(),
            projection.tool.clone(),
        )?;
        request.name = Cow::Owned(projection.tool.to_string());
        let upstream = self
            .upstream(&projection.server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::CallToolRequest(CallToolRequest::new(
                request,
            )))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::CreateTaskResult(mut result) => {
                let upstream_task_id =
                    UpstreamTaskId::new(result.task.task_id.clone()).map_err(|err| {
                        mcp_internal(format!("upstream returned invalid task id: {err}"))
                    })?;
                let gateway_task_id = GatewayTaskId::new(uuid::Uuid::new_v4().to_string())
                    .map_err(|err| {
                        mcp_internal(format!("failed to create gateway task id: {err}"))
                    })?;
                let now = chrono::Utc::now();
                self.state
                    .record_task_mapping(&GatewayTaskMapping {
                        gateway_task_id: gateway_task_id.clone(),
                        upstream_server: projection.server.clone(),
                        upstream_task_id,
                        profile: self.profile_id.clone(),
                        owner: subject.principal.id.clone(),
                        created_at: now,
                        updated_at: now,
                    })
                    .map_err(|err| {
                        mcp_internal(format!("failed to persist gateway task mapping: {err}"))
                    })?;
                result.task.task_id = gateway_task_id.to_string();
                Ok(result)
            }
            other => Err(unexpected_upstream_response("tools/call task", other)),
        }
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let subject = self.authenticated(&context)?;
        let mut resources = Vec::new();
        for server_slug in self.profile_servers() {
            let upstream = self
                .upstream(&server_slug, context.peer.clone(), &subject)
                .await?;
            for mut resource in upstream
                .list_all_resources()
                .await
                .map_err(upstream_error)?
            {
                let Some(projection) = self.project_upstream_resource_for_owner(
                    &server_slug,
                    &subject.principal.id,
                    &resource.uri,
                )?
                else {
                    continue;
                };
                project_listed_resource(&mut resource, &projection);
                if !self.allows_resource(
                    &context,
                    GatewayAction::ResourcesList,
                    projection.server.clone(),
                    &resource.uri,
                )? {
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
        let subject = self.authenticated(&context)?;
        let mut templates = Vec::new();
        for server_slug in self.profile_servers() {
            let upstream = self
                .upstream(&server_slug, context.peer.clone(), &subject)
                .await?;
            for template in upstream
                .list_all_resource_templates()
                .await
                .map_err(upstream_error)?
            {
                if !self.allows_resource(
                    &context,
                    GatewayAction::ResourcesTemplatesList,
                    server_slug.clone(),
                    &template.uri_template,
                )? {
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
        mut request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        let projection = self.project_resource_for_upstream(&request.uri)?;
        let subject = self.authorize_projected_resource(
            &context,
            resource_read_action(&request.uri),
            &projection,
        )?;
        request.uri = projection.upstream_uri.to_string();
        let upstream = self
            .upstream(&projection.server, context.peer.clone(), &subject)
            .await?;
        let mut result = upstream
            .read_resource(request)
            .await
            .map_err(upstream_error)?;
        project_read_resource_result(&mut result, &projection)?;
        Ok(result)
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let mut request = request;
        let uri = request.uri.clone();
        let projection = self.project_resource_for_upstream(&uri)?;
        let resource_uri = projection.gateway_uri.clone();
        let subject = self.authorize_projected_resource(
            &context,
            GatewayAction::ResourcesSubscribe,
            &projection,
        )?;
        request.uri = projection.upstream_uri.to_string();
        let upstream = self
            .upstream(&projection.server, context.peer.clone(), &subject)
            .await?;
        upstream.subscribe(request).await.map_err(upstream_error)?;
        let now = Utc::now();
        self.state
            .record_resource_subscription(&GatewayResourceSubscription {
                profile: self.profile_id.clone(),
                owner: subject.principal.id,
                upstream_server: projection.server,
                resource_uri,
                created_at: now,
                updated_at: now,
            })
            .map_err(|err| {
                mcp_internal(format!(
                    "failed to persist gateway resource subscription: {err}"
                ))
            })?;
        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        let mut request = request;
        let uri = request.uri.clone();
        let projection = self.project_resource_for_upstream(&uri)?;
        let resource_uri = projection.gateway_uri.clone();
        let server = projection.server.clone();
        let subject = self.authenticated(&context)?;
        let subscription = self
            .state
            .resource_subscription(
                &self.profile_id,
                &subject.principal.id,
                &server,
                &resource_uri,
            )
            .map_err(|err| {
                mcp_internal(format!(
                    "failed to read gateway resource subscription: {err}"
                ))
            })?;
        if subscription.is_none() {
            self.record_policy_denial(
                &subject,
                GatewayAction::ResourcesUnsubscribe,
                resource_policy_target(server.clone(), resource_uri.as_str())?,
                PolicyReasonCode::UnknownResource,
            )?;
            return Err(mcp_invalid_params("unknown gateway resource subscription"));
        }
        let subject = self.authorize_projected_resource(
            &context,
            GatewayAction::ResourcesUnsubscribe,
            &projection,
        )?;
        request.uri = projection.upstream_uri.to_string();
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        upstream
            .unsubscribe(request)
            .await
            .map_err(upstream_error)?;
        self.state
            .delete_resource_subscription(
                &self.profile_id,
                &subject.principal.id,
                &server,
                &resource_uri,
            )
            .map_err(|err| {
                mcp_internal(format!(
                    "failed to delete gateway resource subscription: {err}"
                ))
            })?;
        Ok(())
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let subject = self.authenticated(&context)?;
        let mut prompts = Vec::new();
        for server_slug in self.profile_servers() {
            let upstream = self
                .upstream(&server_slug, context.peer.clone(), &subject)
                .await?;
            for prompt in upstream.list_all_prompts().await.map_err(upstream_error)? {
                let prompt_name = PromptName::new(prompt.name.clone()).map_err(|err| {
                    mcp_internal(format!("upstream exposed invalid prompt name: {err}"))
                })?;
                if !self.allows_prompt(
                    &context,
                    GatewayAction::PromptsList,
                    server_slug.clone(),
                    prompt_name,
                )? {
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
        let subject =
            self.authorize_prompt(&context, GatewayAction::PromptsGet, server.clone(), prompt)?;
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
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
        let catalog = self.catalog.current();
        let (_profile, exposure, manifest) = catalog
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
        let subject = self.authorize(&context, GatewayAction::CompletionComplete, target)?;
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        upstream.complete(request).await.map_err(upstream_error)
    }

    async fn list_tasks(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, McpError> {
        let subject = self.authenticated(&context)?;
        let task_servers = self
            .profile_task_servers()
            .into_iter()
            .collect::<BTreeSet<_>>();
        if task_servers.is_empty() {
            return Err(mcp_invalid_request("profile does not expose MCP tasks"));
        }
        let mut allowed_task_servers = BTreeSet::new();
        for server in task_servers {
            let allowed = self.allows(
                &context,
                GatewayAction::TasksList,
                PolicyTarget::TaskList {
                    server: server.clone(),
                },
            )?;
            if allowed {
                allowed_task_servers.insert(server);
            }
        }

        let all_mappings = self
            .state
            .task_mappings_for_profile_owner(&self.profile_id, &subject.principal.id)
            .map_err(|err| mcp_internal(format!("failed to read gateway task mappings: {err}")))?;
        let mut mappings = Vec::new();
        for mapping in all_mappings {
            if !allowed_task_servers.contains(&mapping.upstream_server) {
                continue;
            }
            mappings.push(mapping);
        }

        let page = paginate(mappings, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        let mut tasks = Vec::with_capacity(page.items.len());
        for mapping in page.items {
            let server = mapping.upstream_server.clone();
            let upstream = self
                .upstream(&server, context.peer.clone(), &subject)
                .await?;
            let result = upstream
                .send_request(ClientRequest::GetTaskRequest(GetTaskRequest::new(
                    GetTaskParams::new(mapping.upstream_task_id.to_string()),
                )))
                .await
                .map_err(upstream_error)?;
            match result {
                ServerResult::GetTaskResult(mut result) => {
                    result.task.task_id = mapping.gateway_task_id.to_string();
                    tasks.push(result.task);
                }
                other => return Err(unexpected_upstream_response("tasks/get", other)),
            }
        }

        let mut result = ListTasksResult::new(tasks);
        result.next_cursor = page.next_cursor;
        Ok(result)
    }

    async fn get_task_info(
        &self,
        mut request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        let mapping = self.task_mapping(&request.task_id)?;
        let server = mapping.upstream_server.clone();
        let subject = self.authorize_mapped_task(&context, GatewayAction::TasksGet, &mapping)?;
        request.task_id = mapping.upstream_task_id.to_string();
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::GetTaskRequest(GetTaskRequest::new(request)))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::GetTaskResult(mut result) => {
                result.task.task_id = mapping.gateway_task_id.to_string();
                Ok(result)
            }
            other => Err(unexpected_upstream_response("tasks/get", other)),
        }
    }

    async fn get_task_result(
        &self,
        mut request: GetTaskPayloadParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, McpError> {
        let mapping = self.task_mapping(&request.task_id)?;
        let server = mapping.upstream_server.clone();
        let subject = self.authorize_mapped_task(&context, GatewayAction::TasksResult, &mapping)?;
        request.task_id = mapping.upstream_task_id.to_string();
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::GetTaskPayloadRequest(
                GetTaskPayloadRequest::new(request),
            ))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::GetTaskPayloadResult(mut result) => {
                project_task_payload_result(&mut result, &mapping)?;
                Ok(result)
            }
            ServerResult::CallToolResult(result) => {
                let payload = serde_json::to_value(result)
                    .map_err(|err| mcp_internal(format!("failed to encode task payload: {err}")))?;
                let mut result = GetTaskPayloadResult::new(payload);
                project_task_payload_result(&mut result, &mapping)?;
                Ok(result)
            }
            ServerResult::CustomResult(result) => {
                let mut result = GetTaskPayloadResult::new(result.0);
                project_task_payload_result(&mut result, &mapping)?;
                Ok(result)
            }
            other => Err(unexpected_upstream_response("tasks/result", other)),
        }
    }

    async fn cancel_task(
        &self,
        mut request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, McpError> {
        let mapping = self.task_mapping(&request.task_id)?;
        let server = mapping.upstream_server.clone();
        let subject = self.authorize_mapped_task(&context, GatewayAction::TasksCancel, &mapping)?;
        request.task_id = mapping.upstream_task_id.to_string();
        let upstream = self
            .upstream(&server, context.peer.clone(), &subject)
            .await?;
        let result = upstream
            .send_request(ClientRequest::CancelTaskRequest(CancelTaskRequest::new(
                request,
            )))
            .await
            .map_err(upstream_error)?;
        match result {
            ServerResult::CancelTaskResult(mut result) => {
                result.task.task_id = mapping.gateway_task_id.to_string();
                Ok(result)
            }
            ServerResult::GetTaskResult(upstream_result) => {
                let mut task = upstream_result.task;
                task.task_id = mapping.gateway_task_id.to_string();
                let mut result = CancelTaskResult::new(task);
                result.meta = upstream_result.meta;
                Ok(result)
            }
            other => Err(unexpected_upstream_response("tasks/cancel", other)),
        }
    }
}
