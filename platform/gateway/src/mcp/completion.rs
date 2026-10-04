use super::http_response::RequestError;
use rmcp::{
    model::{CompleteRequestParams, CompleteResult, Reference},
    service::{RequestContext, RoleServer},
};
use veoveo_gateway_contract::GatewayAction;
use veoveo_mcp_contract::{CompletionExposure, PolicyTarget, PromptName};

use crate::mcp_support::{
    mcp_invalid_params, mcp_invalid_request, resource_template_policy_target,
};

use super::GatewayMcp;

impl GatewayMcp {
    pub(super) async fn handle_complete(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, RequestError> {
        let server = match &request.r#ref {
            Reference::Resource(reference) => self.server_for_resource(&reference.uri)?,
            Reference::Prompt(reference) => self.server_for_prompt(&reference.name)?,
            _ => return Err(mcp_invalid_params("unsupported completion reference kind").into()),
        };
        let catalog = self.catalog.current();
        let (_profile, exposure, manifest) = catalog
            .profile_server(&self.profile_id, &server)
            .ok_or_else(|| mcp_invalid_params(format!("server `{server}` is not exposed")))?;
        if exposure.completions != CompletionExposure::Enabled || !manifest.capabilities.completions
        {
            return Err(mcp_invalid_request("profile does not expose completions").into());
        }
        let target = match &request.r#ref {
            Reference::Resource(reference) => {
                resource_template_policy_target(server.clone(), &reference.uri)?
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
            _ => return Err(mcp_invalid_params("unsupported completion reference kind").into()),
        };
        let subject = self
            .authorize(&context, GatewayAction::CompletionComplete, target)
            .await?;
        self.idempotent_upstream_request(&server, context.peer.clone(), &subject, |upstream| {
            let request = request.clone();
            async move { upstream.complete(request).await }
        })
        .await
    }
}
