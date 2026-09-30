use rmcp::{
    model::{
        ErrorData as McpError, GetPromptRequestParams, GetPromptResult, ListPromptsResult,
        PaginatedRequestParams,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::{GatewayAction, PromptName, paginate};

use crate::mcp_support::{ensure_unique_prompts, mcp_internal, mcp_invalid_params};

use super::{GATEWAY_PAGE_SIZE, GatewayMcp};

impl GatewayMcp {
    pub(super) async fn handle_list_prompts(
        &self,
        request: Option<PaginatedRequestParams>,
        context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        let subject = self.authenticated(&context)?;
        let snapshot = self.catalog.snapshot();
        let fingerprint = super::discovery_authorization_fingerprint(&subject)?;
        let mut prompts = Vec::new();
        let mut denied = 0u32;
        for server_slug in self.profile_servers() {
            let key = super::discovery::DiscoveryCacheKey {
                catalog_generation: snapshot.generation(),
                principal: subject.actor.id.clone(),
                authorization_fingerprint: fingerprint,
                server: server_slug.clone(),
            };
            self.ensure_discovery_watch(&key, context.peer.clone(), &subject)
                .await?;
            if let Some(mut cached) = self.discovery.prompts(&key).await {
                denied = denied.saturating_add(cached.denied);
                prompts.append(&mut cached.items);
                continue;
            }
            let fetch = self.discovery.start_prompts(key).await;
            let upstream = self
                .idempotent_upstream_request(
                    &server_slug,
                    context.peer.clone(),
                    &subject,
                    |upstream| async move { upstream.list_all_prompts().await },
                )
                .await?;
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
                .allows_catalog_targets(&context, GatewayAction::PromptsList, targets)
                .await?;
            let rejected: u32 = allowed
                .iter()
                .filter(|allowed| !**allowed)
                .count()
                .try_into()
                .map_err(|_| mcp_internal("catalog exceeds audit count range"))?;
            let mut items = upstream
                .into_iter()
                .zip(allowed)
                .filter_map(|(prompt, allowed)| allowed.then_some(prompt))
                .collect::<Vec<_>>();
            self.discovery
                .store_prompts(fetch, items.clone(), rejected)
                .await;
            denied = denied.saturating_add(rejected);
            prompts.append(&mut items);
        }
        ensure_unique_prompts(&prompts)?;
        prompts.sort_by(|left, right| left.name.cmp(&right.name));
        self.record_discovery(
            &subject,
            veoveo_audit_contract::DiscoveryKind::Prompts,
            prompts.iter().map(|prompt| prompt.name.clone()).collect(),
            denied,
        )
        .await?;
        let page = paginate(prompts, request.as_ref(), GATEWAY_PAGE_SIZE)
            .map_err(|err| mcp_invalid_params(err.to_string()))?;
        Ok(ListPromptsResult {
            prompts: page.items,
            next_cursor: page.next_cursor,
            result_type: Some(rmcp::model::ResultType::COMPLETE),
            ttl_ms: Some(veoveo_mcp_contract::PRIVATE_CATALOG_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            meta: None,
        })
    }

    pub(super) async fn handle_get_prompt(
        &self,
        request: GetPromptRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
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
