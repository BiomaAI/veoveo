//! The original dispatch owns foreground execution. Successors contain its run;
//! they never reconstruct an execution attempt from queued command bytes.
mod guard;
mod lease;
mod outputs;
mod scheduler;
mod settlement;

use crate::WorkerStep;
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_computers::{ComputerError, ComputersStore, commands::*, secrets::ComputerKeyRing};
use veoveo_computers_runtime::{Binding, Observation, OpenShellRuntime, Phase};
use veoveo_task_runtime::{ClaimedTask, TaskError, TaskRuntime};

const LEASE_DURATION: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum CommandWorkerError {
    #[error(transparent)]
    Domain(#[from] ComputerError),
    #[error(transparent)]
    Task(#[from] TaskError),
    #[error("Computer execution configuration is unavailable")]
    Configuration,
    #[error("Computer command Task lease is unavailable")]
    LeaseLost,
    #[error("Computer output publication is unavailable")]
    OutputUnavailable,
}
type Result<T> = std::result::Result<T, CommandWorkerError>;

pub struct CommandWorker {
    store: ComputersStore,
    tasks: TaskRuntime,
    runtime: OpenShellRuntime,
    keys: Arc<ComputerKeyRing>,
    artifacts: HttpArtifactPlane,
    templates: BTreeSet<String>,
}
impl CommandWorker {
    pub fn new(
        store: ComputersStore,
        tasks: TaskRuntime,
        runtime: OpenShellRuntime,
        keys: Arc<ComputerKeyRing>,
        artifacts: HttpArtifactPlane,
        templates: BTreeSet<String>,
    ) -> Result<Self> {
        if store.provider_instance_id() != runtime.provider_instance_id()
            || tasks.server() != "computers"
            || templates.is_empty()
            || templates
                .iter()
                .any(|t| t.len() != 64 || !t.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(CommandWorkerError::Configuration);
        }
        Ok(Self {
            store,
            tasks,
            runtime,
            keys,
            artifacts,
            templates,
        })
    }
    pub async fn step(&self, operation: CommandOperation) -> Result<WorkerStep> {
        if operation.task_projected() {
            self.acknowledge(&operation).await?;
            return Ok(WorkerStep::Settled);
        }
        let id = operation.task_id().to_string();
        let task = match self.tasks.get(&id).await? {
            Some(task) => task,
            None => {
                self.store.ensure_command_task(&operation).await?;
                self.tasks
                    .get(&id)
                    .await?
                    .ok_or(CommandWorkerError::LeaseLost)?
            }
        };
        if task.is_terminal() {
            self.acknowledge(&operation).await?;
            return Ok(WorkerStep::Settled);
        }
        let mut claim = match self.tasks.claim_observation(&id, LEASE_DURATION).await {
            Ok(claim) => claim,
            Err(TaskError::LeaseHeld(_) | TaskError::Conflict(_)) => return Ok(WorkerStep::Busy),
            Err(error) => return Err(error.into()),
        };
        let operation = self.store.command_for_claim(&claim).await?;
        let step = match operation.stage() {
            CommandStage::Completed | CommandStage::Cancelled | CommandStage::Failed => {
                self.project(&operation).await?;
                WorkerStep::Settled
            }
            CommandStage::RecoveryRequired => {
                self.waiting(
                    &id,
                    "Recovery Required; the original command remains protected",
                )
                .await?;
                WorkerStep::RecoveryRequired
            }
            CommandStage::Queued => self.dispatch(&mut claim, &operation).await?,
            CommandStage::Dispatched | CommandStage::Containing => {
                self.contain(&mut claim, CommandInterruption::ExecutionUnknown)
                    .await?
            }
        };
        if matches!(step, WorkerStep::Waiting | WorkerStep::RecoveryRequired) {
            self.tasks.release_observation(&claim).await?;
        }
        Ok(step)
    }
    async fn dispatch(
        &self,
        claim: &mut ClaimedTask,
        operation: &CommandOperation,
    ) -> Result<WorkerStep> {
        let id = operation.task_id().to_string();
        let refusal = if self.tasks.is_cancel_requested(&id).await? {
            Some(CommandRefusal::CancelledBeforeDispatch)
        } else if chrono::Utc::now() - operation.created_at() > chrono::TimeDelta::seconds(301) {
            Some(CommandRefusal::PreparationExpired)
        } else {
            None
        };
        if let Some(reason) = refusal {
            return self.refuse(claim, reason).await;
        }
        if !self
            .templates
            .contains(&operation.binding().template_fingerprint)
        {
            self.waiting(&id, "The Computer execution profile is unavailable")
                .await?;
            return Ok(WorkerStep::Waiting);
        }
        if operation.output_capability_request(&self.keys)?.is_some() {
            self.waiting(&id, "Preparing authorized command output")
                .await?;
            return Ok(WorkerStep::Waiting);
        }
        let ticket = match self.store.begin_command_dispatch(claim, &self.keys).await {
            Ok(ticket) => ticket,
            Err(ComputerError::Forbidden) => {
                return self.refuse(claim, CommandRefusal::AuthorityDenied).await;
            }
            Err(ComputerError::InvalidState | ComputerError::StateConflict) => {
                return self.refuse(claim, CommandRefusal::RunChanged).await;
            }
            Err(_) => {
                self.waiting(&id, "Waiting for current execution authority")
                    .await?;
                return Ok(WorkerStep::Waiting);
            }
        };
        let binding = Binding::from_instance(
            ticket.binding().computer_id,
            ticket.binding().instance_id(),
            ticket.binding().template_fingerprint.clone(),
        )
        .map_err(|_| CommandWorkerError::Configuration)?;
        let expected = Observation {
            sandbox_id: ticket.binding().resource_id.clone(),
            main_process_instance_id: ticket.binding().process_id.clone(),
            phase: Phase::Ready,
            exit_code: None,
        };
        let output = Arc::new(Mutex::new(outputs::CapturedOutput::new(
            ticket.limits().maximum_output_bytes,
        )));
        let capture = output.clone();
        let constrain = output.clone();
        self.waiting(&id, "Running command").await?;
        let started = Instant::now();
        let initial = CommandRunAuthority {
            valid_until: ticket.authority_deadline(),
            execution_deadline: ticket.execution_deadline(),
            maximum_output_bytes: ticket.limits().maximum_output_bytes,
            deadline_reason: CommandInterruption::Deadline,
        };
        let received = self.runtime.execute_request(
            &binding,
            &expected,
            ticket.payload().request(),
            ticket.limits().maximum_seconds,
            ticket.limits().maximum_output_bytes as usize,
            move |chunk| {
                let accepted = capture
                    .lock()
                    .map_err(|_| veoveo_computers_runtime::RuntimeFailure::ExecutionUnknown)
                    .and_then(|mut output| output.append(chunk));
                async move { accepted }
            },
        );
        let observed = guard::run(
            initial,
            received,
            Duration::from_secs(1),
            || self.refresh(claim, ticket.operation()),
            |maximum| {
                constrain
                    .lock()
                    .is_ok_and(|mut output| output.constrain(maximum))
            },
        )
        .await;
        // Renewal may have committed while the provider completed. Recover its
        // current same-worker receipt before any journal write; never redispatch.
        self.recover_lease(claim).await?;
        match observed {
            Ok(Ok(exit)) => {
                let exit = match ticket.observe_exit(
                    exit.exit_code,
                    exit.stdout_bytes,
                    exit.stderr_bytes,
                ) {
                    Ok(exit) => exit,
                    Err(_) => {
                        tracing::warn!(execution_id = %operation.execution_id(), phase = "exit_receipt", "Computer command outcome is unknown");
                        return self
                            .contain(claim, CommandInterruption::ExecutionUnknown)
                            .await;
                    }
                };
                self.waiting(&id, "Publishing command output").await?;
                let buffers = output
                    .lock()
                    .map_err(|_| CommandWorkerError::OutputUnavailable)?
                    .take();
                let deadline = exit.publication_deadline();
                match self
                    .with_lease(claim, deadline, self.publish(&exit, buffers))
                    .await
                {
                    Ok(Ok((stdout, stderr))) => {
                        let completed = self
                            .store
                            .complete_command_output(claim, exit, stdout, stderr)
                            .await?;
                        self.project(&completed).await?;
                        tracing::debug!(execution_id = %operation.execution_id(), elapsed_ms = started.elapsed().as_millis(), "Computer command completed");
                        Ok(WorkerStep::Settled)
                    }
                    _ => {
                        tracing::warn!(execution_id = %operation.execution_id(), phase = "output_publication", "Computer command outcome is unknown");
                        self.contain(claim, CommandInterruption::ExecutionUnknown)
                            .await
                    }
                }
            }
            Err(guard::Interrupted::Domain(reason)) => self.contain(claim, reason).await,
            Err(guard::Interrupted::LeaseLost) => {
                self.contain(claim, CommandInterruption::AuthorityLost)
                    .await
            }
            Ok(Err(_)) => {
                tracing::warn!(execution_id = %operation.execution_id(), phase = "native_execution", "Computer command outcome is unknown");
                self.contain(claim, CommandInterruption::ExecutionUnknown)
                    .await
            }
        }
    }
    async fn refuse(&self, claim: &ClaimedTask, reason: CommandRefusal) -> Result<WorkerStep> {
        let operation = self.store.abort_queued_command(claim, reason).await?;
        self.project(&operation).await?;
        Ok(WorkerStep::Settled)
    }
}
