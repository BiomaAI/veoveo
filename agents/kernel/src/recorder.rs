//! Decision-log hook plus durable task-delivery binding.

use std::str::FromStr;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use rig_core::{
    agent::{AgentHook, Flow, HookContext, StepEvent},
    completion::CompletionModel,
    tool::ToolTaskDescriptor,
};
use veoveo_agent_runtime::{AgentRuntime, NewAgentTask, json_object};
use veoveo_platform_store::{AgentEpisodeId, TaskId};
use veoveo_task_runtime::TaskRetentionPin;

use crate::rrd::RrdRecorder;

const RRD_PAYLOAD_CAP: usize = 8 * 1024;

fn capped(text: &str) -> String {
    if text.len() <= RRD_PAYLOAD_CAP {
        text.to_string()
    } else {
        let mut end = RRD_PAYLOAD_CAP;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}... (+{} bytes)", &text[..end], text.len() - end)
    }
}

pub struct RecorderHook {
    runtime: AgentRuntime,
    rrd: Arc<RrdRecorder>,
    episode_id: AgentEpisodeId,
    retention_pin: TaskRetentionPin,
    tool_calls: Arc<AtomicU64>,
}

impl RecorderHook {
    pub fn new(
        runtime: AgentRuntime,
        rrd: Arc<RrdRecorder>,
        episode_id: AgentEpisodeId,
        retention_pin: TaskRetentionPin,
    ) -> Self {
        Self {
            runtime,
            rrd,
            episode_id,
            retention_pin,
            tool_calls: Arc::new(AtomicU64::new(0)),
        }
    }

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
                tool_name,
                internal_call_id,
                args,
                ..
            } => {
                self.tool_calls.fetch_add(1, Ordering::Relaxed);
                self.rrd.log_text(
                    &format!("/agent/tools/{tool_name}"),
                    format!(
                        "call {} [retention_pin={}, call={internal_call_id}]",
                        capped(args),
                        self.retention_pin
                    ),
                );
                tracing::info!(%episode, tool_name, internal_call_id, turn = ctx.turn(), "tool call");
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
            }
            StepEvent::ToolTaskStarted {
                tool_name,
                task_id,
                immediate_response,
                ..
            } => {
                let task_id = match canonical_task_id(task_id) {
                    Ok(task_id) => task_id,
                    Err(error) => return Flow::terminate(error),
                };
                let descriptor = ToolTaskDescriptor::new(
                    ToolTaskDescriptor::BACKEND_MCP,
                    task_id.to_string(),
                    tool_name,
                );
                let descriptor = match serde_json::to_value(descriptor)
                    .ok()
                    .and_then(|value| json_object(value, "task descriptor").ok())
                {
                    Some(descriptor) => descriptor,
                    None => return Flow::terminate("serializing task descriptor failed"),
                };
                if let Err(error) = self
                    .runtime
                    .record_task(NewAgentTask {
                        task_id,
                        tool_name: tool_name.to_owned(),
                        descriptor,
                        descriptor_complete: false,
                        retention_pin: self.retention_pin.clone(),
                        started_by_episode: episode,
                    })
                    .await
                {
                    return Flow::terminate(format!(
                        "persisting task delivery before detach failed: {error}"
                    ));
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
            }
            StepEvent::ToolTaskStatus {
                task_id, status, ..
            } => {
                self.rrd
                    .log_text(&format!("/agent/tasks/{task_id}"), status.as_str());
            }
            StepEvent::ToolTaskResult {
                task_id,
                result,
                outcome,
                ..
            } => {
                let task_id = match canonical_task_id(task_id) {
                    Ok(task_id) => task_id,
                    Err(error) => return Flow::terminate(error),
                };
                let payload = match json_object(
                    serde_json::json!({ "output": result, "delivered": "in_run" }),
                    "task result",
                ) {
                    Ok(payload) => payload,
                    Err(error) => return Flow::terminate(error.to_string()),
                };
                if let Err(error) = self
                    .runtime
                    .resolve_task_in_episode(task_id, episode, payload, outcome.is_error())
                    .await
                {
                    return Flow::terminate(format!(
                        "persisting in-run task result failed: {error}"
                    ));
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
            }
            StepEvent::ModelTurnFinished { turn, usage, .. } => {
                self.rrd.log_scalars(
                    "/agent/llm",
                    [usage.input_tokens as f64, usage.output_tokens as f64],
                );
                tracing::info!(%episode, turn, input_tokens = usage.input_tokens, output_tokens = usage.output_tokens, "model turn finished");
            }
            _ => {}
        }
        Flow::cont()
    }
}

fn canonical_task_id(value: &str) -> Result<TaskId, String> {
    let task_id = TaskId::from_str(value).map_err(|error| format!("invalid task id: {error}"))?;
    if task_id.as_uuid().get_version_num() != 7 {
        return Err(format!("task id `{value}` is not UUIDv7"));
    }
    Ok(task_id)
}
