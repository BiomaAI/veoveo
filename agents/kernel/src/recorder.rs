//! Decision-log hook plus durable task-delivery binding.

use std::str::FromStr;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use rig_core::{
    agent::{
        AgentHook, HookContext, ModelTurnFinished, ObservationAction, ToolCall, ToolCallAction,
        ToolResultAction, ToolResultEvent, ToolTaskResultAction, ToolTaskResultEvent,
        ToolTaskStartedAction, ToolTaskStartedEvent, ToolTaskStatusAction, ToolTaskStatusEvent,
    },
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
    async fn on_tool_call(&self, ctx: &HookContext, event: ToolCall<'_>) -> ToolCallAction {
        let episode = self.episode_id;
        self.tool_calls.fetch_add(1, Ordering::Relaxed);
        self.rrd.log_text(
            &format!("/agent/tools/{}", event.tool_name),
            format!(
                "call {} [retention_pin={}, call={}]",
                capped(event.args),
                self.retention_pin,
                event.internal_call_id
            ),
        );
        tracing::info!(
            %episode,
            tool_name = event.tool_name,
            internal_call_id = event.internal_call_id,
            turn = ctx.turn(),
            "tool call"
        );
        ToolCallAction::run()
    }

    async fn on_tool_result(
        &self,
        _ctx: &HookContext,
        event: ToolResultEvent<'_>,
    ) -> ToolResultAction {
        let result = event.presentation.render();
        self.rrd.log_text(
            &format!("/agent/tools/{}", event.tool_name),
            format!(
                "{} {}",
                if event.raw_result.is_error() {
                    "error"
                } else {
                    "result"
                },
                capped(&result)
            ),
        );
        ToolResultAction::keep()
    }

    async fn on_tool_task_started(
        &self,
        _ctx: &HookContext,
        event: ToolTaskStartedEvent<'_>,
    ) -> ToolTaskStartedAction {
        let task_id = match canonical_task_id(event.task_id) {
            Ok(task_id) => task_id,
            Err(error) => return ToolTaskStartedAction::stop(error),
        };
        let descriptor = ToolTaskDescriptor::new(
            ToolTaskDescriptor::BACKEND_MCP,
            task_id.to_string(),
            event.tool_name,
        );
        let descriptor = match serde_json::to_value(descriptor)
            .ok()
            .and_then(|value| json_object(value, "task descriptor").ok())
        {
            Some(descriptor) => descriptor,
            None => return ToolTaskStartedAction::stop("serializing task descriptor failed"),
        };
        if let Err(error) = self
            .runtime
            .record_task(NewAgentTask {
                task_id,
                tool_name: event.tool_name.to_owned(),
                descriptor,
                descriptor_complete: false,
                retention_pin: self.retention_pin.clone(),
                started_by_episode: self.episode_id,
            })
            .await
        {
            return ToolTaskStartedAction::stop(format!(
                "persisting task delivery before detach failed: {error}"
            ));
        }
        self.rrd.log_text(
            &format!("/agent/tasks/{task_id}"),
            format!(
                "started {}{}",
                event.tool_name,
                event
                    .immediate_response
                    .map(|hint| format!(": {}", capped(hint)))
                    .unwrap_or_default()
            ),
        );
        ToolTaskStartedAction::continue_task()
    }

    async fn on_tool_task_status(
        &self,
        _ctx: &HookContext,
        event: ToolTaskStatusEvent<'_>,
    ) -> ToolTaskStatusAction {
        self.rrd.log_text(
            &format!("/agent/tasks/{}", event.task_id),
            event.status.as_str(),
        );
        ToolTaskStatusAction::continue_task()
    }

    async fn on_tool_task_result(
        &self,
        _ctx: &HookContext,
        event: ToolTaskResultEvent<'_>,
    ) -> ToolTaskResultAction {
        let task_id = match canonical_task_id(event.task_id) {
            Ok(task_id) => task_id,
            Err(error) => return ToolTaskResultAction::stop(error),
        };
        let result = event.presentation.render();
        let is_error = event.raw_result.is_error();
        let payload = match json_object(
            serde_json::json!({ "output": result, "delivered": "in_run" }),
            "task result",
        ) {
            Ok(payload) => payload,
            Err(error) => return ToolTaskResultAction::stop(error.to_string()),
        };
        if let Err(error) = self
            .runtime
            .resolve_task_in_episode(task_id, self.episode_id, payload, is_error)
            .await
        {
            return ToolTaskResultAction::stop(format!(
                "persisting in-run task result failed: {error}"
            ));
        }
        self.rrd.log_text(
            &format!("/agent/tasks/{task_id}"),
            format!(
                "{} in run: {}",
                if is_error { "failed" } else { "resolved" },
                capped(&result)
            ),
        );
        ToolTaskResultAction::keep()
    }

    async fn on_model_turn_finished(
        &self,
        _ctx: &HookContext,
        event: ModelTurnFinished<'_>,
    ) -> ObservationAction {
        self.rrd.log_scalars(
            "/agent/llm",
            [
                event.usage.input_tokens as f64,
                event.usage.output_tokens as f64,
            ],
        );
        tracing::info!(
            episode = %self.episode_id,
            turn = event.turn,
            input_tokens = event.usage.input_tokens,
            output_tokens = event.usage.output_tokens,
            "model turn finished"
        );
        ObservationAction::continue_run()
    }
}

fn canonical_task_id(value: &str) -> Result<TaskId, String> {
    let task_id = TaskId::from_str(value).map_err(|error| format!("invalid task id: {error}"))?;
    if task_id.as_uuid().get_version_num() != 7 {
        return Err(format!("task id `{value}` is not UUIDv7"));
    }
    Ok(task_id)
}
