use rmcp::{
    model::{
        ErrorData as McpError, GetPromptRequestParams, GetPromptResult, ListPromptsResult,
        PaginatedRequestParams,
    },
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::{GatewayAction, PromptName, paginate};

use crate::mcp_support::{ensure_unique_prompts, mcp_internal, mcp_invalid_params, upstream_error};

use super::{GATEWAY_PAGE_SIZE, GatewayMcp};

impl GatewayMcp {
    pub(super) async fn handle_list_prompts(
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

    pub(super) async fn handle_get_prompt(
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
}
