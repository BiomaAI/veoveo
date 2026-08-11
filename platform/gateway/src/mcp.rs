mod authorization;
mod completion;
mod discovery;
mod final_tasks;
mod health;
mod info;
mod progress;
mod prompts;
mod resources;
mod task_extension;
mod tasks;
mod tools;
mod upstream;
mod upstream_authorized_http;
mod upstream_cache;
mod upstream_http;
pub use upstream_http::GatewayUpstreamHttpClientPool;

use std::{future::Future, sync::Arc};

use chrono::{DateTime, TimeDelta, Utc};
use rmcp::{
    ServiceExt,
    handler::server::ServerHandler,
    model::{
        CallToolRequestParams, CallToolResult, CancelTaskParams, CancelTaskResult,
        CompleteRequestParams, CompleteResult, CreateTaskResult, ErrorData as McpError,
        GetPromptRequestParams, GetPromptResult, GetTaskParams, GetTaskPayloadParams,
        GetTaskPayloadResult, GetTaskResult, InitializeRequestParams, InitializeResult,
        ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult, ListToolsResult,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult, ServerInfo,
        SubscribeRequestParams, UnsubscribeRequestParams,
    },
    service::{Peer, RequestContext, RoleClient, RoleServer, ServiceError},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    GatewayInternalTokenIssuer, GatewayProfileId, InvocationAuthority, Principal, ServerSlug,
    UpstreamTransport,
};
use veoveo_platform_store::PlatformStore;

use crate::{
    AuthenticatedSubject, GatewayCatalogHandle, GatewayState,
    mcp_support::{mcp_internal, mcp_invalid_params, mcp_invalid_request},
};
use discovery::CatalogDiscoveryCache;
use upstream::GatewayUpstreamHandler;
use upstream_authorized_http::GatewayAuthorizedHttpClient;
use upstream_cache::{UpstreamCacheKey, UpstreamConnection, UpstreamConnectionCache};

pub use final_tasks::FinalTaskClient;
pub use health::{GatewayServerHealth, GatewayServerHealthState, probe_gateway_server_health};
pub use task_extension::GatewayTaskExtension;

pub(super) const GATEWAY_PAGE_SIZE: usize = 100;
const INTERNAL_TOKEN_TTL_SECONDS: i64 = 15 * 60;

struct CachedUpstream {
    key: UpstreamCacheKey,
    peer: Peer<RoleClient>,
}

#[derive(Debug)]
pub struct GatewayMcp {
    catalog: GatewayCatalogHandle,
    state: GatewayState,
    platform_store: PlatformStore,
    profile_id: GatewayProfileId,
    internal_token_issuer: GatewayInternalTokenIssuer,
    upstream_http: GatewayUpstreamHttpClientPool,
    upstreams: UpstreamConnectionCache,
    discovery: Arc<CatalogDiscoveryCache>,
    progress_tokens: progress::GatewayProgressTokens,
}

impl GatewayMcp {
    pub fn new(
        catalog: GatewayCatalogHandle,
        profile_id: GatewayProfileId,
        state: GatewayState,
        platform_store: PlatformStore,
        internal_token_issuer: GatewayInternalTokenIssuer,
        upstream_http: GatewayUpstreamHttpClientPool,
    ) -> Self {
        Self {
            catalog,
            state,
            platform_store,
            profile_id,
            internal_token_issuer,
            upstream_http,
            upstreams: UpstreamConnectionCache::new(),
            discovery: Arc::new(CatalogDiscoveryCache::default()),
            progress_tokens: progress::GatewayProgressTokens::default(),
        }
    }

