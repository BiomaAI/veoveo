use super::*;
use veoveo_computers::{ReachedPhase, ReachedState};
use veoveo_computers_runtime::{LifecycleCheckpoint, LifecycleObservation};
use veoveo_task_runtime::{TaskFailure, TaskRetentionPin, TaskStatus, TaskTransition};

impl CommandWorker {
    pub(super) async fn contain(
        &self,
        claim: &mut ClaimedTask,
        reason: CommandInterruption,
    ) -> Result<WorkerStep> {
        let operation = self.store.begin_command_containment(claim, reason).await?;
        let b = operation.binding();
        let binding = Binding::new(b.computer_id, b.template_fingerprint.clone())
            .map_err(|_| CommandWorkerError::Configuration)?;
        let before = Observation {
            sandbox_id: b.resource_id.clone(),
            main_process_instance_id: b.process_id.clone(),
            phase: Phase::Ready,
            exit_code: None,
        };
        let checkpoint = LifecycleCheckpoint::stop(
            b.provider_instance_id,
            operation
                .containment_id()
                .ok_or(CommandWorkerError::Configuration)?,
            binding.clone(),
            &before,
        )
        .map_err(|_| CommandWorkerError::Configuration)?;
        if let Some(ticket) = self.store.admit_command_stop(claim).await? {
            let deadline = Instant::now() + ticket.remaining();
            if let Ok(Ok(current)) = self
                .with_lease(claim, deadline, self.runtime.stop(&binding, &before))
                .await
                && let Ok(Ok(stopped)) = self
                    .with_lease(
                        claim,
                        deadline,
                        self.runtime
                            .wait_for_lifecycle(&checkpoint, &current, ticket.remaining()),
                    )
                    .await
            {
                let completed = self
                    .store
                    .complete_command_stop(claim, ticket, reached(&operation, stopped))
                    .await?;
                self.project(&completed).await?;
                return Ok(WorkerStep::Settled);
            }
            self.recover_lease(claim).await?;
        }
        match self.store.admit_command_containment_read(claim).await? {
            ContainmentReadAdmission::Read(ticket) => {
                if let Ok(Ok(LifecycleObservation::Reached(stopped))) = self
                    .with_lease(
                        claim,
                        Instant::now() + ticket.remaining(),
                        self.runtime
                            .reconcile_lifecycle(&checkpoint, ticket.remaining()),
                    )
                    .await
                {
                    let completed = self
                        .store
                        .complete_command_containment_read(
                            claim,
                            ticket,
                            reached(&operation, stopped),
                        )
                        .await?;
                    self.project(&completed).await?;
                    return Ok(WorkerStep::Settled);
                }
                self.recover_lease(claim).await?;
            }
            ContainmentReadAdmission::Wait { .. } => {}
            ContainmentReadAdmission::RecoveryRequired => {
                self.waiting(
                    &operation.task_id().to_string(),
                    "Recovery Required; original process termination is unconfirmed",
                )
                .await?;
                return Ok(WorkerStep::RecoveryRequired);
            }
        }
        self.waiting(
            &operation.task_id().to_string(),
            "Confirming the original Computer run has stopped",
        )
        .await?;
        Ok(WorkerStep::Waiting)
    }
    pub(super) async fn project(&self, operation: &CommandOperation) -> Result<()> {
        let transition = match operation
            .outcome()
            .ok_or(CommandWorkerError::Configuration)?
        {
            CommandOutcome::Completed(result) => {
                let mut response = rmcp::model::CallToolResult::structured(
                    serde_json::to_value(result).map_err(|_| CommandWorkerError::Configuration)?,
                );
                response.is_error = Some(result.exit_code != 0);
                response.content = vec![rmcp::model::ContentBlock::text(format!(
                    "Command exited with code {}",
                    result.exit_code
                ))];
                response
                    .content
                    .push(rmcp::model::ContentBlock::resource_link(
                        rmcp::model::Resource::new(
                            String::from(result.result_uri),
                            "Command result",
                        ),
                    ));
                TaskTransition::Succeeded {
                    message: "Command completed".into(),
                    result: serde_json::to_value(response)
                        .map_err(|_| CommandWorkerError::Configuration)?,
                }
            }
            CommandOutcome::Undispatched(CommandRefusal::CancelledBeforeDispatch)
            | CommandOutcome::Terminated(CommandInterruption::Cancelled) => {
                TaskTransition::Cancelled
            }
            CommandOutcome::Undispatched(reason) => {
                let (code, message) = match reason {
                    CommandRefusal::AuthorityDenied => (
                        "authority_denied",
                        "Current authority does not permit this command",
                    ),
                    CommandRefusal::RunChanged => (
                        "run_changed",
                        "The Computer run changed before the command started",
                    ),
                    CommandRefusal::PreparationExpired => (
                        "preparation_expired",
                        "Command preparation expired; retry with a new request ID",
                    ),
                    CommandRefusal::CancelledBeforeDispatch => unreachable!(),
                };
                TaskTransition::Failed(TaskFailure::new(code, message))
            }
            CommandOutcome::Terminated(reason) => {
                let (code, message) = match reason {
                    CommandInterruption::Deadline => (
                        "deadline",
                        "The command deadline expired; the original Computer run is stopped",
                    ),
                    CommandInterruption::AuthorityLost => (
                        "authority_lost",
                        "Command authority ended; the original Computer run is stopped",
                    ),
                    CommandInterruption::ExecutionUnknown => (
                        "execution_unknown",
                        "The command result is unavailable; the original Computer run is stopped",
                    ),
                    CommandInterruption::Cancelled => unreachable!(),
                };
                TaskTransition::Failed(TaskFailure::new(code, message))
            }
        };
        self.tasks
            .transition(&operation.task_id().to_string(), transition)
            .await?;
        self.acknowledge(operation).await
    }
    pub(super) async fn acknowledge(&self, operation: &CommandOperation) -> Result<()> {
        self.store.acknowledge_command_task(operation).await?;
        self.tasks
            .acknowledge_retention_pin(
                &operation.task_id().to_string(),
                &TaskRetentionPin::new(format!("computer-execution/{}", operation.execution_id()))
                    .map_err(|_| CommandWorkerError::Configuration)?,
            )
            .await?;
        Ok(())
    }
    pub(super) async fn waiting(&self, id: &str, message: &str) -> Result<()> {
        let task = self
            .tasks
            .get(id)
            .await?
            .ok_or(CommandWorkerError::LeaseLost)?;
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
fn reached(operation: &CommandOperation, observation: Observation) -> ReachedState {
    ReachedState {
        provider_instance_id: operation.binding().provider_instance_id,
        computer_id: operation.computer_id(),
        template_fingerprint: operation.binding().template_fingerprint.clone(),
        resource_id: observation.sandbox_id,
        process_id: observation.main_process_instance_id,
        phase: if observation.phase == Phase::Stopped {
            ReachedPhase::Stopped
        } else {
            ReachedPhase::Ready
        },
    }
}
