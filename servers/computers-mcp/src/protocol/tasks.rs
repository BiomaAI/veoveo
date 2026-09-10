use super::{ComputersMcp, auth};
use crate::{ApplicationError, application};
use rmcp::{ErrorData, RoleServer, model::*, service::RequestContext};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use veoveo_computers::api::ErrorCode as ApiErrorCode;
use veoveo_computers::{ComputerActor, ComputerError, Operation, api::*};

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
enum LifecycleOutput {
    Completed(LifecycleResult),
    Rejected(ApiError),
}
pub fn tools() -> Vec<Tool> {
    let create = Tool::new("create", "Create a retained Computer using the installation default. Reuse requestId when retrying. Requires the Tasks extension.", rmcp::handler::server::tool::schema_for_type::<CreateInput>())
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
        lifecycle(
            "start",
            "Start a stopped Computer with its retained home. Reuse requestId when retrying. Requires the Tasks extension.",
        ),
        lifecycle(
            "stop",
            "Stop the Computer's processes while keeping its retained home. Reuse requestId when retrying. Requires the Tasks extension.",
        ),
    ]
}
fn input<T: DeserializeOwned>(arguments: Option<JsonObject>) -> Result<T, ErrorData> {
    serde_json::from_value(serde_json::Value::Object(arguments.unwrap_or_default()))
        .map_err(|_| ErrorData::invalid_params("invalid Computer action input", None))
}
fn rejection(error: ApplicationError) -> Result<CallToolResponse, ErrorData> {
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
        let action = match request.name.as_ref() {
            "create" => Action::Create,
            "start" => Action::Start,
            "stop" => Action::Stop,
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
        let actor = auth::actor(&context)?;
        let result: application::Result<Operation> = match action {
            Action::Create => self.app.create(actor, input(request.arguments)?).await,
            _ => {
                self.app
                    .lifecycle(actor, input(request.arguments)?, action)
                    .await
            }
        };
        let operation = match result {
            Ok(o) => o,
            Err(e) => return rejection(e),
        };
        for pin in pins {
            self.app
                .tasks
                .adopt_retention_pin_for_repair(&operation.task_id().to_string(), &pin)
                .await
                .map_err(|_| auth::unavailable())?;
        }
        let task = self
            .app
            .tasks
            .get(&operation.task_id().to_string())
            .await
            .map_err(|_| auth::unavailable())?
            .ok_or_else(auth::unavailable)?;
        Ok(CreateTaskResult::new(veoveo_task_runtime::task_seed(&task)).into())
    }
    pub(super) async fn task_owner(
        &self,
        context: &RequestContext<RoleServer>,
        id: &str,
        cancel: bool,
    ) -> Result<ComputerActor, ErrorData> {
        let actor = auth::actor(context)?;
        let id = super::resources::canonical_uuid(id)
            .ok_or_else(|| ErrorData::invalid_params("unknown task", None))?;
        let operation = self
            .app
            .store
            .operation(actor.owner(), id)
            .await
            .map_err(|_| ErrorData::invalid_params("unknown task", None))?;
        let control = self
            .app
            .store
            .control_authority(&actor)
            .await
            .map_err(|_| auth::forbidden())?;
        control
            .require_read(Some(operation.computer_id))
            .map_err(|_| auth::forbidden())?;
        if cancel {
            control
                .require_action(operation.action)
                .map_err(|_| auth::forbidden())?;
        }
        Ok(actor)
    }
}
