mod authorization;
mod completion;
mod info;
mod prompts;
mod resources;
mod tasks;
mod tools;
mod upstream;
mod upstream_cache;
mod upstream_http;

use chrono::{DateTime, TimeDelta, Utc};
use rmcp::{
    ServiceExt,
    handler::server::ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, CancelTaskParams, CancelTaskResult,
        CompleteRequestParams, CompleteResult, CreateTaskResult, ErrorData as McpError,
        GetPromptRequestParams, GetPromptResult, GetTaskParams, GetTaskPayloadParams,
        GetTaskPayloadResult, GetTaskResult, InitializeRequestParams, InitializeResult,
        ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult, ListTasksResult,
        ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult,
        ServerInfo, SubscribeRequestParams, UnsubscribeRequestParams,
    },
    service::{Peer, RequestContext, RoleClient, RoleServer},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use veoveo_mcp_contract::{
    GatewayInternalTokenIssuer, GatewayProfileId, ServerSlug, UpstreamTransport,
};

use crate::{
    AuthenticatedSubject, GatewayCatalogHandle, GatewayState,
    mcp_support::{mcp_internal, mcp_invalid_params, mcp_invalid_request},
};
use upstream::GatewayUpstreamHandler;
use upstream_cache::{UpstreamCacheKey, UpstreamConnection, UpstreamConnectionCache};
use upstream_http::build_upstream_http_client;

pub(super) const GATEWAY_PAGE_SIZE: usize = 100;
const INTERNAL_TOKEN_TTL_SECONDS: i64 = 15 * 60;
const INTERNAL_TOKEN_REFRESH_WINDOW_SECONDS: i64 = 30;

#[derive(Debug)]
pub struct GatewayMcp {
    catalog: GatewayCatalogHandle,
    state: GatewayState,
    profile_id: GatewayProfileId,
    internal_token_issuer: GatewayInternalTokenIssuer,
    upstreams: UpstreamConnectionCache,
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
            upstreams: UpstreamConnectionCache::new(),
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
        self.upstreams.close_stale(catalog_generation).await;
        if let Some(peer) = self.upstreams.reusable_peer(&key, refresh_after).await {
            return Ok(peer);
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

        if let Some(peer) = self.upstreams.reusable_peer(&key, refresh_after).await {
            return Ok(peer);
        }
        self.upstreams
            .close_if_not_reusable(&key, refresh_after, "expired or closed upstream connection")
            .await;

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
        Ok(self
            .upstreams
            .insert_or_reuse(
                key,
                UpstreamConnection {
                    running,
                    expires_at: internal_token.identity.expires_at,
                },
                refresh_after,
            )
            .await)
    }
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
        self.handle_get_info()
    }

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, McpError> {
        self.handle_initialize(request, context).await
    }

    async fn list_tools(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        self.handle_list_tools(request, context).await
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        self.handle_call_tool(request, context).await
    }

    async fn enqueue_task(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CreateTaskResult, McpError> {
        self.handle_enqueue_task(request, context).await
    }

    async fn list_resources(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        self.handle_list_resources(request, context).await
    }

    async fn list_resource_templates(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        self.handle_list_resource_templates(request, context).await
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        self.handle_read_resource(request, context).await
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.handle_subscribe(request, context).await
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.handle_unsubscribe(request, context).await
    }

    async fn list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        self.handle_list_prompts(request, context).await
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        self.handle_get_prompt(request, context).await
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        self.handle_complete(request, context).await
    }

    async fn list_tasks(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListTasksResult, McpError> {
        self.handle_list_tasks(request, context).await
    }

    async fn get_task_info(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        self.handle_get_task_info(request, context).await
    }

    async fn get_task_result(
        &self,
        request: GetTaskPayloadParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskPayloadResult, McpError> {
        self.handle_get_task_result(request, context).await
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CancelTaskResult, McpError> {
        self.handle_cancel_task(request, context).await
    }
}
