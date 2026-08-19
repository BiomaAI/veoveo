//! One bounded agent episode, durably fenced and book-ended in SurrealDB.

use std::sync::Arc;

use anyhow::{Context, Result};
use rig::{
    agent::Agent,
    tool::{
        DeferredExecutionPolicy, DeferredToolResolver, DeferredToolResolverRegistry, ToolContext,
    },
};
use veoveo_agent_runtime::{AgentRuntime, EpisodeCompletion};
use veoveo_platform_store::{AgentEpisodeId, AgentEpisodeState, WakeId};
use veoveo_task_runtime::TASK_RETENTION_PIN_META_KEY;

use crate::{
    background_tasks::BackgroundTaskResolver,
    budget::{BUDGET_TERMINATED_PREFIX, BudgetHook},
    connection::GatewayConnection,
    context,
    input::DurableInputHandler,
    manifest::AgentManifest,
    memory::{EpisodeOutcome, MemoryStore},
    recorder::RecorderHook,
    resource::{ResourceReadLedger, ResourceReadLimits},
    rrd::RrdRecorder,
    summary,
};

pub struct EpisodeDriver {
    manifest: AgentManifest,
    agent: Agent,
    runtime: AgentRuntime,
    memory: MemoryStore,
    rrd: Arc<RrdRecorder>,
    resource_read_limits: ResourceReadLimits,
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
        agent: Agent,
        runtime: AgentRuntime,
        memory: MemoryStore,
        rrd: Arc<RrdRecorder>,
        resource_read_limits: ResourceReadLimits,
    ) -> Self {
        Self {
            manifest,
            agent,
            runtime,
            memory,
            rrd,
            resource_read_limits,
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
        let detached_tasks = recorder.detached_task_counter();
        let mut meta = rmcp::model::RequestMetaObject::new();
        meta.insert(
            TASK_RETENTION_PIN_META_KEY.to_owned(),
            serde_json::json!(episode.retention_pin),
        );
        let mut tool_context = ToolContext::new();
        tool_context.insert(meta);
        tool_context.insert(ResourceReadLedger::new(self.resource_read_limits.clone()));
        let resolvers = DeferredToolResolverRegistry::new();
        if let Some(resolver) = connection.epoch().resolver {
            resolvers
                .register(BackgroundTaskResolver::new(resolver.backend_type()))
                .context("registering gateway background-task resolver")?;
        }
        let mut deferred_policy = DeferredExecutionPolicy::default();
        deferred_policy.timeout = self.manifest.task_deadline();
        deferred_policy.working_poll_interval = std::time::Duration::from_millis(250);
        deferred_policy.max_state_reads = 10_000;
        let response = self
            .agent
            .runner(prompt)
            .tool_context(tool_context)
            .deferred_tool_resolvers(resolvers)
            .deferred_input_handler(DurableInputHandler::new(
                self.runtime.clone(),
                connection.handlers().bus.clone(),
                connection.handlers().input_grace,
            ))
            .deferred_execution_policy(deferred_policy)
            .add_hook(recorder)
            .add_hook(BudgetHook::new(self.manifest.budgets.per_episode.clone()))
            .max_turns(self.manifest.episode.max_turns)
            .run()
            .await;

        let tool_calls = tool_calls.load(std::sync::atomic::Ordering::Relaxed);
        match response {
            Ok(response) => {
                let detached_tasks =
                    detached_tasks.load(std::sync::atomic::Ordering::Relaxed) as usize;
                if response.output.trim().is_empty() && detached_tasks == 0 {
                    let error =
                        "model completed an episode without a final response or detached task";
                    self.runtime
                        .complete_episode(
                            episode.episode_id,
                            EpisodeCompletion {
                                state: AgentEpisodeState::Failed,
                                final_output: String::new(),
                                summary: None,
                                input_tokens: response.usage.input_tokens,
                                output_tokens: response.usage.output_tokens,
                                completion_calls: response.completion_calls.len() as u64,
                                tool_calls,
                                error: Some(error.to_owned()),
                            },
                            &[],
                        )
                        .await?;
                    self.memory.finish_episode_projection(
                        episode.episode_id.as_uuid(),
                        EpisodeOutcome::Error,
                        "",
                        response.usage.input_tokens,
                        response.usage.output_tokens,
                        response.completion_calls.len() as u64,
                        tool_calls,
                        Some(error),
                    )?;
                    self.finish_rrd(format!("episode {} failed: {error}", episode.sequence));
                    anyhow::bail!(error);
                }
                let report = EpisodeReport {
                    episode_id: episode.episode_id,
                    seq: episode.sequence,
                    output: response.output,
                    detached_tasks,
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
                tracing::info!(
                    episode_id = %episode.episode_id,
                    seq = episode.sequence,
                    detached_tasks = report.detached_tasks,
                    "episode completed"
                );
                Ok(report)
            }
            Err(rig::completion::PromptError::PromptCancelled { reason, .. })
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
                    detached_tasks: detached_tasks.load(std::sync::atomic::Ordering::Relaxed)
                        as usize,
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
