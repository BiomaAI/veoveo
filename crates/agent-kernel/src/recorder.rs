//! The flight-recorder hook: every step observed, durable facts persisted.
//!
//! Slice 1 records to tracing (structured JSON logs) plus the crash-safety
//! rows the ledger needs mid-run: a provisional task row the moment a
//! deferred dispatch is observed, and immediate resolution for tasks that
//! complete inside the run. The RRD plane joins in slice 2 behind the same
//! events.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use rig_core::{
    agent::{AgentHook, Flow, HookContext, StepEvent},
    completion::CompletionModel,
    tool::ToolTaskDescriptor,
};
use uuid::Uuid;

use crate::ledger::KernelLedger;

pub struct RecorderHook {
    ledger: KernelLedger,
    episode_id: Uuid,
    tool_calls: Arc<AtomicU64>,
}

impl RecorderHook {
    pub fn new(ledger: KernelLedger, episode_id: Uuid) -> Self {
        Self {
            ledger,
            episode_id,
            tool_calls: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Shared counter the episode driver reads after the run.
    pub fn tool_call_counter(&self) -> Arc<AtomicU64> {
        self.tool_calls.clone()
    }
}

impl<M> AgentHook<M> for RecorderHook
where
    M: CompletionModel,
{
    async fn on_event(&self, ctx: &HookContext, event: StepEvent<'_, M>) -> Flow {
        let episode = self.episode_id;
        match event {
            StepEvent::ToolCall { tool_name, .. } => {
                self.tool_calls.fetch_add(1, Ordering::Relaxed);
                tracing::info!(%episode, tool_name, turn = ctx.turn(), "tool call");
            }
            StepEvent::ToolResult {
                tool_name, outcome, ..
            } => {
                tracing::info!(
                    %episode,
                    tool_name,
                    is_error = outcome.is_error(),
                    "tool result"
                );
            }
            StepEvent::ToolTaskStarted {
                tool_name, task_id, ..
            } => {
                // Crash safety: a minimal descriptor (backend + task id + tool
                // name) is enough for McpTaskResumer to rehydrate the task if
                // the process dies before the episode persists the full one.
                let descriptor =
                    ToolTaskDescriptor::new(ToolTaskDescriptor::BACKEND_MCP, task_id, tool_name);
                let descriptor_json = match serde_json::to_string(&descriptor) {
                    Ok(json) => json,
                    Err(err) => {
                        tracing::error!(%episode, task_id, %err, "descriptor serialization failed");
                        return Flow::cont();
                    }
                };
                if let Err(err) = self.ledger.record_provisional_task(
                    task_id,
                    tool_name,
                    &descriptor_json,
                    episode,
                ) {
                    tracing::error!(%episode, task_id, %err, "provisional task row failed");
                }
                tracing::info!(%episode, tool_name, task_id, "task started");
            }
            StepEvent::ToolTaskStatus {
                task_id, status, ..
            } => {
                tracing::info!(%episode, task_id, status = status.as_str(), "task status");
            }
            StepEvent::ToolTaskResult {
                tool_name,
                task_id,
                result,
                outcome,
                ..
            } => {
                // The task resolved inside this run; the model already saw the
                // result, so record and consume it in one step.
                let result_json =
                    serde_json::json!({ "output": result, "delivered": "in_run" }).to_string();
                if let Err(err) =
                    self.ledger
                        .resolve_task(task_id, &result_json, outcome.is_error())
                {
                    tracing::error!(%episode, task_id, %err, "in-run task resolution failed");
                } else if let Err(err) = self.ledger.mark_task_consumed(task_id, episode) {
                    tracing::error!(%episode, task_id, %err, "in-run task consumption failed");
                }
                tracing::info!(
                    %episode,
                    tool_name,
                    task_id,
                    is_error = outcome.is_error(),
                    "task resolved in run"
                );
            }
            StepEvent::ModelTurnFinished { turn, usage, .. } => {
                tracing::info!(
                    %episode,
                    turn,
                    input_tokens = usage.input_tokens,
                    output_tokens = usage.output_tokens,
                    "model turn finished"
                );
            }
            _ => {}
        }
        Flow::cont()
    }
}
