//! Durable operator elicitations.

use std::{
    collections::HashMap,
    str::FromStr,
    sync::{Arc, Mutex},
    time::Duration,
};

use rig_core::tool::rmcp::{McpElicitationHandler, related_task_id};
use rig_core::wasm_compat::WasmBoxedFuture;
use rmcp::model::{ElicitRequestParams, ElicitResult, ElicitationAction};
use tokio::sync::oneshot;
use veoveo_agent_runtime::{
    AgentRuntime, ElicitationAnswer, NewElicitation, json_object, wrapped_json,
};
use veoveo_platform_store::{AgentElicitationId, AgentElicitationState, TaskId};

use crate::wake::WakeBus;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ElicitAnswer {
    Accept {
        #[serde(default)]
        content: serde_json::Value,
    },
    Decline,
    Cancel,
}

impl ElicitAnswer {
    fn durable(&self, answered_by: &str) -> ElicitationAnswer {
        let (state, answer) = match self {
            Self::Accept { content } => (
                AgentElicitationState::Answered,
                Some(match content.clone() {
                    value @ serde_json::Value::Object(_) => {
                        json_object(value, "elicitation answer").expect("object checked")
                    }
                    value => wrapped_json(value),
                }),
            ),
            Self::Decline => (AgentElicitationState::Declined, None),
            Self::Cancel => (AgentElicitationState::Cancelled, None),
        };
        ElicitationAnswer {
            state,
            answer,
            answered_by: answered_by.to_owned(),
        }
    }

    fn into_result(self) -> ElicitResult {
        match self {
            Self::Accept { content } => {
                ElicitResult::new(ElicitationAction::Accept).with_content(content)
            }
            Self::Decline => ElicitResult::new(ElicitationAction::Decline),
            Self::Cancel => ElicitResult::new(ElicitationAction::Cancel),
        }
    }
}

pub type ElicitationWaiters =
    Arc<Mutex<HashMap<AgentElicitationId, oneshot::Sender<ElicitAnswer>>>>;

pub struct ParkedElicitationHandler {
    runtime: AgentRuntime,
    bus: WakeBus,
    waiters: ElicitationWaiters,
    grace: Duration,
}

impl ParkedElicitationHandler {
    pub fn new(
        runtime: AgentRuntime,
        bus: WakeBus,
        waiters: ElicitationWaiters,
        grace: Duration,
    ) -> Self {
        Self {
            runtime,
            bus,
            waiters,
            grace,
        }
    }
}

pub async fn deliver_answer(
    runtime: &AgentRuntime,
    bus: &WakeBus,
    waiters: &ElicitationWaiters,
    elicitation_id: AgentElicitationId,
    answer: ElicitAnswer,
    answered_by: &str,
) -> anyhow::Result<()> {
    let wake_id = runtime
        .answer_elicitation(elicitation_id, answer.durable(answered_by))
        .await?;
    bus.hint(wake_id);
    let waiter = waiters
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&elicitation_id);
    if let Some(slot) = waiter {
        let _ = slot.send(answer);
    }
    Ok(())
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

            let (answer_tx, answer_rx) = oneshot::channel();
            self.waiters
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(elicitation_id, answer_tx);
            tracing::info!(%elicitation_id, related_task = ?related, "elicitation parked");

            match tokio::time::timeout(self.grace, answer_rx).await {
                Ok(Ok(answer)) => Ok(answer.into_result()),
                _ => {
                    self.waiters
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .remove(&elicitation_id);
                    Ok(ElicitResult::new(ElicitationAction::Decline))
                }
            }
        })
    }
}
