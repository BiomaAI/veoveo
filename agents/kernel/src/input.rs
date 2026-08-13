//! Durable application input for protocol-neutral deferred tools.

use std::time::Duration;

use rig::{
    tool::{
        DeferredInputHandler, DeferredToolDescriptor, InputRequests, InputResponse, InputResponses,
        ToolExecutionError,
    },
    wasm_compat::WasmBoxedFuture,
};
use veoveo_agent_runtime::{AgentRuntime, NewInputRequest, json_object};
use veoveo_mcp_contract::CanonicalTaskId;
use veoveo_platform_store::{AgentInputRequestId, AgentInputRequestState};

use crate::wake::WakeBus;

#[derive(Clone)]
pub struct DurableInputHandler {
    runtime: AgentRuntime,
    bus: WakeBus,
    grace: Duration,
}

impl DurableInputHandler {
    pub fn new(runtime: AgentRuntime, bus: WakeBus, grace: Duration) -> Self {
        Self {
            runtime,
            bus,
            grace,
        }
    }

    async fn respond_one(
        &self,
        descriptor: &DeferredToolDescriptor,
        request: &rig::tool::InputRequest,
    ) -> Result<InputResponse, ToolExecutionError> {
        if request.kind != "elicitation" {
            return Err(ToolExecutionError::invalid_args(format!(
                "unsupported deferred input kind '{}'",
                request.kind
            )));
        }
        let input_request_id = AgentInputRequestId::new();
        let related_task = CanonicalTaskId::new(descriptor.execution_id().to_owned()).ok();
        let requested_schema = request
            .schema
            .clone()
            .map(|value| json_object(value, "input schema"))
            .transpose()
            .map_err(|error| ToolExecutionError::invalid_args(error.to_string()))?;
        let wake_id = self
            .runtime
            .create_input_request(NewInputRequest {
                input_request_id,
                related_task,
                message: request
                    .prompt
                    .clone()
                    .unwrap_or_else(|| "Additional input is required".to_owned()),
                requested_schema,
            })
            .await
            .map_err(|error| ToolExecutionError::other(error.to_string()))?;
        self.bus.hint(wake_id);

        let record = self
            .runtime
            .wait_for_input_request_terminal(input_request_id, self.grace)
            .await
            .map_err(|error| ToolExecutionError::other(error.to_string()))?
            .ok_or_else(|| ToolExecutionError::timeout("operator input wait expired"))?;
        let value = match record.state {
            AgentInputRequestState::Answered => {
                record.answer.map_or(serde_json::Value::Null, |answer| {
                    serde_json::Value::Object(answer.into_map().into_iter().collect())
                })
            }
            AgentInputRequestState::Cancelled => {
                return Err(ToolExecutionError::cancelled("operator cancelled input"));
            }
            AgentInputRequestState::Declined | AgentInputRequestState::Pending => {
                return Err(ToolExecutionError::cancelled("operator declined input"));
            }
        };
        serde_json::from_value(serde_json::json!({
            "requestId": request.id,
            "value": value,
        }))
        .map_err(|error| ToolExecutionError::other(error.to_string()))
    }
}

impl DeferredInputHandler for DurableInputHandler {
    fn respond<'a>(
        &'a self,
        descriptor: &'a DeferredToolDescriptor,
        requests: &'a InputRequests,
    ) -> WasmBoxedFuture<'a, Result<InputResponses, ToolExecutionError>> {
        Box::pin(async move {
            let mut responses = Vec::with_capacity(requests.0.len());
            for request in &requests.0 {
                responses.push(self.respond_one(descriptor, request).await?);
            }
            Ok(InputResponses(responses))
        })
    }
}
