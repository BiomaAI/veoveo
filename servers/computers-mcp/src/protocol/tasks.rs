use super::{ComputersMcp, auth};
use crate::{ApplicationError, application};
use rmcp::{
    ErrorData, RoleServer, handler::server::wrapper::Parameters, model::*, service::RequestContext,
    tool, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use veoveo_computers::api::ErrorCode as ApiErrorCode;
use veoveo_computers::{ComputerError, api::*};
use veoveo_mcp_contract::hosting::plane_caller;
use veoveo_task_runtime::TaskRetentionPin;
use veoveo_types::TaskId;

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
/// Validate transport requirements before any domain reservation. Typed handlers
/// receive metadata from RMCP's request context, without rebuilding a wire request.
struct TaskAdmission {
    pins: BTreeSet<TaskRetentionPin>,
}
impl TaskAdmission {
    fn new(context: &RequestContext<RoleServer>) -> Result<Self, ErrorData> {
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
        Ok(Self {
            pins: veoveo_task_runtime::retention_pins(Some(&context.meta))?,
        })
    }
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
#[tool_router(router = task_tool_router, vis = "pub(super)")]
impl ComputersMcp {
    #[tool(
        title = "Create Computer",
        description = "Create a Computer from the installation's default environment. Its home directory is kept across Stop and Start. Reuse requestId when retrying. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<LifecycleOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn create(
        &self,
        Parameters(input): Parameters<CreateInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let admission = TaskAdmission::new(&context)?;
        let actor = auth::actor(&context)?;
        let result = self
            .app
            .create(actor, input)
            .await
            .map(|operation| operation.task_id());
        self.task_reply(result, admission, &context).await
    }

    #[tool(
        description = "Start a stopped Computer. Its home directory is unchanged. Agents pass a grantId with Start permission. Reuse requestId and grantId when retrying. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<LifecycleOutput>(),
        annotations(read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false)
    )]
    async fn start(
        &self,
        Parameters(input): Parameters<LifecycleInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let admission = TaskAdmission::new(&context)?;
        let actor = auth::actor(&context)?;
        let result = self
            .app
            .lifecycle(actor, input, Action::Start)
            .await
            .map(|operation| operation.task_id());
        self.task_reply(result, admission, &context).await
    }

    #[tool(
        description = "Stop a Computer's processes. Its home directory is kept. Agents pass a grantId with Stop permission. Reuse requestId and grantId when retrying. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<LifecycleOutput>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn stop(
        &self,
        Parameters(input): Parameters<LifecycleInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let admission = TaskAdmission::new(&context)?;
        let actor = auth::actor(&context)?;
        let result = self
            .app
            .lifecycle(actor, input, Action::Stop)
            .await
            .map(|operation| operation.task_id());
        self.task_reply(result, admission, &context).await
    }

    #[tool(
        title = "Execute Computer command",
        description = "Run a command (argv list) on a Computer using your automation grant. The working directory is relative to the home directory, and stdin is standard padded base64. After a lost reply, retry with the same requestId and input. Cancelling may stop the command's whole run, as the grant's Stop permission allows. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<ExecutionOutputSchema>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = true)
    )]
    async fn execute(
        &self,
        Parameters(input): Parameters<ExecuteInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let admission = TaskAdmission::new(&context)?;
        let caller = plane_caller(&context)?;
        let result = self
            .app
            .execute(&caller, input)
            .await
            .map(|operation| operation.task_id());
        self.task_reply(result, admission, &context).await
    }

    #[tool(
        title = "Transfer Computer file",
        description = "Copy an Artifact into a new file in the Computer's home directory, or save a regular file from the home directory as an Artifact. Paths are relative to the home directory. Imports never overwrite existing files or extract archives. After a lost reply, retry with the same requestId and input. Owners omit grantId; agents pass a grant with Execute permission. Cancelling an active transfer may stop the Computer. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<FileOutputSchema>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn transfer_file(
        &self,
        Parameters(input): Parameters<TransferFileInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let admission = TaskAdmission::new(&context)?;
        let caller = plane_caller(&context)?;
        let result = self
            .app
            .transfer_file(&caller, input)
            .await
            .map(|operation| operation.task_id());
        self.task_reply(result, admission, &context).await
    }

    #[tool(
        title = "Update environment",
        description = "Move a Computer to another environment the installation allows, keeping its home directory. This stops its processes, so finish active commands first. Omit templateId to use the current default. After a lost reply, retry with the same requestId to keep the original choice. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<MaintenanceOutputSchema>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn update_template(
        &self,
        Parameters(input): Parameters<UpdateTemplateInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let admission = TaskAdmission::new(&context)?;
        let actor = auth::actor(&context)?;
        let result = self
            .app
            .update_template(&actor, input)
            .await
            .map(|operation| operation.task_id());
        self.task_reply(result, admission, &context).await
    }

    #[tool(
        title = "Resume environment update",
        description = "Resume a paused environment update. Pass the update's current updatedAt, and acknowledge pendingCancellationAt if the update shows one. Keep requestId and all inputs the same on retries. The home directory is kept, and steps whose outcome is uncertain are not repeated. Run as an MCP Task.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<MaintenanceOutputSchema>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn resume_update(
        &self,
        Parameters(input): Parameters<ResumeUpdateInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let admission = TaskAdmission::new(&context)?;
        let actor = auth::actor(&context)?;
        let result = self
            .app
            .resume_update(&actor, input)
            .await
            .map(|operation| operation.task_id());
        self.task_reply(result, admission, &context).await
    }

    #[tool(
        title = "Revoke Computer access",
        description = "Revoke your Computer access grant. The attachment closes within its access deadline; the Computer keeps running. Repeating revocation is safe.",
        output_schema = rmcp::handler::server::tool::schema_for_type::<AccessOutput>(),
        annotations(read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = false)
    )]
    async fn revoke_access(
        &self,
        Parameters(input): Parameters<RevokeAccessInput>,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let actor = auth::actor(&context)?;
        match self.app.revoke_access(&actor, input).await {
            Ok(result) => {
                let mut response = CallToolResult::success(vec![ContentBlock::text(
                    "Access revoked. The Computer keeps running.",
                )]);
                response.structured_content =
                    Some(serde_json::to_value(result).map_err(|_| auth::unavailable())?);
                Ok(response.into())
            }
            Err(error) => rejection(error),
        }
    }

    async fn task_reply(
        &self,
        result: application::Result<TaskId>,
        admission: TaskAdmission,
        context: &RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let task_id = match result {
            Ok(task_id) => task_id,
            Err(error) => return rejection(error),
        };
        let access = self
            .task_access(context, &task_id.to_string(), false)
            .await?;
        for pin in admission.pins {
            access
                .run(async {
                    self.app
                        .tasks
                        .adopt_retention_pin_for_repair(task_id, &pin)
                        .await
                        .map_err(|_| auth::unavailable())
                })
                .await?;
        }
        let task = access
            .run(veoveo_task_runtime::authorized_snapshot(
                &self.app.tasks.for_owner(&access.owner),
                &task_id.to_string(),
            ))
            .await?;
        Ok(CreateTaskResult::new(veoveo_task_runtime::task_seed(&task)).into())
    }
}
