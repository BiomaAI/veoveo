//! Parked elicitations: a tool's question becomes an operator work item.
//!
//! When a gateway tool elicits mid-task, an autonomous agent has no human at
//! the keyboard. The handler parks the question in the ledger, wakes the
//! scheduler, and holds the tool turn open for a short grace so an operator
//! already watching can answer inline (via the operator HTTP endpoint). Past
//! the grace, it declines with a pointer to the parked item — the task keeps
//! waiting server-side (`input_required` tasks stay open on this connection),
//! and the eventual answer wakes an episode that can re-drive it.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use rig_core::tool::rmcp::{McpElicitationHandler, related_task_id};
use rig_core::wasm_compat::WasmBoxedFuture;
use rmcp::model::{ElicitRequestParams, ElicitResult, ElicitationAction};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::{
    ledger::{ElicitationState, KernelLedger},
    wake::{WakeBus, WakeEvent},
};

/// An operator's verdict on a parked elicitation.
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
    pub fn state(&self) -> ElicitationState {
        match self {
            ElicitAnswer::Accept { .. } => ElicitationState::Answered,
            ElicitAnswer::Decline => ElicitationState::Declined,
            ElicitAnswer::Cancel => ElicitationState::Cancelled,
        }
    }

    fn into_result(self) -> ElicitResult {
        match self {
            ElicitAnswer::Accept { content } => {
                ElicitResult::new(ElicitationAction::Accept).with_content(content)
            }
            ElicitAnswer::Decline => ElicitResult::new(ElicitationAction::Decline),
            ElicitAnswer::Cancel => ElicitResult::new(ElicitationAction::Cancel),
        }
    }
}

/// In-grace waiters: elicitation id → the open tool turn's answer slot.
pub type ElicitationWaiters = Arc<Mutex<HashMap<Uuid, oneshot::Sender<ElicitAnswer>>>>;

pub struct ParkedElicitationHandler {
    ledger: KernelLedger,
    bus: WakeBus,
    waiters: ElicitationWaiters,
    grace: Duration,
}

impl ParkedElicitationHandler {
    pub fn new(
        ledger: KernelLedger,
        bus: WakeBus,
        waiters: ElicitationWaiters,
        grace: Duration,
    ) -> Self {
        Self {
            ledger,
            bus,
            waiters,
            grace,
        }
    }
}

/// Deliver an operator answer: settles the ledger row, completes an in-grace
/// waiter inline, or wakes the scheduler for the parked path.
pub async fn deliver_answer(
    ledger: &KernelLedger,
    bus: &WakeBus,
    waiters: &ElicitationWaiters,
    elicitation_id: Uuid,
    answer: ElicitAnswer,
    answered_by: &str,
) -> anyhow::Result<()> {
    let answer_json = match &answer {
        ElicitAnswer::Accept { content } => Some(content.to_string()),
        _ => None,
    };
    ledger.answer_elicitation(
        elicitation_id,
        answer.state(),
        answer_json.as_deref(),
        answered_by,
    )?;
    let waiter = waiters
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&elicitation_id);
    match waiter {
        Some(slot) => {
            // The tool turn is still open; the answer flows straight back.
            let _ = slot.send(answer);
        }
        None => {
            bus.send(WakeEvent::elicitation_answered(elicitation_id))
                .await;
        }
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

            let elicitation_id = Uuid::new_v4();
            let related = related_task_id(meta.as_ref());
            let schema_json = serde_json::to_string(requested_schema).ok();
            if let Err(err) = self.ledger.park_elicitation(
                elicitation_id,
                related.as_deref(),
                message,
                schema_json.as_deref(),
            ) {
                tracing::error!(%err, "parking elicitation failed");
                return Ok(ElicitResult::new(ElicitationAction::Decline));
            }

            let (answer_tx, answer_rx) = oneshot::channel();
            self.waiters
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(elicitation_id, answer_tx);
            self.bus
                .send(WakeEvent::elicitation_pending(elicitation_id))
                .await;
            tracing::info!(
                %elicitation_id,
                related_task = related.as_deref().unwrap_or("-"),
                "elicitation parked"
            );

            match tokio::time::timeout(self.grace, answer_rx).await {
                Ok(Ok(answer)) => Ok(answer.into_result()),
                _ => {
                    // Grace elapsed: release the waiter slot; the parked row
                    // stays open for the asynchronous answer path.
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
