//! Retained completed Task reads across an externally operated physical backend fence.
use super::*;
#[path = "handoff/routing.rs"]
mod routing;
use chrono::{DateTime, Utc};
use routing::Routing;
use serde::de::IgnoredAny;
use sha2::{Digest, Sha256};
use std::{os::unix::fs::PermissionsExt, path::PathBuf};
use veoveo_testing_support::{
    final_tasks::public_caller::read_private_input,
    installed::restart::{HandoffFixture, HandoffObserver, HandoffPhase, HandoffReceipt},
};
use veoveo_types::Sha256Digest;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    installation: installed::InstalledSource,
    original_input: PathBuf,
    completion_receipt: PathBuf,
    completion_receipt_sha256: Sha256Digest,
    task_id: CanonicalTaskId,
    created_at: DateTime<Utc>,
    setup: HandoffFixture,
    routing: Routing,
}
// Admit the actual prior journal envelope. Historical transport diagnostics are
// intentionally ignored; they never supply the domain facts used by this case.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Prior {
    schema_version: String,
    mode: lifecycle::Mode,
    #[serde(rename = "lifecycle")]
    _lifecycle: IgnoredAny,
    authority: EffectiveTimeAuthority,
    request: ExpandScheduleRequest,
    expected: ExpandScheduleOutput,
    phase: Phase,
    dispatch_intent: bool,
    task_id: Option<CanonicalTaskId>,
    created: Option<TaskObservation>,
    #[serde(rename = "firstDeliveredStatus")]
    _first_delivered_status: Option<TaskStatus>,
    #[serde(rename = "workingDelivered")]
    _working_delivered: bool,
    delivered_completed: Option<TaskObservation>,
    current: Option<TaskObservation>,
    result: Option<ExpandScheduleOutput>,
    terminal_settled: bool,
    listener_closed: bool,
    caller_closed: bool,
    failure: Option<Failure>,
    #[serde(rename = "trace")]
    _trace: IgnoredAny,
    #[serde(rename = "cleanupTrace")]
    _cleanup_trace: IgnoredAny,
    #[serde(rename = "cleanupOwner")]
    _cleanup_owner: String,
    #[serde(rename = "remainingGates")]
    _remaining_gates: Vec<String>,
}
fn prior(input: &Input, original: &super::Input) -> Result<Prior> {
    let path = &input.completion_receipt;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        path.is_absolute()
            && metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.permissions().mode() & 0o077 == 0
            && metadata.len() <= 16 * 1024 * 1024,
        "prior completion receipt must be a bounded private regular file"
    );
    let bytes = fs::read(path)?;
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == input.completion_receipt_sha256,
        "prior completion receipt digest differs"
    );
    let mut last = None;
    for value in serde_json::Deserializer::from_slice(&bytes).into_iter::<Prior>() {
        last = Some(value?);
    }
    let last = last.context("prior completion receipt is empty")?;
    admit_prior(
        &input.installation,
        &input.completion_receipt,
        &input.task_id,
        input.created_at,
        original,
        &last,
    )?;
    Ok(last)
}
fn admit_prior(
    installation: &installed::InstalledSource,
    receipt: &std::path::Path,
    id: &CanonicalTaskId,
    created_at: DateTime<Utc>,
    original: &super::Input,
    prior: &Prior,
) -> Result<()> {
    ensure!(
        original.mode == lifecycle::Mode::Complete
            && prior.mode == lifecycle::Mode::Complete
            && prior.schema_version == "veoveo.ai/time-schedule-task-acceptance/v2"
            && prior.phase == Phase::Passed
            && prior.failure.is_none()
            && prior.dispatch_intent
            && prior.terminal_settled
            && prior.listener_closed
            && prior.caller_closed,
        "prior Task completion and cleanup are not qualified"
    );
    ensure!(
        original.installation.output == receipt
            && original.installation.endpoint == installation.endpoint
            && original.installation.installation_target == installation.installation_target
            && original.installation.deployment == installation.deployment,
        "prior completion belongs to another installation"
    );
    ensure!(
        prior.authority == original.authority
            && prior.request == original.request
            && prior.expected == original.expected
            && prior.result.as_ref() == Some(&original.expected),
        "prior completion domain inputs or independent output differ"
    );
    ensure!(
        prior.task_id.as_ref() == Some(id),
        "prior Task identity differs"
    );
    for observation in [&prior.created, &prior.current, &prior.delivered_completed] {
        let observation = observation
            .as_ref()
            .context("prior Task observation absent")?;
        ensure!(
            &observation.task_id == id && observation.created_at == created_at,
            "prior Task acknowledgement identity differs"
        );
    }
    ensure!(
        prior.current.as_ref().unwrap().status == TaskStatus::Completed
            && prior.delivered_completed.as_ref().unwrap().status == TaskStatus::Completed,
        "prior Task did not deliver completion"
    );
    Ok(())
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Step {
    Admitted,
    OriginalBaseline,
    CreateReplacementIntent,
    BothReady,
    HandoffIntent,
    BarrierRequested,
    ReplacementVerified,
    RestoreOriginalIntent,
    RestoreSelectorIntent,
    RetireReplacementIntent,
    Passed,
    Failed,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    schema_version: &'static str,
    step: Step,
    task_id: CanonicalTaskId,
    created_at: DateTime<Utc>,
    prior_receipt_sha256: Sha256Digest,
    setup: HandoffFixture,
    routing: Routing,
    physical: HandoffReceipt,
    original: Option<TaskObservation>,
    replacement: Option<TaskObservation>,
    result: Option<ExpandScheduleOutput>,
    caller_a_closed: bool,
    listener_a_closed: bool,
    caller_b_closed: bool,
    listener_b_closed: bool,
    failed: bool,
    restoration_failed: bool,
    watch_cleanup_failed: bool,
    trace: trace::Trace,
    remaining_gates: [&'static str; 2],
}
struct Journal {
    evidence: Evidence,
    output: fs::File,
}
impl Journal {
    fn persist(&mut self, step: Step) -> Result<()> {
        self.evidence.step = step;
        serde_json::to_writer(&mut self.output, &self.evidence)?;
        self.output.write_all(b"\n")?;
        self.output.sync_all()?;
        Ok(())
    }
}
struct Physical {
    observer: HandoffObserver,
    initialized: bool,
    changed: bool,
    restore_end: Option<std::time::Instant>,
    restore_failed: bool,
    watch_failed: bool,
}
impl Physical {
    async fn restore(&mut self, journal: &Arc<Mutex<Journal>>) -> Result<()> {
        let end = *self.restore_end.get_or_insert(owner::cleanup_deadline()?);
        let restore_end = restoration_cutoff(end);
        let result = if self.restore_failed || std::time::Instant::now() >= restore_end {
            Err(anyhow::anyhow!("handoff restoration allowance exhausted"))
        } else {
            tokio::time::timeout_at(restore_end.into(), async {
                if self.initialized {
                    if self.changed && self.observer.check(HandoffPhase::Restored).is_err() {
                        if !self
                            .observer
                            .original_route_for_cleanup(restore_end)
                            .await?
                        {
                            journal.lock().await.persist(Step::RestoreOriginalIntent)?;
                            self.observer
                                .wait(
                                    HandoffPhase::OriginalRestoredWithReplacementRouting,
                                    restore_end.into(),
                                )
                                .await?;
                            {
                                let mut journal = journal.lock().await;
                                journal.evidence.physical = self.observer.receipt().clone();
                                journal.persist(Step::RestoreSelectorIntent)?;
                            }
                            self.observer
                                .wait(HandoffPhase::BothReady, restore_end.into())
                                .await?;
                        }
                        let mut journal = journal.lock().await;
                        journal.evidence.physical = self.observer.receipt().clone();
                        journal.persist(Step::RetireReplacementIntent)?;
                    }
                    self.observer
                        .wait(HandoffPhase::Restored, restore_end.into())
                        .await?;
                }
                Ok::<_, anyhow::Error>(())
            })
            .await
            .map_err(|_| anyhow::anyhow!("handoff restoration deadline"))
            .and_then(|v| v)
        };
        let result = if std::time::Instant::now() >= restore_end {
            Err(anyhow::anyhow!("handoff restoration allowance expired"))
        } else {
            result
        };
        let outcomes = close_after_restoration(result, end, |original_end| {
            self.observer.close_until(original_end)
        })
        .await;
        self.restore_failed |= outcomes.restoration.is_err();
        self.watch_failed |= outcomes.watches.is_err();
        {
            let mut journal = journal.lock().await;
            journal.evidence.physical = self.observer.receipt().clone();
            journal.evidence.restoration_failed = self.restore_failed;
            journal.evidence.watch_cleanup_failed = self.watch_failed;
        }
        outcomes.restoration?;
        outcomes.watches?;
        ensure!(
            !self.watch_failed,
            "handoff watch cleanup previously failed"
        );
        Ok(())
    }
}

fn restoration_cutoff(end: std::time::Instant) -> std::time::Instant {
    end.checked_sub(Duration::from_secs(2)).unwrap_or(end)
}
struct CleanupOutcomes {
    restoration: Result<()>,
    watches: Result<()>,
}
async fn close_after_restoration<F: std::future::Future<Output = Result<()>>>(
    restoration: Result<()>,
    end: std::time::Instant,
    close: impl FnOnce(std::time::Instant) -> F,
) -> CleanupOutcomes {
    let expired = std::time::Instant::now() >= end;
    // Native AsyncChild closure owns its original cap; even an expired attempt
    // must preserve failed ownership rather than skip the actual close path.
    let result = close(end).await;
    let watches = if expired || std::time::Instant::now() >= end {
        Err(anyhow::anyhow!(
            "handoff watch original cleanup deadline expired"
        ))
    } else {
        result
    };
    CleanupOutcomes {
        restoration,
        watches,
    }
}

#[tokio::test]
#[ignore = "requires a retained completed Time Task, private prior receipt and externally operated A/B physical routing fences"]
async fn retained_completed_schedule_task_through_distinct_backends() -> Result<()> {
    // Every public error is static; SDK/OS/provider error text stays in private memory.
    let result = run().await;
    ensure!(
        result.is_ok(),
        "Time retained Task handoff failed; inspect private journal and lifecycle ownership lease"
    );
    Ok(())
}
async fn run() -> Result<()> {
    let input: Input = installed::input_from("VEOVEO_TIME_SCHEDULE_HANDOFF_INPUT")
        .map_err(|_| anyhow::anyhow!("handoff input admission failed"))?;
    let original: super::Input = read_private_input(&input.original_input)?;
    original.admit()?;
    let prior = prior(&input, &original)?;
    let target = input.installation.validate()?;
    ensure!(
        input.installation.deployment == input.setup.deployment_a && input.setup.pvc.is_some(),
        "Time handoff requires the selected authority PVC and original Deployment"
    );
    input.routing.admit(&target, &input.setup)?;
    let observer = HandoffObserver::new(&target, input.setup.clone())?;
    let journal = Arc::new(Mutex::new(Journal {
        output: open_receipt(&input.installation.output)?,
        evidence: Evidence {
            schema_version: "veoveo.ai/time-completed-task-handoff/v1",
            step: Step::Admitted,
            task_id: input.task_id.clone(),
            created_at: input.created_at,
            prior_receipt_sha256: input.completion_receipt_sha256.clone(),
            setup: input.setup.clone(),
            routing: input.routing.clone(),
            physical: HandoffReceipt::default(),
            original: None,
            replacement: None,
            result: None,
            caller_a_closed: true,
            listener_a_closed: true,
            caller_b_closed: true,
            listener_b_closed: true,
            failed: false,
            restoration_failed: false,
            watch_cleanup_failed: false,
            trace: trace::Trace::default(),
            remaining_gates: [
                "unfinished_cross_replica_recovery",
                "post_mutation_task_update",
            ],
        },
    }));
    journal.lock().await.persist(Step::Admitted)?;
    let physical = Arc::new(Mutex::new(Physical {
        observer,
        initialized: false,
        changed: false,
        restore_end: None,
        restore_failed: false,
        watch_failed: false,
    }));
    let a = Arc::new(Mutex::new(Handles::new()));
    let b = Arc::new(Mutex::new(Handles::new()));
    // Keep the composed SDK/watch operation off the libtest thread's stack.
    // Pinning changes storage only; the owner still drops work before cleanup.
    let outcome = owner::run(Box::pin(async {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
        let restoration = physical.clone();
        let restoration_journal = journal.clone();
        owner::register_cleanup(
            owner::CleanupKind::Remote,
            "Time physical handoff watches and restoration",
            &uuid::Uuid::now_v7().to_string(),
            move || async move { restoration.lock().await.restore(&restoration_journal).await },
        )?;
        for handles in [&a, &b] {
            let handles = handles.clone();
            owner::register_cleanup(
                owner::CleanupKind::Remote,
                "Time retained Task SDK handles",
                &uuid::Uuid::now_v7().to_string(),
                move || async move { close_handles(&mut *handles.lock().await).await },
            )?;
        }
        tokio::time::timeout_at(
            deadline,
            Box::pin(async {
                let mut topology = physical.lock().await;
                input.routing.live(&target, &input.setup).await?;
                topology.observer.initialize().await?;
                topology.initialized = true;
                let baseline = topology
                    .observer
                    .protect(
                        HandoffPhase::OriginalOnly,
                        observe(&a, &input, &original, &prior, &journal),
                    )
                    .await?;
                {
                    let mut journal = journal.lock().await;
                    journal.evidence.original = Some(baseline);
                    journal.evidence.physical = topology.observer.receipt().clone();
                    journal.persist(Step::OriginalBaseline)?;
                    // Ops may create B only after this append is durable.
                    journal.persist(Step::CreateReplacementIntent)?;
                }
                topology.changed = true;
                topology
                    .observer
                    .wait(HandoffPhase::BothReady, deadline)
                    .await?;
                ensure!(
                    topology
                        .observer
                        .receipt()
                        .pod_b_created_at
                        .is_some_and(|created| created > input.created_at),
                    "replacement predates retained Task acknowledgement"
                );
                journal.lock().await.persist(Step::BothReady)?;
                journal.lock().await.persist(Step::HandoffIntent)?;
                topology
                    .observer
                    .wait(HandoffPhase::ReplacementOnly, deadline)
                    .await?;
                let replacement = topology
                    .observer
                    .protect(
                        HandoffPhase::ReplacementOnly,
                        observe(&b, &input, &original, &prior, &journal),
                    )
                    .await?;
                topology.observer.request_barrier(uuid::Uuid::now_v7())?;
                {
                    let mut journal = journal.lock().await;
                    journal.evidence.physical = topology.observer.receipt().clone();
                    journal.persist(Step::BarrierRequested)?;
                }
                topology.observer.finish_barrier(deadline).await?;
                input.routing.live(&target, &input.setup).await?;
                let mut journal = journal.lock().await;
                journal.evidence.replacement = Some(replacement);
                journal.evidence.result = Some(original.expected.clone());
                journal.evidence.physical = topology.observer.receipt().clone();
                journal.persist(Step::ReplacementVerified)?;
                Ok::<_, anyhow::Error>(())
            }),
        )
        .await
        .map_err(|_| anyhow::anyhow!("handoff original operation deadline"))
        .and_then(|v| v)
    }))
    .await;
    let a = a.lock().await;
    let b = b.lock().await;
    let topology = physical.lock().await;
    let mut journal = journal.lock().await;
    journal.evidence.caller_a_closed = a.caller_closed;
    journal.evidence.listener_a_closed = a.listener_closed;
    journal.evidence.caller_b_closed = b.caller_closed;
    journal.evidence.listener_b_closed = b.listener_closed;
    journal.evidence.physical = topology.observer.receipt().clone();
    if let Err(error) = &outcome {
        journal.evidence.trace.interrupt(error);
    }
    journal.evidence.failed = outcome.is_err()
        || topology.restore_failed
        || topology.watch_failed
        || !journal.evidence.physical.restored
        || !journal.evidence.physical.watches_closed
        || !a.caller_closed
        || !a.listener_closed
        || !b.caller_closed
        || !b.listener_closed;
    let step = if journal.evidence.failed {
        Step::Failed
    } else {
        Step::Passed
    };
    journal.persist(step)?;
    ensure!(
        !journal.evidence.failed,
        "handoff qualification or cleanup unresolved"
    );
    Ok(())
}
async fn observe(
    handles: &Arc<Mutex<Handles>>,
    input: &Input,
    original: &super::Input,
    prior: &Prior,
    journal: &Arc<Mutex<Journal>>,
) -> Result<TaskObservation> {
    let mut handles = handles.lock().await;
    // Hold the retained slot before acquisition: cancellation has no unowned handle gap.
    journal
        .lock()
        .await
        .evidence
        .trace
        .begin(trace::Request::TaskConnection);
    handles.caller = Some(input.installation.task_caller().await?);
    handles.caller_closed = false;
    handles.task_id = Some(input.task_id.clone());
    let filter = SubscriptionFilter::builder()
        .task_id(input.task_id.to_string())
        .build();
    journal
        .lock()
        .await
        .evidence
        .trace
        .begin(trace::Request::TaskListen(input.task_id.clone()));
    handles.listener = Some(
        handles
            .caller
            .as_ref()
            .unwrap()
            .listen(filter.clone())
            .await?,
    );
    handles.listener_closed = false;
    let listener = handles.listener.as_mut().unwrap();
    exact_ack(listener.acknowledged(), &filter)?;
    let notification = listener
        .next()
        .await?
        .context("retained Task listener ended")?;
    ensure!(
        notification.get_meta().subscription_id() == Some(listener.id().clone()),
        "retained Task delivery subscription differs"
    );
    let ServerNotification::TaskStatusNotification(update) = notification else {
        anyhow::bail!("retained Task listener delivered another notification kind");
    };
    let caller = handles.caller.as_ref().unwrap();
    let current =
        await_task_terminal_with_timeout(caller, input.task_id.as_str(), Duration::from_secs(15))
            .await?;
    journal
        .lock()
        .await
        .evidence
        .trace
        .begin(trace::Request::TaskResult(input.task_id.clone()));
    let result = decode_output(task_payload(caller, input.task_id.as_str()).await?)?;
    let created = prior.created.as_ref().unwrap();
    let acknowledgement = Task::new(
        input.task_id.to_string(),
        created.status,
        created.created_at.to_rfc3339(),
        created.last_updated_at.to_rfc3339(),
    );
    assert_agreement(
        &input.task_id,
        &acknowledgement,
        &update.params.task,
        &current,
        &result,
        &original.expected,
    )?;
    let authority: EffectiveTimeAuthority = journal
        .lock()
        .await
        .evidence
        .trace
        .read(caller.peer(), TimeResource::AuthoritiesCurrent)
        .await?;
    ensure!(
        authority == original.authority,
        "Time authority differs across readers"
    );
    TaskObservation::admit(&current.task)
}

fn exact_ack(acknowledged: &SubscriptionFilter, requested: &SubscriptionFilter) -> Result<()> {
    ensure!(
        acknowledged == requested,
        "retained Task acknowledgement narrowed or changed filter"
    );
    Ok(())
}

#[cfg(test)]
#[path = "handoff/tests.rs"]
mod tests;
