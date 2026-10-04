use super::{ComputersMcp, auth, tasks};
use rmcp::{
    ErrorData, RoleServer, handler::server::wrapper::Parameters, model::*, service::RequestContext,
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_computers::api::*;

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
enum GrantOutput {
    Completed(AutomationGrantResult),
    Rejected(ApiError),
}

#[tool_router(router = automation_tool_router, vis = "pub(super)")]
impl ComputersMcp {
    #[tool(
        title = "Grant Computer automation",
        description = "Let a named user or agent, through a specific OAuth client, perform selected actions on your Computer. Granting Execute also requires allowing Stop, so a cancelled command can stop its run. Reuse requestId with the same scope when retrying.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<GrantOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn grant_automation(
        &self,
        Parameters(input): Parameters<IssueAutomationGrantInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let actor = auth::actor(&context)?;
        grant_reply(self.app.grant_automation(&actor, input).await)
    }

    #[tool(
        title = "Revoke Computer automation",
        description = "Revoke an automation grant on your Computer. Commands running under it are stopped as its Stop permission allows; files in the home directory are kept. Revoking twice is safe.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<GrantOutput>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn revoke_automation(
        &self,
        Parameters(input): Parameters<RevokeAutomationGrantInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let actor = auth::actor(&context)?;
        grant_reply(self.app.revoke_automation(&actor, input).await)
    }
}

fn grant_reply(
    result: crate::application::Result<AutomationGrantResult>,
) -> Result<CallToolResponse, ErrorData> {
    match result {
        Ok(result) => {
            let mut reply = CallToolResult::success(vec![
                ContentBlock::text(if result.grant().revoked_at.is_some() {
                    "Automation grant revoked."
                } else {
                    "Automation grant issued."
                }),
                ContentBlock::resource_link(Resource::new(
                    String::from(result.result_uri()),
                    "Automation grant",
                )),
            ]);
            reply.structured_content =
                Some(serde_json::to_value(result).map_err(|_| auth::unavailable())?);
            Ok(reply.into())
        }
        Err(error) => tasks::rejection(error),
    }
}
