//! One bounded agent episode, durably fenced and book-ended in SurrealDB.

use std::sync::Arc;

use anyhow::{Context, Result};
use rig::{
    agent::Agent,
    tool::{
        DeferredExecutionPolicy, DeferredToolResolver, DeferredToolResolverRegistry, ToolContext,
    },
};
use veoveo_agent_runtime::{AgentRuntime, EpisodeCompletion, EpisodeHandle};
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

/// Recorded when the gateway can't confirm a managed agent may keep running.
/// The underlying error goes to the kernel log.
const DISPATCH_UNCONFIRMED: &str =
    "The run stopped because the gateway couldn't confirm this agent may keep running.";

pub struct EpisodeDriver {
    manifest: AgentManifest,
    agent: Agent,
    runtime: AgentRuntime,
    memory: MemoryStore,
    rrd: Arc<RrdRecorder>,
    resource_read_limits: ResourceReadLimits,
    managed_deadline: Option<std::time::Duration>,
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
            managed_deadline: None,
        }
    }

    pub fn with_managed_deadline(mut self, deadline: Option<std::time::Duration>) -> Self {
        self.managed_deadline = deadline;
        self
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

        with_started_episode(&self.runtime, wake_note, |episode| {
            self.run_admitted_episode(connection, episode, wake_note, wake_body, wake_ids)
        })
        .await
    }

    async fn run_admitted_episode(
        &self,
        connection: &mut GatewayConnection,
        episode: EpisodeHandle,
        wake_note: &str,
        wake_body: &str,
        wake_ids: &[WakeId],
    ) -> Result<EpisodeReport> {
        let deadline = self
            .managed_deadline
            .map(|limit| tokio::time::Instant::now() + limit);
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
        let run = self
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
            .add_hook(
                BudgetHook::new(self.manifest.budgets.per_episode.clone())
                    .with_managed_dispatch(connection.clone(), episode.managed.clone()),
            )
            .max_turns(self.manifest.episode.max_turns)
            .run();
        let bounded = async {
            if let Some(deadline) = deadline {
                tokio::time::timeout_at(deadline, run)
                    .await
                    .unwrap_or_else(|_| {
                        Err(rig::completion::PromptError::PromptCancelled {
                            chat_history: vec![],
                            reason: format!("{BUDGET_TERMINATED_PREFIX}: episode deadline reached"),
                        })
                    })
            } else {
                run.await
            }
        };
        let mut response = if let Some(binding) = &episode.managed {
            tokio::select! {
                result = bounded => result,
                revoked = self.runtime.wait_for_managed_dispatch_revocation(binding) => {
                    Err(rig::completion::PromptError::PromptCancelled {
                        chat_history: vec![],
                        reason: match revoked {
                            Ok(_) => "The run stopped because this agent's permission to run was revoked.".into(),
                            Err(error) => {
                                tracing::warn!(%error, "managed dispatch revocation check failed");
                                DISPATCH_UNCONFIRMED.into()
                            }
                        },
                    })
                }
            }
        } else {
            bounded.await
        };
        if response.is_ok()
            && let Some(binding) = &episode.managed
            && let Err(error) = connection.managed_dispatch(binding).await
        {
            tracing::warn!(%error, "managed dispatch confirmation failed");
            response = Err(rig::completion::PromptError::PromptCancelled {
                chat_history: vec![],
                reason: DISPATCH_UNCONFIRMED.into(),
            });
        }
        if self.runtime.episode_record(episode.episode_id).await?.state
            == AgentEpisodeState::Stopped
        {
            return self.finish_stopped(&episode, wake_ids).await;
        }

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
                if self.runtime.episode_record(episode.episode_id).await?.state
                    == AgentEpisodeState::Stopped
                {
                    return self.finish_stopped(&episode, wake_ids).await;
                }
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

    async fn finish_stopped(
        &self,
        episode: &EpisodeHandle,
        wakes: &[WakeId],
    ) -> Result<EpisodeReport> {
        let reason = "Stopped by an authorized operator.";
        self.runtime
            .complete_episode(
                episode.episode_id,
                EpisodeCompletion {
                    state: AgentEpisodeState::Stopped,
                    final_output: reason.into(),
                    summary: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    completion_calls: 0,
                    tool_calls: 0,
                    error: None,
                },
                wakes,
            )
            .await?;
        // DuckDB is an analytical projection with a deliberately smaller outcome
        // vocabulary. The canonical runtime episode records `stopped` explicitly.
        self.memory.finish_episode_projection(
            episode.episode_id.as_uuid(),
            EpisodeOutcome::Error,
            reason,
            0,
            0,
            0,
            0,
            Some(reason),
        )?;
        self.finish_rrd(reason.into());
        Ok(EpisodeReport {
            episode_id: episode.episode_id,
            seq: episode.sequence,
            output: reason.into(),
            detached_tasks: 0,
        })
    }

    fn finish_rrd(&self, text: String) {
        self.rrd.log_text("/agent/episodes", text);
        self.rrd.flush();
        if let Err(error) = self.rrd.rotate_if_needed() {
            tracing::warn!(%error, "rrd rotation failed");
        }
    }
}

/// Construct and execute the dispatch continuation only after durable admission.
/// Gateway refresh happens before this gate; all episode/model/tool work is inside it.
async fn with_started_episode<T, F>(
    runtime: &AgentRuntime,
    wake_note: &str,
    execute: impl FnOnce(EpisodeHandle) -> F,
) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    let episode = runtime.start_episode(wake_note).await?;
    execute(episode).await
}

