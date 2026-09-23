use super::{ComputersMcp, auth};
use crate::{ApplicationError, application};
use rmcp::{ErrorData, RoleServer, model::*, service::RequestContext};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use veoveo_computers::api::ErrorCode as ApiErrorCode;
use veoveo_computers::{ComputerError, Operation, api::*};

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
enum LifecycleOutput {
    Completed(LifecycleResult),
    Rejected(ApiError),
}
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
enum AccessOutput {
    Completed(AccessRevocation),
    Rejected(ApiError),
}
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
enum ExecutionOutputSchema {
    Completed(ExecutionResult),
    Rejected(ApiError),
}
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
enum FileOutputSchema {
    Completed(FileTransferResult),
    Rejected(ApiError),
}
#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
enum MaintenanceOutputSchema {
    Completed(MaintenanceResult),
    Rejected(ApiError),
}
pub fn tools() -> Vec<Tool> {
    let create = Tool::new("create", "Create a Computer from the installation's default environment. Its home directory is kept across Stop and Start. Reuse requestId when retrying. Run as an MCP Task.", rmcp::handler::server::tool::schema_for_type::<CreateInput>())
        .with_title("Create Computer").with_output_schema::<LifecycleOutput>()
        .with_annotations(ToolAnnotations::new().read_only(false).destructive(false).idempotent(true).open_world(false));
    let lifecycle = |name: &'static str, description: &'static str| {
        Tool::new(
            name,
            description,
            rmcp::handler::server::tool::schema_for_type::<LifecycleInput>(),
        )
        .with_output_schema::<LifecycleOutput>()
        .with_annotations(
            ToolAnnotations::new()
                .read_only(false)
                .destructive(name == "stop")
                .idempotent(true)
                .open_world(false),
        )
    };
    vec![
        create,
        Tool::new("transfer_file", "Copy an Artifact into a new file in the Computer's home directory, or save a regular file from the home directory as an Artifact. Paths are relative to the home directory. Imports never overwrite existing files or extract archives. After a lost reply, retry with the same requestId and input. Owners omit grantId; agents pass a grant with Execute permission. Cancelling an active transfer may stop the Computer. Run as an MCP Task.", rmcp::handler::server::tool::schema_for_type::<TransferFileInput>())
            .with_title("Transfer Computer file")
            .with_output_schema::<FileOutputSchema>()
            .with_annotations(ToolAnnotations::new().read_only(false).destructive(true).idempotent(true).open_world(false)),
        Tool::new("resume_update", "Resume a paused environment update. Pass the update's current updatedAt, and acknowledge pendingCancellationAt if the update shows one. Keep requestId and all inputs the same on retries. The home directory is kept, and steps whose outcome is uncertain are not repeated. Run as an MCP Task.", rmcp::handler::server::tool::schema_for_type::<ResumeUpdateInput>())
            .with_title("Resume environment update")
            .with_output_schema::<MaintenanceOutputSchema>()
            .with_annotations(ToolAnnotations::new().read_only(false).destructive(true).idempotent(true).open_world(false)),
        Tool::new("update_template", "Move a Computer to another environment the installation allows, keeping its home directory. This stops its processes, so finish active commands first. Omit templateId to use the current default. After a lost reply, retry with the same requestId to keep the original choice. Run as an MCP Task.", rmcp::handler::server::tool::schema_for_type::<UpdateTemplateInput>())
            .with_title("Update environment")
            .with_output_schema::<MaintenanceOutputSchema>()
            .with_annotations(ToolAnnotations::new().read_only(false).destructive(true).idempotent(true).open_world(false)),
        Tool::new("execute", "Run a command (argv list) on a Computer using your automation grant. The working directory is relative to the home directory, and stdin is standard padded base64. After a lost reply, retry with the same requestId and input. Cancelling may stop the command's whole run, as the grant's Stop permission allows. Run as an MCP Task.", rmcp::handler::server::tool::schema_for_type::<ExecuteInput>())
            .with_title("Execute Computer command")
            .with_output_schema::<ExecutionOutputSchema>()
            .with_annotations(ToolAnnotations::new().read_only(false).destructive(true).idempotent(true).open_world(true)),
        Tool::new("revoke_access", "Revoke your Computer access grant. The attachment closes within its access deadline; the Computer keeps running. Repeating revocation is safe.", rmcp::handler::server::tool::schema_for_type::<RevokeAccessInput>())
            .with_title("Revoke Computer access")
            .with_output_schema::<AccessOutput>()
            .with_annotations(ToolAnnotations::new().read_only(false).destructive(true).idempotent(true).open_world(false)),
        lifecycle(
            "start",
            "Start a stopped Computer. Its home directory is unchanged. Agents pass a grantId with Start permission. Reuse requestId and grantId when retrying. Run as an MCP Task.",
        ),
        lifecycle(
            "stop",
            "Stop a Computer's processes. Its home directory is kept. Agents pass a grantId with Stop permission. Reuse requestId and grantId when retrying. Run as an MCP Task.",
        ),
    ]
}
pub(super) fn input<T: DeserializeOwned>(arguments: Option<JsonObject>) -> Result<T, ErrorData> {
    serde_json::from_value(serde_json::Value::Object(arguments.unwrap_or_default())).map_err(
        |error| {
            // serde names the offending field; drop its JSON line/column suffix.
            let detail = error.to_string();
            let detail = detail.split(" at line ").next().unwrap_or_default();
            ErrorData::invalid_params(format!("Invalid Computer action input: {detail}."), None)
        },
    )
}
pub(super) fn rejection(error: ApplicationError) -> Result<CallToolResponse, ErrorData> {
    if matches!(error, ApplicationError::Domain(ComputerError::Forbidden)) {
        return Err(auth::forbidden());
    }
    let code = match &error {
        ApplicationError::Domain(ComputerError::NotFound) => ApiErrorCode::NotFound,
        ApplicationError::Domain(ComputerError::CapacityFull) => ApiErrorCode::CapacityFull,
        ApplicationError::Domain(ComputerError::OperationBusy) => ApiErrorCode::Busy,
        ApplicationError::Domain(ComputerError::InvalidState | ComputerError::StateConflict) => {
            ApiErrorCode::InvalidState
        }
        ApplicationError::Domain(ComputerError::InvalidInput | ComputerError::RequestConflict) => {
            ApiErrorCode::InvalidInput
        }
        _ => ApiErrorCode::Unavailable,
    };
    let result = ApiError {
        code,
        message: error.to_string(),
    };
    let mut reply = CallToolResult::error(vec![ContentBlock::text(result.message.clone())]);
    reply.structured_content = Some(serde_json::to_value(result).map_err(|_| auth::unavailable())?);
    Ok(reply.into())
}
impl ComputersMcp {
    pub(super) async fn call(
        &self,
        mut request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        if matches!(
            request.name.as_ref(),
            "grant_automation" | "revoke_automation"
        ) {
            return self.automation(request, context).await;
        }
        if request.name == "revoke_access" {
            let actor = auth::actor(&context)?;
            return match self
                .app
                .revoke_access(&actor, input(request.arguments)?)
                .await
            {
                Ok(result) => {
                    let mut response = CallToolResult::success(vec![ContentBlock::text(
                        "Access revoked. The Computer keeps running.",
                    )]);
                    response.structured_content =
                        Some(serde_json::to_value(result).map_err(|_| auth::unavailable())?);
                    Ok(response.into())
                }
                Err(error) => rejection(error),
            };
        }
        let action = match request.name.as_ref() {
            "create" => Some(Action::Create),
            "start" => Some(Action::Start),
            "stop" => Some(Action::Stop),
            "execute" | "transfer_file" | "update_template" | "resume_update" => None,
            _ => return Err(ErrorData::invalid_params("unknown Computer tool", None)),
        };
        // Capability admission precedes domain reservation and Task creation.
        if !context
            .meta
            .client_capabilities()
            .is_some_and(|c| c.supports_tasks())
        {
            let mut required = ClientCapabilities::default();
            required.extensions = Some(std::collections::BTreeMap::from([(
                TASKS_EXTENSION_ID.into(),
                JsonObject::new(),
            )]));
            return Err(ErrorData::missing_required_client_capability(required));
        }
        veoveo_task_runtime::restore_task_retention_meta(&mut request, &context.meta)?;
        let pins = veoveo_task_runtime::retention_pins(request.meta.as_ref())?;
        let task_id = if let Some(action) = action {
            let actor = auth::actor(&context)?;
            let result: application::Result<Operation> = match action {
                Action::Create => self.app.create(actor, input(request.arguments)?).await,
                _ => {
                    self.app
                        .lifecycle(actor, input(request.arguments)?, action)
                        .await
                }
            };
            match result {
                Ok(operation) => operation.task_id().to_string(),
                Err(error) => return rejection(error),
            }
        } else if matches!(request.name.as_ref(), "update_template" | "resume_update") {
            let actor = auth::actor(&context)?;
            let result = if request.name == "resume_update" {
                self.app
                    .resume_update(&actor, input(request.arguments)?)
                    .await
            } else {
                self.app
                    .update_template(&actor, input(request.arguments)?)
                    .await
            };
            match result {
                Ok(operation) => operation.task_id().to_string(),
                Err(error) => return rejection(error),
            }
        } else if request.name == "transfer_file" {
            match self
                .app
                .transfer_file(&auth::caller(&context)?, input(request.arguments)?)
                .await
            {
                Ok(operation) => operation.task_id().to_string(),
                Err(error) => return rejection(error),
            }
        } else {
            match self
                .app
                .execute(&auth::caller(&context)?, input(request.arguments)?)
                .await
            {
                Ok(command) => command.task_id().to_string(),
                Err(error) => return rejection(error),
            }
        };
        let access = self.task_access(&context, &task_id, false).await?;
        for pin in pins {
            access
                .run(async {
                    self.app
                        .tasks
                        .adopt_retention_pin_for_repair(&task_id, &pin)
                        .await
                        .map_err(|_| auth::unavailable())
                })
                .await?;
        }
        let task = access
            .run(veoveo_task_runtime::authorized_snapshot(
                &self.app.tasks,
                &access.owner,
                &task_id,
            ))
            .await?;
        Ok(CreateTaskResult::new(veoveo_task_runtime::task_seed(&task)).into())
    }
}
