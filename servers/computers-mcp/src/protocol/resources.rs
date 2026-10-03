use super::{ComputersMcp, auth};
use rmcp::{ErrorData, RoleServer, model::*, service::RequestContext};
use serde::Serialize;
use veoveo_computers_contract::ComputerResource;
use veoveo_mcp_contract::hosting::served_by_host;
use veoveo_types::TaskTypeDefinition;

fn json<T: Serialize>(uri: &str, value: &T) -> Result<ReadResourceResult, ErrorData> {
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(
            serde_json::to_string(value).map_err(|_| auth::unavailable())?,
            uri,
        )
        .with_mime_type("application/json"),
    ]))
}
impl ComputersMcp {
    pub(super) async fn resource(
        &self,
        address: ComputerResource,
        request: &ReadResourceRequestParams,
        context: &RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, ErrorData> {
        let actor = auth::actor(context)?;
        if request.request_state.is_some() || request.input_responses.is_some() {
            return Err(ErrorData::invalid_params(
                "Computer resources have no interactive request state",
                None,
            ));
        }
        let uri = &request.uri;
        let result = match address {
            ComputerResource::Maintenance(id) => json(
                uri,
                &self
                    .app
                    .maintenance_state(&actor, id)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ComputerResource::Collection(after) => json(
                uri,
                &self
                    .app
                    .snapshot(&actor, after)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ComputerResource::Computer(id) => json(
                uri,
                &self
                    .app
                    .computer(&actor, id)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ComputerResource::Access(id) => json(
                uri,
                &self
                    .app
                    .access_grants(&actor, id)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ComputerResource::Execution(id) => {
                let access = self
                    .app
                    .store
                    .authorize_command_task(
                        &actor,
                        id,
                        veoveo_computers::commands::CommandTaskAction::Observe,
                    )
                    .await
                    .map_err(|error| super::read_error(error.into()))?;
                let read = async {
                    let task = veoveo_task_runtime::authorized_snapshot(
                        &self
                            .app
                            .tasks
                            .for_owner(access.owner().map_err(|_| auth::forbidden())?),
                        &id.to_string(),
                    )
                    .await?;
                    if task.status != veoveo_task_runtime::TaskStatus::Succeeded
                        || task.task_type
                            != veoveo_computers::api::ComputerTaskKind::Execution.name()
                    {
                        return Err(ErrorData::invalid_params(
                            "command result is not available",
                            None,
                        ));
                    }
                    let response: CallToolResult =
                        serde_json::from_value(task.result.ok_or_else(auth::unavailable)?)
                            .map_err(|_| auth::unavailable())?;
                    let result: veoveo_computers::api::ExecutionResult = serde_json::from_value(
                        response.structured_content.ok_or_else(auth::unavailable)?,
                    )
                    .map_err(|_| auth::unavailable())?;
                    if result.execution_id() != id
                        || result.computer_id() != access.computer_id()
                        || String::from(result.result_uri()) != *uri
                    {
                        return Err(auth::unavailable());
                    }
                    access.owner().map_err(|_| auth::forbidden())?;
                    json(uri, &result)
                };
                tokio::time::timeout_at(tokio::time::Instant::from_std(access.valid_until()), read)
                    .await
                    .map_err(|_| auth::forbidden())??
            }
            ComputerResource::Transfer(id) => json(
                uri,
                &self
                    .app
                    .file_result(&actor, id)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ComputerResource::Automation(id) => json(
                uri,
                &self
                    .app
                    .automation_grants(&actor, id)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ComputerResource::Grant { computer, grant } => json(
                uri,
                &self
                    .app
                    .automation_grant(&actor, computer, grant)
                    .await
                    .map_err(super::read_error)?,
            )?,
            ComputerResource::Docs | ComputerResource::Contract | ComputerResource::Document(_) => {
                return Err(served_by_host());
            }
        };
        Ok(result)
    }
}
