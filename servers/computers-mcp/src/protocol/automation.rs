use super::{ComputersMcp, auth, tasks};
use rmcp::{ErrorData, RoleServer, model::*, service::RequestContext};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_computers::api::*;

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
enum GrantOutput {
    Completed(AutomationGrantResult),
    Rejected(ApiError),
}
pub(super) fn tools() -> Vec<Tool> {
    vec![
        Tool::new("grant_automation", "Let a named user or agent, through a specific OAuth client, perform selected actions on your Computer. Granting Execute also requires allowing Stop, so a cancelled command can stop its run. Reuse requestId with the same scope when retrying.", rmcp::handler::server::tool::schema_for_type::<IssueAutomationGrantInput>())
            .with_title("Grant Computer automation")
            .with_output_schema::<GrantOutput>()
            .with_annotations(ToolAnnotations::new().read_only(false).destructive(false).idempotent(true).open_world(false)),
        Tool::new("revoke_automation", "Revoke an automation grant on your Computer. Commands running under it are stopped as its Stop permission allows; files in the home directory are kept. Revoking twice is safe.", rmcp::handler::server::tool::schema_for_type::<RevokeAutomationGrantInput>())
            .with_title("Revoke Computer automation")
            .with_output_schema::<GrantOutput>()
            .with_annotations(ToolAnnotations::new().read_only(false).destructive(true).idempotent(true).open_world(false)),
    ]
}
impl ComputersMcp {
    pub(super) async fn automation(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let actor = auth::actor(&context)?;
        let result = if request.name == "grant_automation" {
            self.app
                .grant_automation(&actor, tasks::input(request.arguments)?)
                .await
        } else {
            self.app
                .revoke_automation(&actor, tasks::input(request.arguments)?)
                .await
        };
        match result {
            Ok(result) => {
                let mut reply = CallToolResult::success(vec![
                    ContentBlock::text(if result.grant.revoked_at.is_some() {
                        "Automation grant revoked."
                    } else {
                        "Automation grant issued."
                    }),
                    ContentBlock::resource_link(Resource::new(
                        result.result_uri.clone(),
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
}
