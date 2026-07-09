//! The decision recorder hook: every step observed, both planes written.
//!
//! Rule of the split: the RRD plane gets everything (append-only, time-indexed
//! decision log); the DuckDB plane gets only durable kernel facts needed for
//! crash safety mid-run — a provisional task row the moment a deferred
//! dispatch is observed, and immediate resolution for tasks that complete
//! inside the run.

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

use crate::{ledger::KernelLedger, rrd::RrdRecorder};

const RRD_PAYLOAD_CAP: usize = 8 * 1024;

fn capped(text: &str) -> String {
    if text.len() <= RRD_PAYLOAD_CAP {
        text.to_string()
    } else {
        let mut end = RRD_PAYLOAD_CAP;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}… (+{} bytes)", &text[..end], text.len() - end)
    }
}

pub struct RecorderHook {
    ledger: KernelLedger,
    rrd: Arc<RrdRecorder>,
    episode_id: Uuid,
    tool_calls: Arc<AtomicU64>,
}

impl RecorderHook {
    pub fn new(ledger: KernelLedger, rrd: Arc<RrdRecorder>, episode_id: Uuid) -> Self {
        Self {
            ledger,
            rrd,
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
            StepEvent::ToolCall {
                tool_name, args, ..
            } => {
                self.tool_calls.fetch_add(1, Ordering::Relaxed);
                self.rrd.log_text(
                    &format!("/agent/tools/{tool_name}"),
                    format!("call {}", capped(args)),
                );
                tracing::info!(%episode, tool_name, turn = ctx.turn(), "tool call");
            }
            StepEvent::ToolResult {
                tool_name,
                result,
                outcome,
                ..
            } => {
                self.rrd.log_text(
                    &format!("/agent/tools/{tool_name}"),
                    format!(
                        "{} {}",
                        if outcome.is_error() {
                            "error"
                        } else {
                            "result"
                        },
                        capped(result)
                    ),
                );
                tracing::info!(
                    %episode,
                    tool_name,
                    is_error = outcome.is_error(),
                    "tool result"
                );
            }
            StepEvent::ToolTaskStarted {
                tool_name,
                task_id,
                immediate_response,
                ..
            } => {
                // Crash safety: a minimal descriptor (backend + task id + tool
                // name) is enough for McpTaskResumer to rehydrate the task if
                // the process dies before the episode persists the full one.
                let descriptor =
                    ToolTaskDescriptor::new(ToolTaskDescriptor::BACKEND_MCP, task_id, tool_name);
                match serde_json::to_string(&descriptor) {
                    Ok(descriptor_json) => {
                        if let Err(err) = self.ledger.record_provisional_task(
                            task_id,
                            tool_name,
                            &descriptor_json,
                            episode,
                        ) {
                            tracing::error!(%episode, task_id, %err, "provisional task row failed");
                        }
                    }
                    Err(err) => {
                        tracing::error!(%episode, task_id, %err, "descriptor serialization failed");
                    }
                }
                self.rrd.log_text(
                    &format!("/agent/tasks/{task_id}"),
                    format!(
                        "started {tool_name}{}",
                        immediate_response
                            .map(|hint| format!(": {}", capped(hint)))
                            .unwrap_or_default()
                    ),
                );
                tracing::info!(%episode, tool_name, task_id, "task started");
            }
            StepEvent::ToolTaskStatus {
                task_id, status, ..
            } => {
                self.rrd
                    .log_text(&format!("/agent/tasks/{task_id}"), status.as_str());
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
                self.rrd.log_text(
                    &format!("/agent/tasks/{task_id}"),
                    format!(
                        "{} in run: {}",
                        if outcome.is_error() {
                            "failed"
                        } else {
                            "resolved"
                        },
                        capped(result)
                    ),
                );
                tracing::info!(
                    %episode,
                    tool_name,
                    task_id,
                    is_error = outcome.is_error(),
                    "task resolved in run"
                );
            }
            StepEvent::ModelTurnFinished { turn, usage, .. } => {
                self.rrd.log_scalars(
                    "/agent/llm",
                    [usage.input_tokens as f64, usage.output_tokens as f64],
                );
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
