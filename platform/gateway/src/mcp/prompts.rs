use super::http_response::RequestError;
use futures::{StreamExt, stream};
use rmcp::{
    model::{
        ErrorData as McpError, GetPromptRequestParams, GetPromptResult, ListPromptsResult,
        PaginatedRequestParams, Prompt,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_gateway_contract::GatewayAction;
use veoveo_gateway_contract::GatewayDiscoverySurface;
use veoveo_mcp_contract::{DiscoveryFailureMode, Exposure, PromptName, ServerSlug};

use crate::{
    AuthenticatedSubject,
    mcp_support::{ensure_unique_prompts, mcp_internal, mcp_invalid_params},
};

use super::{
    GatewayMcp,
    discovery::{
        AdmittedCatalog, DiscoveryCacheKey, MAX_CONCURRENT_DISCOVERY, enforce_complete_discovery,
        isolate_discovery_failures,
    },
};

impl GatewayMcp {
    /// Lists prompts from every profile server that exposes them. A server that
    /// fails discovery is left out and named in degradation metadata, unless the
    /// profile requires complete discovery.
    pub(super) async fn handle_list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let subject = self.authenticated(&context)?;
        let snapshot = self.catalog.snapshot();
        let catalog_generation = snapshot.generation();
        let discovery_failure_mode = snapshot
            .catalog()
            .profile(&self.profile_id)
            .map(|profile| profile.discovery_failure_mode)
            .unwrap_or(DiscoveryFailureMode::FailClosed);
        let servers = snapshot
            .catalog()
            .profile_servers(&self.profile_id)
            .into_iter()
            .filter(|(exposure, server)| {
                server.capabilities.prompts && !matches!(exposure.prompts, Exposure::None)
            })
            .map(|(_, server)| server.slug.clone())
            .collect::<Vec<_>>();
        let fingerprint = super::discovery_authorization_fingerprint(&subject)?;
        let results = stream::iter(servers.into_iter().map(|server_slug| {
            let context = &context;
            let subject = &subject;
            async move {
                let key = DiscoveryCacheKey {
                    catalog_generation,
                    principal: subject.actor.id.clone(),
                    authorization_fingerprint: fingerprint,
                    server: server_slug.clone(),
                };
                let result = self
                    .server_prompts(key, &server_slug, context, subject)
                    .await;
                (server_slug, result)
            }
        }))
        .buffer_unordered(MAX_CONCURRENT_DISCOVERY)
        .collect::<Vec<_>>()
        .await;
        let denied = results
            .iter()
            .filter_map(|(_, result)| result.as_ref().ok())
            .map(|catalog| catalog.denied)
            .sum();
        let results = results
            .into_iter()
            .map(|(server, result)| (server, result.map(|catalog| catalog.items)))
            .collect();
        let (prompts, degradation, errors) =
            isolate_discovery_failures(GatewayDiscoverySurface::Prompts, results);
        for (server, error) in &errors {
            tracing::warn!(%server, %error, "isolated upstream prompt discovery failure");
        }
        enforce_complete_discovery(
            GatewayDiscoverySurface::Prompts,
            discovery_failure_mode,
            &errors,
        )?;
        ensure_unique_prompts(&prompts)?;
        self.record_discovery(
            &subject,
            veoveo_audit_contract::DiscoveryKind::Prompts,
            prompts.iter().map(|prompt| prompt.name.clone()).collect(),
            denied,
        )
        .await?;
        let page = super::catalog_pages::page(prompts, request.as_ref())?;
        Ok(ListPromptsResult {
            prompts: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: degradation.into_meta(),
        })
    }

    /// One server's admitted prompts, from the discovery cache when present.
    async fn server_prompts(
        &self,
        key: DiscoveryCacheKey,
        server_slug: &ServerSlug,
        context: &RequestContext<RoleServer>,
        subject: &AuthenticatedSubject,
    ) -> Result<AdmittedCatalog<Prompt>, McpError> {
        self.ensure_discovery_watch(&key, context.peer.clone(), subject)
            .await?;
        if let Some(cached) = self.discovery.prompts(&key).await {
            return Ok(cached);
        }
        let fetch = self.discovery.start_prompts(key).await;
        let discovered = self
            .discover_prompts_for_server(server_slug, context, subject)
            .await;
        match discovered {
            Ok(catalog) => {
                self.discovery
                    .store_prompts(fetch, catalog.items.clone(), catalog.denied)
                    .await;
                Ok(catalog)
            }
            Err(error) => {
                if let Some(fetch) = fetch {
                    self.discovery
                        .finish_failure(GatewayDiscoverySurface::Prompts, fetch)
                        .await;
                }
                Err(error)
            }
        }
    }

    async fn discover_prompts_for_server(
        &self,
        server_slug: &ServerSlug,
        context: &RequestContext<RoleServer>,
        subject: &AuthenticatedSubject,
    ) -> Result<AdmittedCatalog<Prompt>, McpError> {
        let upstream = self
            .idempotent_upstream_request(
                server_slug,
                context.peer.clone(),
                subject,
                |upstream| async move { upstream.list_all_prompts().await },
            )
            .await
            .map_err(RequestError::into_protocol)?;
        let targets = upstream
            .iter()
            .map(|prompt| {
                Ok(veoveo_mcp_contract::PolicyTarget::Prompt {
                    server: server_slug.clone(),
                    prompt: PromptName::new(prompt.name.clone())
                        .map_err(|_| mcp_internal("upstream exposed invalid prompt name"))?,
                })
            })
            .collect::<Result<Vec<_>, McpError>>()?;
        let allowed = self
            .allows_catalog_targets(context, GatewayAction::PromptsList, targets)
            .await?;
        let denied: u32 = allowed
            .iter()
            .filter(|allowed| !**allowed)
            .count()
            .try_into()
            .map_err(|_| mcp_internal("catalog exceeds audit count range"))?;
        let items = upstream
            .into_iter()
            .zip(allowed)
            .filter_map(|(prompt, allowed)| allowed.then_some(prompt))
            .collect();
        Ok(AdmittedCatalog { items, denied })
    }

    pub(super) async fn handle_get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, RequestError> {
        let server = self.server_for_prompt(&request.name)?;
        let prompt = PromptName::new(request.name.clone())
            .map_err(|err| mcp_invalid_params(format!("invalid prompt name: {err}")))?;
        let subject = self
            .authorize_prompt(&context, GatewayAction::PromptsGet, server.clone(), prompt)
            .await?;
        self.idempotent_upstream_request(&server, context.peer.clone(), &subject, |upstream| {
            let request = request.clone();
            async move { upstream.get_prompt(request).await }
        })
        .await
    }
}
