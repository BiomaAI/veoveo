//! One bounded agent episode, durably fenced and book-ended in SurrealDB.

use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use rig_core::{
    agent::{Agent, run::TaskDrainPolicy},
    tool::ToolCallExtensions,
};
use veoveo_agent_runtime::{AgentRuntime, EpisodeCompletion, json_object};
use veoveo_mcp_task_extension::TASK_RETENTION_PIN_META_KEY;
use veoveo_platform_store::{AgentEpisodeId, AgentEpisodeState, TaskId, WakeId};

use crate::{
    budget::{BUDGET_TERMINATED_PREFIX, BudgetHook},
    connection::GatewayConnection,
    context,
    llm::KernelModel,
    manifest::AgentManifest,
    memory::{EpisodeOutcome, MemoryStore},
    recorder::RecorderHook,
    rrd::RrdRecorder,
    summary,
};

pub struct EpisodeDriver {
    manifest: AgentManifest,
    agent: Agent<KernelModel>,
    runtime: AgentRuntime,
    memory: MemoryStore,
    rrd: Arc<RrdRecorder>,
}

#[derive(Debug)]
pub struct EpisodeReport {
    pub episode_id: AgentEpisodeId,
    pub seq: i64,
    pub output: String,
    pub detached_tasks: usize,
}

impl EpisodeDriver {
    pub fn new(
        manifest: AgentManifest,
        agent: Agent<KernelModel>,
        runtime: AgentRuntime,
        memory: MemoryStore,
        rrd: Arc<RrdRecorder>,
    ) -> Self {
        Self {
            manifest,
            agent,
            runtime,
            memory,
            rrd,
        }
    }

