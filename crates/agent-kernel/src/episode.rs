//! The episode driver: one bounded agent run, durably book-ended.
//!
//! An episode starts from a wake (boot prompt, task result, later: timers and
//! operators), runs the rig agent with `TaskDrainPolicy::Detach`, and ends by
//! persisting whatever is still in flight: every unresolved task descriptor
//! goes to the ledger for the watchers, usage lands on the episode row. The
//! next episode assembles context fresh — nothing rides in model history.

use anyhow::{Context, Result};
use rig_core::agent::{Agent, run::TaskDrainPolicy};
use uuid::Uuid;

use crate::{
    connection::GatewayConnection,
    ledger::{EpisodeOutcome, KernelLedger},
    llm::KernelModel,
    manifest::AgentManifest,
    recorder::RecorderHook,
};

pub struct EpisodeDriver {
    manifest: AgentManifest,
    agent: Agent<KernelModel>,
    ledger: KernelLedger,
}

#[derive(Debug)]
pub struct EpisodeReport {
    pub episode_id: Uuid,
    pub seq: i64,
    pub output: String,
    pub detached_tasks: usize,
}

impl EpisodeDriver {
    pub fn new(manifest: AgentManifest, agent: Agent<KernelModel>, ledger: KernelLedger) -> Self {
        Self {
            manifest,
            agent,
            ledger,
        }
    }

    pub async fn run_episode(
        &self,
        connection: &mut GatewayConnection,
        wake_note: &str,
        prompt: String,
    ) -> Result<EpisodeReport> {
        connection
            .ensure_fresh()
            .await
            .context("refreshing the gateway connection")?;

        let episode_id = Uuid::new_v4();
        let seq = self.ledger.begin_episode(episode_id, wake_note)?;
        tracing::info!(%episode_id, seq, wake_note, "episode started");

        let recorder = RecorderHook::new(self.ledger.clone(), episode_id);
        let tool_calls = recorder.tool_call_counter();
        let response = self
            .agent
            .runner(prompt)
            .add_hook(recorder)
            .max_turns(self.manifest.episode.max_turns)
            .task_deadline(self.manifest.task_deadline())
            .task_poll_interval(self.manifest.task_poll_interval())
            .task_drain(TaskDrainPolicy::Detach)
            .run()
            .await;

        let tool_calls = tool_calls.load(std::sync::atomic::Ordering::Relaxed);
        match response {
            Ok(response) => {
                for descriptor in &response.unresolved_tasks {
                    let descriptor_json = serde_json::to_string(descriptor)
                        .context("serializing a detached task descriptor")?;
                    self.ledger.record_detached_task(
                        &descriptor.task_id,
                        &descriptor.tool_name,
                        descriptor.server_key.as_deref(),
                        &descriptor_json,
                        episode_id,
                    )?;
                    tracing::info!(
                        %episode_id,
                        task_id = descriptor.task_id,
                        tool_name = descriptor.tool_name,
                        "task detached"
                    );
                }
                self.ledger.finish_episode(
                    episode_id,
                    EpisodeOutcome::Completed,
                    &response.output,
                    response.usage.input_tokens,
                    response.usage.output_tokens,
                    response.completion_calls.len() as u64,
                    tool_calls,
                    None,
                )?;
                tracing::info!(
                    %episode_id,
                    seq,
                    detached_tasks = response.unresolved_tasks.len(),
                    outcome = EpisodeOutcome::Completed.as_str(),
                    "episode completed"
                );
                Ok(EpisodeReport {
                    episode_id,
                    seq,
                    output: response.output,
                    detached_tasks: response.unresolved_tasks.len(),
                })
            }
            Err(err) => {
                self.ledger.finish_episode(
                    episode_id,
                    EpisodeOutcome::Error,
                    "",
                    0,
                    0,
                    0,
                    tool_calls,
                    Some(&err.to_string()),
                )?;
                tracing::error!(%episode_id, seq, %err, "episode failed");
                Err(err).context("running the episode")
            }
        }
    }
}