    async fn upstream(
        &self,
        server_slug: &ServerSlug,
        downstream: Peer<RoleServer>,
        subject: &AuthenticatedSubject,
    ) -> Result<CachedUpstream, McpError> {
        let snapshot = self.catalog.snapshot();
        let catalog_generation = snapshot.generation();
        let authorization_fingerprint =
            invocation_authorization_fingerprint(&subject.actor, &subject.authority)?;
        let key = UpstreamCacheKey {
            server: server_slug.clone(),
            principal: subject.actor.id.clone(),
            authorization_fingerprint,
            catalog_generation,
        };
        self.upstreams.close_stale(catalog_generation).await;
        if let Some(peer) = self.upstreams.reusable_peer(&key).await {
            return Ok(CachedUpstream { key, peer });
        }

        let initialization_lock = self.upstreams.initialization_lock(&key).await;
        let _initialization_guard = initialization_lock.lock().await;
        if let Some(peer) = self.upstreams.reusable_peer(&key).await {
            return Ok(CachedUpstream { key, peer });
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

        let http_client = self
            .upstream_http
            .client(snapshot.catalog(), &server)
            .await?;
        let authorized_http_client = GatewayAuthorizedHttpClient::new(
            http_client,
            self.internal_token_issuer.clone(),
            self.profile_id.clone(),
            server_slug.clone(),
            subject.actor.clone(),
            subject.authority.clone(),
        );
        let transport = StreamableHttpClientTransport::<GatewayAuthorizedHttpClient>::with_client(
            authorized_http_client,
            upstream_transport_config(server.upstream.url.as_str()),
        );
        let handler = GatewayUpstreamHandler::new(
            self.catalog.clone(),
            self.profile_id.clone(),
            subject.principal.id.clone(),
            server_slug.clone(),
            downstream,
            self.progress_tokens.clone(),
            self.discovery.clone(),
        );
        let running = handler
            .serve(transport)
            .await
            .map_err(|err| mcp_internal(format!("failed to initialize upstream MCP: {err}")))?;
        let peer = self
            .upstreams
            .insert_or_reuse(key.clone(), UpstreamConnection { running })
            .await;
        Ok(CachedUpstream { key, peer })
    }

    async fn idempotent_upstream_request<T, F, Fut>(
        &self,
        server_slug: &ServerSlug,
        downstream: Peer<RoleServer>,
        subject: &AuthenticatedSubject,
        request: F,
    ) -> Result<T, McpError>
    where
        F: Fn(Peer<RoleClient>) -> Fut,
        Fut: Future<Output = Result<T, ServiceError>>,
    {
        let upstream = self
            .upstream(server_slug, downstream.clone(), subject)
            .await?;
        match request(upstream.peer).await {
            Ok(result) => Ok(result),
            Err(error) if recoverable_upstream_session_error(&error) => {
                tracing::warn!(
                    server = %server_slug,
                    principal = %subject.actor.id,
                    %error,
                    "evicting broken upstream MCP session before one idempotent retry"
                );
                self.upstreams
                    .invalidate(
                        &upstream.key,
                        "recoverable idempotent upstream request failure",
                    )
                    .await;
                let retry = self.upstream(server_slug, downstream, subject).await?;
                request(retry.peer)
                    .await
                    .map_err(crate::mcp_support::upstream_error)
            }
            Err(error) => Err(crate::mcp_support::upstream_error(error)),
        }
    }

    async fn final_task_client(
        &self,
        server_slug: &ServerSlug,
        subject: &AuthenticatedSubject,
    ) -> Result<final_tasks::FinalTaskClient, McpError> {
        let snapshot = self.catalog.snapshot();
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
        let token_expires_at = internal_token_expires_at(subject)?;
        let internal_token = self
            .internal_token_issuer
            .issue(
                self.profile_id.clone(),
                server_slug.clone(),
                subject.actor.clone(),
                subject.authority.clone(),
                token_expires_at,
            )
            .map_err(|err| mcp_internal(format!("failed to issue internal token: {err}")))?;
        final_tasks::FinalTaskClient::for_server(
            &self.upstream_http,
            snapshot.catalog(),
            &server,
            internal_token.bearer_token,
        )
        .await
    }
}

fn upstream_transport_config(uri: &str) -> StreamableHttpClientTransportConfig {
    // An upstream server may discard its in-memory MCP session after a pod restart
    // while this gateway-owned connection remains cached. RMCP recovers only from
    // the authoritative HTTP 404 and retries the request once after a new
    // initialize handshake. The request was not dispatched without a live
    // session, so this does not create a polling or general retry path.
    StreamableHttpClientTransportConfig::with_uri(uri.to_owned()).reinit_on_expired_session(true)
}

fn recoverable_upstream_session_error(error: &ServiceError) -> bool {
    match error {
        ServiceError::TransportSend(_) | ServiceError::TransportClosed => true,
        // RMCP currently maps an abruptly terminated Streamable HTTP response
        // (including "no close frame received or sent") to its transport-level
        // pseudo JSON-RPC code 0. No conforming MCP application error uses 0.
        ServiceError::McpError(error) => error.code.0 == 0,
        _ => false,
    }
}

fn invocation_authorization_fingerprint(
    actor: &Principal,
    authority: &InvocationAuthority,
) -> Result<[u8; 32], McpError> {
    // `authenticated_at` records when this HTTP request re-verified the bearer
    // token. It changes on every Streamable HTTP request even though the token
    // and its effective authorization are unchanged, so it must not split the
    // session-owned upstream MCP connection cache.
    let mut stable_actor = actor.clone();
    stable_actor.authenticated_at = None;
    Ok(Sha256::digest(
        serde_json::to_vec(&(stable_actor, authority))
            .map_err(|err| mcp_internal(format!("failed to fingerprint invocation: {err}")))?,
    )
    .into())
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use veoveo_mcp_contract::{
        AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, PrincipalKind, RoleId,
        ScopeName, TenantId, TokenIssuer, TokenSubject, WorkContextId, WorkContextMembershipLevel,
        WorkContextOutputPolicy,
    };

    use super::*;

    fn principal() -> Principal {
        Principal {
            id: PrincipalId::new("issuer#subject").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://identity.example").unwrap(),
            subject: TokenSubject::new("subject").unwrap(),
            tenant: None,
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::from([RoleId::new("operator").unwrap()]),
            scopes: BTreeSet::from([ScopeName::new("tools:call").unwrap()]),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        }
    }

    fn authority() -> InvocationAuthority {
        InvocationAuthority {
            work_context: WorkContextId::new("mission").unwrap(),
            tenant: TenantId::new("tenant").unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::new("r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(PrincipalId::new("issuer#subject").unwrap()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: PrincipalId::new("issuer#subject").unwrap(),
            },
        }
    }

    #[test]
    fn upstream_fingerprint_covers_actor_and_authority() {
        let baseline = principal();
        let mut changed = baseline.clone();
        changed.roles.insert(RoleId::new("administrator").unwrap());
        let mut reverified = baseline.clone();
        reverified.authenticated_at = Some(Utc::now());

        assert_eq!(
            invocation_authorization_fingerprint(&baseline, &authority()).unwrap(),
            invocation_authorization_fingerprint(&baseline, &authority()).unwrap()
        );
        assert_eq!(
            invocation_authorization_fingerprint(&baseline, &authority()).unwrap(),
            invocation_authorization_fingerprint(&reverified, &authority()).unwrap(),
            "HTTP bearer re-verification time is audit metadata, not authorization identity"
        );
        assert_ne!(
            invocation_authorization_fingerprint(&baseline, &authority()).unwrap(),
            invocation_authorization_fingerprint(&changed, &authority()).unwrap()
        );
    }

    #[test]
    fn upstream_transport_recovers_one_expired_mcp_session() {
        let config = upstream_transport_config("http://view-mcp:8788/view/mcp");
        assert!(config.reinit_on_expired_session);
    }

    #[test]
    fn abrupt_upstream_transport_error_is_recoverable_for_idempotent_requests() {
        let error = ServiceError::McpError(McpError::new(
            rmcp::model::ErrorCode(0),
            "no close frame received or sent",
            None,
        ));
        assert!(recoverable_upstream_session_error(&error));
    }

    #[test]
    fn application_upstream_error_is_not_retried() {
        let error = ServiceError::McpError(McpError::internal_error("application failure", None));
        assert!(!recoverable_upstream_session_error(&error));
    }
}
