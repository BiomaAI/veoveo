use rmcp::{
    model::{CompleteRequestParams, CompleteResult, ErrorData as McpError, Reference},
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::{
    CompletionExposure, GatewayAction, PolicyTarget, PromptName, ResourceUri,
};

use crate::mcp_support::{mcp_invalid_params, mcp_invalid_request, upstream_error};

use super::GatewayMcp;

impl GatewayMcp {
    pub(super) async fn handle_complete(
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
}
