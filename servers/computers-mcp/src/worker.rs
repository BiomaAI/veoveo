//! A dispatch ticket is consumed once; subsequent attempts only observe its intent.
mod lease;
mod projection;
mod scheduler;
use crate::Preflight;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use veoveo_computers::{
    ComputersStore, ObservationAdmission, Operation, OperationStage, ReachedPhase, ReachedState,
    UndispatchedOutcome, api::Action,
};
use veoveo_computers_runtime::{
    Binding, DevelopmentTemplate, LifecycleCheckpoint, LifecycleObservation, Observation,
    OpenShellRuntime, Phase,
};
use veoveo_task_runtime::{ClaimedTask, TaskError, TaskRuntime, TaskStatus, TaskTransition};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerStep {
    Settled,
    Waiting,
    RecoveryRequired,
    Busy,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error(transparent)]
    Domain(#[from] veoveo_computers::ComputerError),
    #[error("shared Task authority is unavailable")]
    Task(#[from] TaskError),
    #[error("recorded provider template or identity is unavailable")]
    Configuration,
    #[error("worker authority check timed out")]
    LeaseLost,
}
type Result<T> = std::result::Result<T, WorkerError>;
const LEASE_DURATION: Duration = Duration::from_secs(60);

pub struct LifecycleWorker<G> {
    store: ComputersStore,
    tasks: TaskRuntime,
    runtime: OpenShellRuntime,
    templates: BTreeMap<String, DevelopmentTemplate>,
    preflight: G,
}
impl<G: Preflight> LifecycleWorker<G> {
    pub fn new(
        store: ComputersStore,
        tasks: TaskRuntime,
        runtime: OpenShellRuntime,
        templates: Vec<DevelopmentTemplate>,
        preflight: G,
    ) -> Result<Self> {
        if tasks.server() != "computers"
            || templates.is_empty()
            || store.provider_instance_id() != runtime.provider_instance_id()
            || templates.iter().any(|t| t.persistent_home().is_none())
        {
            return Err(WorkerError::Configuration);
        }
        let mut selected = BTreeMap::new();
        for template in templates {
            if selected.insert(template.fingerprint(), template).is_some() {
                return Err(WorkerError::Configuration);
            }
        }
        Ok(Self {
            store,
            tasks,
            runtime,
            templates: selected,
            preflight,
        })
    }

    /// Repairs a lost Task link and terminal projection as well as active work.
    /// Repeating this method never repeats an already journaled dispatch.
    pub async fn step(&self, operation: Operation) -> Result<WorkerStep> {
        if operation.task_projected_at.is_some() {
            self.acknowledge(&operation).await?;
            return Ok(WorkerStep::Settled);
        }
        let id = operation.task_id().to_string();
        let task = match self.tasks.get(&id).await? {
            Some(task) => task,
            None => {
                self.store
                    .ensure_operation_task(&operation.actor, operation.operation_id)
                    .await?;
                self.tasks
                    .get(&id)
                    .await?
                    .ok_or_else(|| TaskError::NotFound(id.clone()))?
            }
        };
        if task.is_terminal() {
            self.acknowledge(&operation).await?;
            return Ok(WorkerStep::Settled);
        }
        let mut claimed = match self.tasks.claim_observation(&id, LEASE_DURATION).await {
            Ok(claimed) => claimed,
            Err(TaskError::LeaseHeld(_) | TaskError::Conflict(_)) => return Ok(WorkerStep::Busy),
            Err(error) => return Err(error.into()),
        };
        let operation = self.store.operation_for_claim(&claimed).await?;
        let step = match operation.stage {
            OperationStage::Succeeded | OperationStage::Failed | OperationStage::Cancelled => {
                self.project(&operation).await?;
                Ok(WorkerStep::Settled)
            }
            OperationStage::RecoveryRequired => {
                self.waiting(&id, "Recovery Required; the operation remains protected")
                    .await?;
                Ok(WorkerStep::RecoveryRequired)
            }
            OperationStage::Queued => self.dispatch(&mut claimed, &operation).await,
            OperationStage::Dispatched => self.observe(&mut claimed, &operation).await,
        }?;
        if matches!(step, WorkerStep::Waiting | WorkerStep::RecoveryRequired) {
            self.tasks.release_observation(&claimed).await?;
        }
        Ok(step)
    }

    async fn dispatch(
        &self,
        claimed: &mut ClaimedTask,
        operation: &Operation,
    ) -> Result<WorkerStep> {
        let id = operation.task_id().to_string();
        if self.tasks.is_cancel_requested(&id).await? {
            let aborted = self
                .store
                .abort_undispatched(claimed, UndispatchedOutcome::CancelledBeforeDispatch)
                .await?;
            self.project(&aborted).await?;
            return Ok(WorkerStep::Settled);
        }
        let template = self
            .templates
            .get(&operation.template_fingerprint)
            .ok_or(WorkerError::Configuration)?;
        let (binding, checkpoint, before) = native_intent(operation)?;
        if operation.action != Action::Stop {
            self.waiting(&id, "Preparing retained storage").await?;
            let prepared = self
                .with_lease(
                    claimed,
                    tokio::time::timeout(
                        Duration::from_secs(180),
                        self.preflight.prepare_home(operation, &binding, template),
                    ),
                )
                .await?;
            if !matches!(prepared, Ok(Ok(()))) {
                self.waiting(&id, "Retained storage is unavailable; files are retained")
                    .await?;
                return Ok(WorkerStep::Waiting);
            }
        }
        // The domain reads current policy and journals its decision under the
        // exact Task lease. No external gate can mint dispatch authority.
        let ticket =
            match tokio::time::timeout(Duration::from_secs(10), self.store.begin_dispatch(claimed))
                .await
            {
                Ok(Ok(ticket)) => ticket,
                Ok(Err(veoveo_computers::ComputerError::Forbidden)) => {
                    let aborted = self
                        .store
                        .abort_undispatched(claimed, UndispatchedOutcome::AuthorityDenied)
                        .await?;
                    self.project(&aborted).await?;
                    return Ok(WorkerStep::Settled);
                }
                _ => {
                    self.waiting(&id, "Current action authority is unavailable")
                        .await?;
                    return Ok(WorkerStep::Waiting);
                }
            };
        let authority_deadline = tokio::time::Instant::from_std(ticket.authority_deadline());
        let dispatched = async {
            if ticket.authority_remaining().is_zero() {
                return Err(veoveo_computers_runtime::RuntimeFailure::LeaseExpired);
            }
            match operation.action {
                Action::Create => self.runtime.create(&binding, template).await,
                Action::Start => {
                    self.runtime
                        .start(
                            &binding,
                            before
                                .as_ref()
                                .ok_or(veoveo_computers_runtime::RuntimeFailure::BindingMismatch)?,
                        )
                        .await
                }
                Action::Stop => {
                    self.runtime
                        .stop(
                            &binding,
                            before
                                .as_ref()
                                .ok_or(veoveo_computers_runtime::RuntimeFailure::BindingMismatch)?,
                        )
                        .await
                }
            }
        };
        let outcome = self
            .with_lease(
                claimed,
                tokio::time::timeout_at(authority_deadline, dispatched),
            )
            .await?;
        if let Ok(Ok(current)) = outcome {
            let reached = self
                .with_lease(
                    claimed,
                    self.runtime
                        .wait_for_lifecycle(&checkpoint, &current, ticket.remaining()),
                )
                .await?;
            if let Ok(seen) = reached {
                let completed = self
                    .store
                    .complete_dispatch(claimed, ticket, reached_state(operation, seen)?)
                    .await?;
                self.project(&completed).await?;
                return Ok(WorkerStep::Settled);
            }
        }
        self.observe(claimed, operation).await
    }

    async fn observe(
        &self,
        claimed: &mut ClaimedTask,
        operation: &Operation,
    ) -> Result<WorkerStep> {
        let (_, checkpoint, _) = native_intent(operation)?;
        match self.store.admit_observation(claimed).await? {
            ObservationAdmission::Read(ticket) => {
                let seen = self
                    .with_lease(
                        claimed,
                        self.runtime
                            .reconcile_lifecycle(&checkpoint, ticket.remaining()),
                    )
                    .await?;
                if let Ok(LifecycleObservation::Reached(seen)) = seen {
                    let completed = self
                        .store
                        .complete_observation(claimed, ticket, reached_state(operation, seen)?)
                        .await?;
                    self.project(&completed).await?;
                    return Ok(WorkerStep::Settled);
                }
            }
            ObservationAdmission::Wait { .. } => {}
            ObservationAdmission::RecoveryRequired => {
                self.waiting(
                    &operation.task_id().to_string(),
                    "Recovery Required; the operation remains protected",
                )
                .await?;
                return Ok(WorkerStep::RecoveryRequired);
            }
        }
        self.waiting(
            &operation.task_id().to_string(),
            "Observing the original operation; files are retained",
        )
        .await?;
        Ok(WorkerStep::Waiting)
    }

    async fn waiting(&self, id: &str, message: &str) -> Result<()> {
        let task = self
            .tasks
            .get(id)
            .await?
            .ok_or_else(|| TaskError::NotFound(id.into()))?;
        if task.status == TaskStatus::CancelRequested
            || task.is_terminal()
            || (task.status == TaskStatus::Waiting
                && task.status_message.as_deref() == Some(message))
        {
            return Ok(());
        }
        self.tasks
            .transition_if_current(
                &task,
                TaskTransition::Waiting {
                    message: message.into(),
                    progress: task.progress,
                },
            )
            .await?;
        Ok(())
    }
}

fn native_intent(
    operation: &Operation,
) -> Result<(Binding, LifecycleCheckpoint, Option<Observation>)> {
    let binding = Binding::from_instance(
        operation.computer_id,
        operation.instance_id(),
        operation.template_fingerprint.clone(),
    )
    .map_err(|_| WorkerError::Configuration)?;
    let before = if operation.action == Action::Create {
        None
    } else {
        Some(Observation {
            sandbox_id: operation
                .previous_resource_id
                .clone()
                .ok_or(WorkerError::Configuration)?,
            main_process_instance_id: operation
                .previous_process_id
                .clone()
                .ok_or(WorkerError::Configuration)?,
            phase: if operation.action == Action::Start {
                Phase::Stopped
            } else {
                Phase::Ready
            },
            exit_code: None,
        })
    };
    let checkpoint = match operation.action {
        Action::Create => LifecycleCheckpoint::create(
            operation.provider_instance_id,
            operation.operation_id,
            binding.clone(),
        ),
        Action::Start => LifecycleCheckpoint::start(
            operation.provider_instance_id,
            operation.operation_id,
            binding.clone(),
            before.as_ref().ok_or(WorkerError::Configuration)?,
        ),
        Action::Stop => LifecycleCheckpoint::stop(
            operation.provider_instance_id,
            operation.operation_id,
            binding.clone(),
            before.as_ref().ok_or(WorkerError::Configuration)?,
        ),
    }
    .map_err(|_| WorkerError::Configuration)?;
    Ok((binding, checkpoint, before))
}
fn reached_state(operation: &Operation, seen: Observation) -> Result<ReachedState> {
    Ok(ReachedState {
        provider_instance_id: operation.provider_instance_id,
        computer_id: operation.computer_id,
        replacement_instance_id: operation.replacement_instance_id,
        template_fingerprint: operation.template_fingerprint.clone(),
        resource_id: seen.sandbox_id,
        process_id: seen.main_process_instance_id,
        phase: match seen.phase {
            Phase::Ready => ReachedPhase::Ready,
            Phase::Stopped => ReachedPhase::Stopped,
            _ => return Err(WorkerError::Configuration),
        },
    })
}
