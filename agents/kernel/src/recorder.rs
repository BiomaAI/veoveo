//! Decision-log hook plus durable deferred-tool delivery binding.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use rig::{
    agent::{
        AgentHook, DeferredToolEvent, HookContext, ModelTurnAction, ModelTurnFinished,
        ObservationAction, ToolCall, ToolCallAction, ToolResultAction, ToolResultEvent,
    },
    tool::DeferredToolLifecycleEvent,
};
use tokio::sync::Mutex;
use veoveo_agent_runtime::{AgentRuntime, NewAgentTask, json_object};
use veoveo_mcp_contract::CanonicalTaskId;
use veoveo_platform_store::AgentEpisodeId;
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
    deferred: Mutex<HashMap<String, CanonicalTaskId>>,
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
            deferred: Mutex::new(HashMap::new()),
        }
    }

    pub fn tool_call_counter(&self) -> Arc<AtomicU64> {
        self.tool_calls.clone()
    }

    async fn settle_without_result(
        &self,
        internal_call_id: &str,
        status: &str,
    ) -> Result<(), String> {
        let Some(task_id) = self.deferred.lock().await.remove(internal_call_id) else {
            return Ok(());
        };
        let payload = json_object(
            serde_json::json!({ "error": status, "delivered": "in_run" }),
            "deferred task result",
        )
        .map_err(|error| error.to_string())?;
        self.runtime
            .resolve_task_in_episode(task_id, self.episode_id, payload, true)
            .await
            .map_err(|error| error.to_string())
    }
}

impl AgentHook for RecorderHook {
    async fn on_tool_call(&self, ctx: &HookContext, event: ToolCall<'_>) -> ToolCallAction {
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
            episode = %self.episode_id,
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
        if let Some(task_id) = self.deferred.lock().await.remove(event.internal_call_id) {
            let payload = match json_object(
                serde_json::json!({ "output": result, "delivered": "in_run" }),
                "task result",
            ) {
                Ok(payload) => payload,
                Err(error) => return ToolResultAction::stop(error.to_string()),
            };
            if let Err(error) = self
                .runtime
                .resolve_task_in_episode(
                    task_id,
                    self.episode_id,
                    payload,
                    event.raw_result.is_error(),
                )
                .await
            {
                return ToolResultAction::stop(format!(
                    "persisting in-run deferred result failed: {error}"
                ));
            }
        }
        ToolResultAction::keep()
    }

    async fn on_deferred_tool_event(
        &self,
        _ctx: &HookContext,
        event: DeferredToolEvent<'_>,
    ) -> ObservationAction {
        let task_id = match CanonicalTaskId::new(event.descriptor.execution_id().to_owned()) {
            Ok(task_id) => task_id,
            Err(error) => return ObservationAction::stop(error.to_string()),
        };
        match event.lifecycle {
            DeferredToolLifecycleEvent::Started => {
                let descriptor = match serde_json::to_value(event.descriptor)
                    .ok()
                    .and_then(|value| json_object(value, "deferred descriptor").ok())
                {
                    Some(descriptor) => descriptor,
                    None => {
                        return ObservationAction::stop("serializing deferred descriptor failed");
                    }
                };
                if let Err(error) = self
                    .runtime
                    .record_task(NewAgentTask {
                        task_id: task_id.clone(),
                        tool_name: event.tool_name.to_owned(),
                        descriptor,
                        descriptor_complete: true,
                        retention_pin: self.retention_pin.clone(),
                        started_by_episode: self.episode_id,
                    })
                    .await
                {
                    return ObservationAction::stop(format!(
                        "persisting deferred descriptor failed: {error}"
                    ));
                }
                self.deferred
                    .lock()
                    .await
                    .insert(event.internal_call_id.to_owned(), task_id.clone());
                self.rrd.log_text(
                    &format!("/agent/tasks/{task_id}"),
                    format!("started {}", event.tool_name),
                );
            }
            DeferredToolLifecycleEvent::Failed { message } => {
                if let Err(error) = self
                    .settle_without_result(event.internal_call_id, message)
                    .await
                {
                    return ObservationAction::stop(error);
                }
            }
            DeferredToolLifecycleEvent::Cancelled => {
                if let Err(error) = self
                    .settle_without_result(event.internal_call_id, "cancelled")
                    .await
                {
                    return ObservationAction::stop(error);
                }
            }
            lifecycle => self
                .rrd
                .log_text(&format!("/agent/tasks/{task_id}"), format!("{lifecycle:?}")),
        }
        ObservationAction::continue_run()
    }

    async fn on_model_turn_finished(
        &self,
        _ctx: &HookContext,
        event: ModelTurnFinished<'_>,
    ) -> ModelTurnAction {
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
        ModelTurnAction::continue_run()
    }
}