#[cfg(test)]
#[path = "../../../testing/fixtures/store.rs"]
mod database;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };
    use veoveo_agent_runtime::{AgentInstanceId, AgentRuntimeError, AgentSpec};
    use veoveo_platform_store::{
        AgentState, ArtifactGrantSubjectKind, InvocationAuthorityRecord, InvocationMode,
        OpenObject, WorkContextMembershipLevel,
    };

    #[tokio::test]
    async fn failed_episode_persistence_never_enters_dispatch_continuation() {
        let database = database::TestDb::new().await;
        tokio::time::timeout(Duration::from_secs(30), async {
            let spec = || AgentSpec {
                tenant_key: "episode-admission".into(),
                agent_key: "dispatch-order".into(),
                display_name: "Dispatch ordering".into(),
                profile: "qualification".into(),
                authority: InvocationAuthorityRecord {
                    context_key: "episode-admission".into(),
                    membership: WorkContextMembershipLevel::Contributor,
                    policy_revision: "r1".into(),
                    owner_kind: ArtifactGrantSubjectKind::Principal,
                    owner_key: "agent:dispatch-order".into(),
                    initial_grants: Vec::new(),
                    classification: None,
                    data_labels: Vec::new(),
                    invocation_mode: InvocationMode::Automated,
                    initiator_key: None,
                    delegation_id: None,
                },
                manifest: OpenObject::default(),
                memory_database: "memory.duckdb".into(),
            };
            let stale = AgentRuntime::register(database.a.clone(), spec(), AgentInstanceId::new())
                .await
                .unwrap();
            let current =
                AgentRuntime::register(database.b.clone(), spec(), AgentInstanceId::new())
                    .await
                    .unwrap();
            let first_lease = stale
                .acquire_lease(Duration::from_millis(100))
                .await
                .unwrap()
                .expect("initial scheduler lease");
            tokio::time::sleep(Duration::from_millis(150)).await;
            let replacement_lease = current
                .acquire_lease(Duration::from_secs(30))
                .await
                .unwrap()
                .expect("replacement scheduler lease");
            assert!(replacement_lease.fence > first_lease.fence);

            let baseline = current.agent_record().await.unwrap();
            assert_eq!(baseline.fence, replacement_lease.fence);
            assert_eq!(
                baseline.lease_owner,
                Some(current.instance_id().to_string())
            );
            assert_ne!(baseline.lease_owner, Some(stale.instance_id().to_string()));
            assert_eq!(baseline.next_episode_sequence, 1);
            assert!(baseline.last_episode.is_none());
            let dispatches = AtomicUsize::new(0);
            let failed = with_started_episode(&stale, "stale scheduler", |_| {
                dispatches.fetch_add(1, Ordering::SeqCst);
                async { Ok(()) }
            })
            .await
            .unwrap_err();
            // check() returns the first statement error: the rollback marks earlier
            // statements NotExecuted before the later lease-fencing THROW. Assert
            // a structured query rejection, rather than the masked THROW text or
            // a missing local lease/connection failure. The same query succeeds
            // below with the replacement's stored owner and fence.
            let Some(AgentRuntimeError::Database(error)) =
                failed.downcast_ref::<AgentRuntimeError>()
            else {
                panic!("episode admission did not reach the database: {failed}");
            };
            assert!(
                error.is_query(),
                "unexpected rejection kind: {}",
                error.kind_str()
            );
            assert!(
                error.query_details().is_some(),
                "query rejection has no structured details"
            );
            assert_eq!(dispatches.load(Ordering::SeqCst), 0);
            assert_eq!(current.agent_record().await.unwrap(), baseline);
            assert_eq!(
                current
                    .episodes_started_since(DateTime::<Utc>::UNIX_EPOCH)
                    .await
                    .unwrap(),
                0
            );

            let admitted_runtime = &current;
            let admitted = with_started_episode(&current, "current scheduler", |episode| {
                dispatches.fetch_add(1, Ordering::SeqCst);
                async move {
                    // The successful control observes admission before execution.
                    let stored = admitted_runtime.episode_record(episode.episode_id).await?;
                    assert_eq!(stored.state, AgentEpisodeState::Running);
                    assert_eq!(stored.sequence, episode.sequence);
                    assert_eq!(stored.completion_calls, 0);
                    let agent = admitted_runtime.agent_record().await?;
                    assert_eq!(agent.state, AgentState::Running);
                    assert_eq!(agent.next_episode_sequence, episode.sequence + 1);
                    assert_eq!(agent.last_episode, Some(episode.episode_id.record_id()));
                    Ok(episode)
                }
            })
            .await
            .unwrap();
            assert_eq!(dispatches.load(Ordering::SeqCst), 1);
            assert_eq!(admitted.sequence, 1);
            assert_eq!(
                current
                    .episodes_started_since(DateTime::<Utc>::UNIX_EPOCH)
                    .await
                    .unwrap(),
                1
            );
            current
                .complete_episode(
                    admitted.episode_id,
                    EpisodeCompletion {
                        state: AgentEpisodeState::Completed,
                        final_output: "dispatch continuation qualified".into(),
                        summary: None,
                        input_tokens: 0,
                        output_tokens: 0,
                        completion_calls: 0,
                        tool_calls: 0,
                        error: None,
                    },
                    &[],
                )
                .await
                .unwrap();
            current.release_lease().await.unwrap();
        })
        .await
        .expect("episode admission qualification exceeded 30 seconds");
    }
}
