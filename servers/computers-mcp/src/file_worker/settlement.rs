use super::*;
use veoveo_computers::{ReachedPhase, ReachedState};
use veoveo_computers_runtime::{LifecycleCheckpoint, LifecycleObservation};
use veoveo_task_runtime::{TaskFailure, TaskRetentionPin, TaskStatus, TaskTransition};

impl FileWorker {
    pub(super) async fn contain(
        &self,
        claim: &mut ClaimedTask,
        reason: FileInterruption,
    ) -> Result<WorkerStep> {
        let operation = self.store.begin_file_containment(claim, reason).await?;
        let b = operation.binding();
        let binding =
            Binding::from_instance(b.computer_id, b.instance_id, b.template_fingerprint.clone())
                .map_err(|_| FileWorkerError::Configuration)?;
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
                .ok_or(FileWorkerError::Configuration)?,
            binding.clone(),
            &before,
        )
        .map_err(|_| FileWorkerError::Configuration)?;
        if let Some(ticket) = self.store.admit_file_stop(claim).await? {
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
                    .complete_file_stop(claim, ticket, reached(&operation, stopped))
                    .await?;
                self.project(&completed).await?;
                return Ok(WorkerStep::Settled);
            }
            self.recover_lease(claim).await?;
        }
        match self.store.admit_file_containment_read(claim).await? {
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
                        .complete_file_containment_read(claim, ticket, reached(&operation, stopped))
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
    pub(super) async fn project(&self, operation: &FileOperation) -> Result<()> {
        let transition = match operation.outcome().ok_or(FileWorkerError::Configuration)? {
            FileOutcome::Completed(result) => {
                let uri = result.result_uri.clone();
                let mut response = rmcp::model::CallToolResult::structured(
                    serde_json::to_value(result).map_err(|_| FileWorkerError::Configuration)?,
                );
                response.is_error = Some(false);
                response.content = vec![
                    rmcp::model::ContentBlock::text("File transferred"),
                    rmcp::model::ContentBlock::resource_link(rmcp::model::Resource::new(
                        uri,
                        "File transfer result",
                    )),
                ];
                TaskTransition::Succeeded {
                    message: "File transferred".into(),
                    result: serde_json::to_value(response)
                        .map_err(|_| FileWorkerError::Configuration)?,
                }
            }
            FileOutcome::Rejected(reason) => {
                let code =
                    serde_json::to_value(reason).map_err(|_| FileWorkerError::Configuration)?;
                TaskTransition::Failed(TaskFailure::new(
                    code.as_str().ok_or(FileWorkerError::Configuration)?,
                    super::data::rejection_message(reason),
                ))
            }
            FileOutcome::Undispatched(FileRefusal::CancelledBeforeDispatch)
            | FileOutcome::Terminated(FileInterruption::Cancelled) => TaskTransition::Cancelled,
            FileOutcome::Undispatched(reason) => {
                let (code, message) = match reason {
                    FileRefusal::ArtifactUnavailable => (
                        "artifact_unavailable",
                        "The source Artifact could not be prepared for this Computer; verify its access, labels and size",
                    ),
                    FileRefusal::AuthorityDenied => (
                        "authority_denied",
                        "Current authority does not permit this file",
                    ),
                    FileRefusal::RunChanged => (
                        "run_changed",
                        "The Computer run changed before the file started",
                    ),
                    FileRefusal::PreparationExpired => (
                        "preparation_expired",
                        "File preparation expired; retry with a new request ID",
                    ),
                    FileRefusal::CancelledBeforeDispatch => unreachable!(),
                };
                TaskTransition::Failed(TaskFailure::new(code, message))
            }
            FileOutcome::Terminated(reason) => {
                let (code, message) = match reason {
                    FileInterruption::Deadline => (
                        "deadline",
                        "The file deadline expired; the original Computer run is stopped",
                    ),
                    FileInterruption::AuthorityLost => (
                        "authority_lost",
                        "File authority ended; the original Computer run is stopped",
                    ),
                    FileInterruption::ExecutionUnknown => (
                        "execution_unknown",
                        "The file result is unavailable; the original Computer run is stopped",
                    ),
                    FileInterruption::Cancelled => unreachable!(),
                };
                TaskTransition::Failed(TaskFailure::new(code, message))
            }
        };
        self.tasks
            .transition(&operation.task_id().to_string(), transition)
            .await?;
        self.acknowledge(operation).await
    }
    pub(super) async fn acknowledge(&self, operation: &FileOperation) -> Result<()> {
        self.store.acknowledge_file_task(operation).await?;
        self.tasks
            .acknowledge_retention_pin(
                &operation.task_id().to_string(),
                &TaskRetentionPin::new(format!(
                    "computer-file-transfer/{}",
                    operation.transfer_id()
                ))
                .map_err(|_| FileWorkerError::Configuration)?,
            )
            .await?;
        Ok(())
    }
    pub(super) async fn waiting(&self, id: &str, message: &str) -> Result<()> {
        let task = self
            .tasks
            .get(id)
            .await?
            .ok_or(FileWorkerError::LeaseLost)?;
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
fn reached(operation: &FileOperation, observation: Observation) -> ReachedState {
    ReachedState {
        provider_instance_id: operation.binding().provider_instance_id,
        computer_id: operation.computer_id(),
        replacement_instance_id: (operation.binding().instance_id != operation.computer_id())
            .then_some(operation.binding().instance_id),
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
