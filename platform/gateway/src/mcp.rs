mod authorization;
mod completion;
mod discovery;
mod health;
mod info;
mod progress;
mod prompts;
mod resources;
mod subscriptions;
mod tasks;
mod tools;
mod upstream;
mod upstream_authorized_http;
mod upstream_http;
pub use upstream_http::GatewayUpstreamHttpClientPool;

use std::{future::Future, sync::Arc};

use rmcp::{
    ClientLifecycleMode, ClientServiceExt,
    handler::server::ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CancelTaskParams, CompleteRequestParams,
        CompleteResult, ErrorData as McpError, GetPromptRequestParams, GetTaskParams,
        GetTaskResult, ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult,
        ListToolsResult, PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResponse,
        ServerInfo, SubscriptionFilter, UpdateTaskParams,
    },
    service::{
        Peer, RequestContext, RoleClient, RoleServer, RunningService, ServiceError,
        SubscriptionContext,
    },
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    GatewayInternalTokenIssuer, GatewayProfileId, InvocationAuthority, Principal, ServerSlug,
    UpstreamTransport,
};

use crate::{
    AuthenticatedSubject, GatewayCatalogHandle, GatewayState,
    mcp_support::{mcp_internal, mcp_invalid_params},
};
use discovery::CatalogDiscoveryCache;
use upstream::{GatewayUpstreamHandler, GatewayUpstreamHandlerConfig};
use upstream_authorized_http::GatewayAuthorizedHttpClient;

pub use health::{GatewayServerHealth, GatewayServerHealthState, probe_gateway_server_health};

pub(super) const GATEWAY_PAGE_SIZE: usize = 100;

struct RequestUpstream {
    peer: Peer<RoleClient>,
    _running: RunningService<RoleClient, GatewayUpstreamHandler>,
}

#[derive(Debug, Clone)]
pub struct GatewayMcp {
    catalog: GatewayCatalogHandle,
    state: GatewayState,
    profile_id: GatewayProfileId,
    internal_token_issuer: GatewayInternalTokenIssuer,
    upstream_http: GatewayUpstreamHttpClientPool,
    discovery: Arc<CatalogDiscoveryCache>,
    progress_tokens: progress::GatewayProgressTokens,
}

impl GatewayMcp {
    pub fn new(
        catalog: GatewayCatalogHandle,
        profile_id: GatewayProfileId,
        state: GatewayState,
        internal_token_issuer: GatewayInternalTokenIssuer,
        upstream_http: GatewayUpstreamHttpClientPool,
    ) -> Self {
        Self {
            catalog,
            state,
            profile_id,
            internal_token_issuer,
            upstream_http,
            discovery: Arc::new(CatalogDiscoveryCache::default()),
            progress_tokens: progress::GatewayProgressTokens::default(),
        }
    }

    async fn upstream(
        &self,
        server_slug: &ServerSlug,
        downstream: Peer<RoleServer>,
        subject: &AuthenticatedSubject,
    ) -> Result<RequestUpstream, McpError> {
        self.upstream_with_tasks(server_slug, downstream, subject, false)
            .await
    }

    async fn upstream_with_tasks(
        &self,
        server_slug: &ServerSlug,
        downstream: Peer<RoleServer>,
        subject: &AuthenticatedSubject,
        tasks: bool,
    ) -> Result<RequestUpstream, McpError> {
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
        let handler = GatewayUpstreamHandler::new(GatewayUpstreamHandlerConfig {
            catalog: self.catalog.clone(),
            profile_id: self.profile_id.clone(),
            principal_id: subject.principal.id.clone(),
            upstream_server: server_slug.clone(),
            downstream,
            progress_tokens: self.progress_tokens.clone(),
            discovery: self.discovery.clone(),
            tasks,
        });
        let running = handler
            .serve_with_lifecycle(
                transport,
                ClientLifecycleMode::Discover {
                    preferred_versions: vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                },
            )
            .await
            .map_err(|err| mcp_internal(format!("failed to discover upstream MCP: {err}")))?;
        let peer = running.peer().clone();
        Ok(RequestUpstream {
            peer,
            _running: running,
        })
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
        match request(upstream.peer.clone()).await {
            Ok(result) => Ok(result),
            Err(error) if recoverable_upstream_connection_error(&error) => {
                tracing::warn!(
                    server = %server_slug,
                    principal = %subject.actor.id,
                    %error,
                    "reconnecting a request-scoped upstream MCP client before one idempotent retry"
                );
                drop(upstream);
                let retry = self.upstream(server_slug, downstream, subject).await?;
                request(retry.peer)
                    .await
                    .map_err(crate::mcp_support::upstream_error)
            }
            Err(error) => Err(crate::mcp_support::upstream_error(error)),
        }
    }
}

fn upstream_transport_config(uri: &str) -> StreamableHttpClientTransportConfig {
    StreamableHttpClientTransportConfig::with_uri(uri.to_owned())
}

fn recoverable_upstream_connection_error(error: &ServiceError) -> bool {
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
    // and its effective authorization are unchanged, so it is excluded from
    // the durable task-route authority binding.
    let mut stable_actor = actor.clone();
    stable_actor.authenticated_at = None;
    Ok(Sha256::digest(
        serde_json::to_vec(&(stable_actor, authority))
            .map_err(|err| mcp_internal(format!("failed to fingerprint invocation: {err}")))?,
    )
    .into())
}

impl ServerHandler for GatewayMcp {
    fn get_info(&self) -> ServerInfo {
        self.handle_get_info()
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
    ) -> Result<CallToolResponse, McpError> {
        self.handle_call_tool(request, context).await
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
    ) -> Result<ReadResourceResponse, McpError> {
        self.handle_read_resource(request, context)
            .await
            .map(Into::into)
    }

    fn accepted_subscription_filter(
        &self,
        requested: &SubscriptionFilter,
    ) -> Option<SubscriptionFilter> {
        Some(self.accepted_subscriptions(requested))
    }

    async fn listen(&self, context: SubscriptionContext) -> Result<(), McpError> {
        self.handle_listen(context).await
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
    ) -> Result<rmcp::model::GetPromptResponse, McpError> {
        async { self.handle_get_prompt(request, context).await }
            .await
            .map(Into::into)
    }

    async fn complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        self.handle_complete(request, context).await
    }

    async fn get_task(
        &self,
        request: GetTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetTaskResult, McpError> {
        self.handle_get_task(request, context).await
    }

    async fn update_task(
        &self,
        request: UpdateTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.handle_update_task(request, context).await
    }

    async fn cancel_task(
        &self,
        request: CancelTaskParams,
        context: RequestContext<RoleServer>,
    ) -> Result<(), McpError> {
        self.handle_cancel_task(request, context).await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::Utc;
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
    fn abrupt_upstream_transport_error_is_recoverable_for_idempotent_requests() {
        let error = ServiceError::McpError(McpError::new(
            rmcp::model::ErrorCode(0),
            "no close frame received or sent",
            None,
        ));
        assert!(recoverable_upstream_connection_error(&error));
    }

    #[test]
    fn application_upstream_error_is_not_retried() {
        let error = ServiceError::McpError(McpError::internal_error("application failure", None));
        assert!(!recoverable_upstream_connection_error(&error));
    }
}
