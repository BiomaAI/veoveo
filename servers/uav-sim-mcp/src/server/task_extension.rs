use std::sync::Arc;

use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller};

use crate::contract::{
    CaptureDatasetRequest, DurableOperation, ExecuteVehicleMissionPlanRequest, RunScenarioRequest,
};

use super::ownership::{plane_caller, runtime_owner};
use super::state::AppState;
use super::task_worker::{start_operation, start_vehicle_mission_plan};

#[derive(Clone)]
pub(super) struct UavSimTaskExtension {
    state: Arc<AppState>,
}

impl UavSimTaskExtension {
    pub(super) fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[derive(Clone)]
pub(super) struct AuthenticatedCaller {
    identity: GatewayInternalIdentity,
    plane: PlaneCaller,
}

impl veoveo_task_runtime::DurableTaskService for UavSimTaskExtension {
    type Caller = AuthenticatedCaller;

    fn authenticate(
        &self,
        context: &rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<Self::Caller, rmcp::ErrorData> {
        let parts = context
            .extensions
            .get::<axum::http::request::Parts>()
            .ok_or_else(|| rmcp::ErrorData::invalid_request("gateway identity missing", None))?;
        let identity = parts
            .extensions
            .get::<GatewayInternalIdentity>()
            .cloned()
            .ok_or_else(|| rmcp::ErrorData::invalid_request("gateway identity missing", None))?;
        let bearer = parts
            .extensions
            .get::<super::auth::ForwardedBearer>()
            .map(|bearer| bearer.0.clone())
            .ok_or_else(|| rmcp::ErrorData::invalid_request("forwarded bearer missing", None))?;
        Ok(AuthenticatedCaller {
            plane: plane_caller(identity.clone(), bearer),
            identity,
        })
    }

    async fn start_tool_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::CallToolRequestParams,
    ) -> Result<Option<rmcp::model::CreateTaskResult>, rmcp::ErrorData> {
        let arguments = serde_json::Value::Object(request.arguments.unwrap_or_default());
        if request.name.as_ref() == "execute_vehicle_mission_plan" {
            require_scope(&caller.identity, "uav-sim:control")?;
            let mission_request =
                serde_json::from_value::<ExecuteVehicleMissionPlanRequest>(arguments)
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?;
            let snapshot = start_vehicle_mission_plan(
                self.state.clone(),
                caller.plane.clone(),
                mission_request,
                veoveo_task_runtime::retention_pins(request.meta.as_ref())?,
            )
            .await
            .map_err(|error| rmcp::ErrorData::invalid_request(error, None))?;
            return Ok(Some(rmcp::model::CreateTaskResult::new(
                veoveo_task_runtime::task_seed(&snapshot),
            )));
        }
        let operation = match request.name.as_ref() {
            "run_scenario" => DurableOperation::RunScenario(
                serde_json::from_value::<RunScenarioRequest>(arguments)
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?,
            ),
            "capture_dataset" => DurableOperation::CaptureDataset(
                serde_json::from_value::<CaptureDatasetRequest>(arguments)
                    .map_err(|error| rmcp::ErrorData::invalid_params(error.to_string(), None))?,
            ),
            _ => return Ok(None),
        };
        let snapshot = start_operation(
            self.state.clone(),
            caller.plane.clone(),
            operation,
            veoveo_task_runtime::retention_pins(request.meta.as_ref())?,
        )
        .await
        .map_err(|error| rmcp::ErrorData::internal_error(error, None))?;
        Ok(Some(rmcp::model::CreateTaskResult::new(
            veoveo_task_runtime::task_seed(&snapshot),
        )))
    }

    async fn get_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::GetTaskParams,
    ) -> Result<rmcp::model::GetTaskResult, rmcp::ErrorData> {
        veoveo_task_runtime::get_durable_task(
            &self.state.tasks,
            &runtime_owner(&caller.identity),
            request,
        )
        .await
    }

    async fn update_task(
        &self,
        caller: &Self::Caller,
        request: rmcp::model::UpdateTaskParams,
    ) -> Result<(), rmcp::ErrorData> {
        veoveo_task_runtime::update_durable_task(
            &self.state.tasks,
            &runtime_owner(&caller.identity),
            request,
        )
        .await
    }

    async fn cancel_task(
        &self,
        caller: &Self::Caller,
        task_id: String,
    ) -> Result<(), rmcp::ErrorData> {
        veoveo_task_runtime::cancel_durable_task(
            &self.state.tasks,
            &runtime_owner(&caller.identity),
            task_id,
        )
        .await
    }

    async fn subscribe_tasks(
        &self,
        caller: &Self::Caller,
        task_ids: Vec<String>,
    ) -> Result<veoveo_task_runtime::DurableTaskSubscription, rmcp::ErrorData> {
        veoveo_task_runtime::subscribe_durable_tasks(
            &self.state.tasks,
            runtime_owner(&caller.identity),
            task_ids,
        )
        .await
    }
}

fn require_scope(
    identity: &GatewayInternalIdentity,
    required: &str,
) -> Result<(), rmcp::ErrorData> {
    identity
        .actor
        .scopes
        .iter()
        .any(|scope| scope.as_str() == required)
        .then_some(())
        .ok_or_else(|| {
            rmcp::ErrorData::invalid_request(format!("scope `{required}` is required"), None)
        })
}