    pub async fn run_episode(
        &self,
        connection: &mut GatewayConnection,
        wake_note: &str,
        wake_body: &str,
        wake_ids: &[WakeId],
    ) -> Result<EpisodeReport> {
        connection
            .ensure_fresh()
            .await
            .context("refreshing the gateway connection")?;

        let episode = self.runtime.start_episode(wake_note).await?;
        self.memory.start_episode_projection(
            episode.episode_id.as_uuid(),
            episode.sequence,
            wake_note,
        )?;
        self.rrd.begin_episode(episode.sequence);
        tracing::info!(episode_id = %episode.episode_id, seq = episode.sequence, wake_note, "episode started");

        let pending = self.runtime.pending_task_count().await?;
        let unconsumed = self.runtime.unconsumed_task_results().await?.len();
        let prompt =
            context::assemble(&self.manifest, &self.memory, wake_body, pending, unconsumed)
                .context("assembling episode context")?;
        self.rrd
            .log_document("/agent/episodes", "text/markdown", prompt.clone());

        let recorder = RecorderHook::new(
            self.runtime.clone(),
            self.rrd.clone(),
            episode.episode_id,
            episode.retention_pin.clone(),
        );
        let tool_calls = recorder.tool_call_counter();
        let mut meta = rmcp::model::Meta::new();
        meta.0.insert(
            TASK_RETENTION_PIN_META_KEY.to_owned(),
            serde_json::json!(episode.retention_pin),
        );
        let mut extensions = ToolCallExtensions::new();
        extensions.insert(meta);
        let response = self
            .agent
            .runner(prompt)
            .tool_extensions(extensions)
            .add_hook(recorder)
            .add_hook(BudgetHook::new(self.manifest.budgets.per_episode.clone()))
            .max_turns(self.manifest.episode.max_turns)
            .task_deadline(self.manifest.task_deadline())
            .task_drain(TaskDrainPolicy::Detach)
            .run()
            .await;

        let tool_calls = tool_calls.load(std::sync::atomic::Ordering::Relaxed);
        match response {
            Ok(response) => {
                for descriptor in &response.unresolved_tasks {
                    let task_id = canonical_task_id(&descriptor.task_id)?;
                    let descriptor = json_object(
                        serde_json::to_value(descriptor)
                            .context("serializing detached task descriptor")?,
                        "task descriptor",
                    )?;
                    self.runtime
                        .complete_task_descriptor(task_id, descriptor)
                        .await?;
                    tracing::info!(
                        episode_id = %episode.episode_id,
                        %task_id,
                        tool_name = descriptor_tool_name(&response.unresolved_tasks, task_id),
                        "task detached"
                    );
                }
                let report = EpisodeReport {
                    episode_id: episode.episode_id,
                    seq: episode.sequence,
                    output: response.output,
                    detached_tasks: response.unresolved_tasks.len(),
                };
                let summary = summary::deterministic(&report, wake_note, tool_calls);
                self.runtime
                    .complete_episode(
                        episode.episode_id,
                        EpisodeCompletion {
                            state: AgentEpisodeState::Completed,
                            final_output: report.output.clone(),
                            summary: Some(summary.clone()),
                            input_tokens: response.usage.input_tokens,
                            output_tokens: response.usage.output_tokens,
                            completion_calls: response.completion_calls.len() as u64,
                            tool_calls,
                            error: None,
                        },
                        wake_ids,
                    )
                    .await?;
                self.memory.finish_episode_projection(
                    episode.episode_id.as_uuid(),
                    EpisodeOutcome::Completed,
                    &report.output,
                    response.usage.input_tokens,
                    response.usage.output_tokens,
                    response.completion_calls.len() as u64,
                    tool_calls,
                    None,
                )?;
                self.memory
                    .set_episode_projection_summary(episode.episode_id.as_uuid(), &summary)?;
                self.finish_rrd(summary);
                Ok(report)
            }
            Err(rig_core::completion::PromptError::PromptCancelled { reason, .. })
                if reason.starts_with(BUDGET_TERMINATED_PREFIX) =>
            {
                self.runtime
                    .complete_episode(
                        episode.episode_id,
                        EpisodeCompletion {
                            state: AgentEpisodeState::BudgetTerminated,
                            final_output: reason.clone(),
                            summary: None,
                            input_tokens: 0,
                            output_tokens: 0,
                            completion_calls: 0,
                            tool_calls,
                            error: None,
                        },
                        wake_ids,
                    )
                    .await?;
                self.memory.finish_episode_projection(
                    episode.episode_id.as_uuid(),
                    EpisodeOutcome::BudgetTerminated,
                    &reason,
                    0,
                    0,
                    0,
                    tool_calls,
                    None,
                )?;
                self.finish_rrd(format!("budget terminated: {reason}"));
                Ok(EpisodeReport {
                    episode_id: episode.episode_id,
                    seq: episode.sequence,
                    output: reason,
                    detached_tasks: 0,
                })
            }
            Err(error) => {
                self.runtime
                    .complete_episode(
                        episode.episode_id,
                        EpisodeCompletion {
                            state: AgentEpisodeState::Failed,
                            final_output: String::new(),
                            summary: None,
                            input_tokens: 0,
                            output_tokens: 0,
                            completion_calls: 0,
                            tool_calls,
                            error: Some(error.to_string()),
                        },
                        &[],
                    )
                    .await?;
                self.memory.finish_episode_projection(
                    episode.episode_id.as_uuid(),
                    EpisodeOutcome::Error,
                    "",
                    0,
                    0,
                    0,
                    tool_calls,
                    Some(&error.to_string()),
                )?;
                self.finish_rrd(format!("episode {} failed: {error:#}", episode.sequence));
                Err(error).context("running the episode")
            }
        }
    }

    fn finish_rrd(&self, text: String) {
        self.rrd.log_text("/agent/episodes", text);
        self.rrd.flush();
        if let Err(error) = self.rrd.rotate_if_needed() {
            tracing::warn!(%error, "rrd rotation failed");
        }
    }
}

fn canonical_task_id(value: &str) -> Result<TaskId> {
    let task_id = TaskId::from_str(value).context("task id is not a UUID")?;
    if task_id.as_uuid().get_version_num() != 7 {
        bail!("task id `{value}` is not the canonical UUIDv7 identity");
    }
    Ok(task_id)
}

fn descriptor_tool_name(
    descriptors: &[rig_core::tool::ToolTaskDescriptor],
    task_id: TaskId,
) -> &str {
    descriptors
        .iter()
        .find(|descriptor| descriptor.task_id == task_id.to_string())
        .map_or("unknown", |descriptor| descriptor.tool_name.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_noncanonical_task_ids() {
        assert!(canonical_task_id(&uuid::Uuid::new_v4().to_string()).is_err());
        assert!(canonical_task_id(&uuid::Uuid::now_v7().to_string()).is_ok());
    }
}
