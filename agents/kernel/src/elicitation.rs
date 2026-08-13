//! Durable operator elicitations.

use std::{str::FromStr, time::Duration};

use rig_core::tool::rmcp::{McpElicitationHandler, related_task_id};
use rig_core::wasm_compat::WasmBoxedFuture;
use rmcp::model::{ElicitRequestParams, ElicitResult, ElicitationAction};
use veoveo_agent_runtime::{AgentRuntime, NewElicitation, json_object};
use veoveo_platform_store::{
    AgentElicitationId, AgentElicitationRecord, AgentElicitationState, TaskId,
};

use crate::wake::WakeBus;

pub struct ParkedElicitationHandler {
    runtime: AgentRuntime,
    bus: WakeBus,
    grace: Duration,
}

impl ParkedElicitationHandler {
    pub fn new(runtime: AgentRuntime, bus: WakeBus, grace: Duration) -> Self {
        Self {
            runtime,
            bus,
            grace,
        }
    }
}

impl McpElicitationHandler for ParkedElicitationHandler {
    fn elicit(
        &self,
        request: ElicitRequestParams,
    ) -> WasmBoxedFuture<'_, Result<ElicitResult, rmcp::ErrorData>> {
        Box::pin(async move {
            let ElicitRequestParams::FormElicitationParams {
                meta,
                message,
                requested_schema,
            } = &request
            else {
                return Ok(ElicitResult::new(ElicitationAction::Decline));
            };

            let elicitation_id = AgentElicitationId::new();
            let related = related_task_id(meta.as_ref())
                .and_then(|value| TaskId::from_str(&value).ok())
                .filter(|task_id| task_id.as_uuid().get_version_num() == 7);
            let requested_schema = serde_json::to_value(requested_schema)
                .ok()
                .and_then(|value| json_object(value, "elicitation schema").ok());
            let wake_id = match self
                .runtime
                .park_elicitation(NewElicitation {
                    elicitation_id,
                    related_task: related,
                    message: message.clone(),
                    requested_schema,
                })
                .await
            {
                Ok(wake_id) => wake_id,
                Err(error) => {
                    tracing::error!(%error, "parking elicitation failed");
                    return Ok(ElicitResult::new(ElicitationAction::Decline));
                }
            };
            self.bus.hint(wake_id);

            tracing::info!(%elicitation_id, related_task = ?related, "elicitation parked");

            match self
                .runtime
                .wait_for_elicitation_terminal(elicitation_id, self.grace)
                .await
            {
                Ok(Some(record)) => Ok(durable_result(record)),
                Ok(None) => Ok(ElicitResult::new(ElicitationAction::Decline)),
                Err(error) => {
                    tracing::error!(%error, %elicitation_id, "waiting for elicitation answer failed");
                    Ok(ElicitResult::new(ElicitationAction::Decline))
                }
            }
        })
    }
}

fn durable_result(record: AgentElicitationRecord) -> ElicitResult {
    match record.state {
        AgentElicitationState::Answered => ElicitResult::new(ElicitationAction::Accept)
            .with_content(record.answer.map_or(serde_json::Value::Null, |answer| {
                serde_json::Value::Object(answer.into_map().into_iter().collect())
            })),
        AgentElicitationState::Cancelled => ElicitResult::new(ElicitationAction::Cancel),
        AgentElicitationState::Declined | AgentElicitationState::Parked => {
            ElicitResult::new(ElicitationAction::Decline)
        }
    }
}
