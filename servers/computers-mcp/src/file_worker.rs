//! Governed regular-file movement through one native attempt and one Task.
mod data;
mod lease;
mod scheduler;
mod settlement;

use crate::{WorkerStep, io_guard as guard};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_computer_execution::FileRequest;
use veoveo_computers::{
    ComputerError, ComputersStore, api::FileTransferStage, files::*, secrets::ComputerKeyRing,
};
use veoveo_computers_runtime::{Binding, FileTransferResult, Observation, OpenShellRuntime, Phase};
use veoveo_task_runtime::{ClaimedTask, TaskError, TaskRuntime};

const LEASE_DURATION: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum FileWorkerError {
    #[error(transparent)]
    Domain(#[from] ComputerError),
    #[error(transparent)]
    Task(#[from] TaskError),
    #[error("Computer file configuration is unavailable")]
    Configuration,
    #[error("Computer file Task lease is unavailable")]
    LeaseLost,
    #[error("The Artifact transfer is unavailable")]
    ArtifactUnavailable,
}
type Result<T> = std::result::Result<T, FileWorkerError>;

pub struct FileWorker {
    store: ComputersStore,
    tasks: TaskRuntime,
    runtime: OpenShellRuntime,
    keys: Arc<ComputerKeyRing>,
    artifacts: HttpArtifactPlane,
    templates: BTreeSet<String>,
}
impl FileWorker {
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
            return Err(FileWorkerError::Configuration);
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
    pub async fn step(&self, operation: FileOperation) -> Result<WorkerStep> {
        if operation.task_projected() {
            self.acknowledge(&operation).await?;
            return Ok(WorkerStep::Settled);
        }
        let id = operation.task_id().to_string();
        let task = match self.tasks.get(&id).await? {
            Some(task) => task,
            None => {
                self.store.ensure_file_task(&operation).await?;
                self.tasks
                    .get(&id)
                    .await?
                    .ok_or(FileWorkerError::LeaseLost)?
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
        let operation = self.store.file_for_claim(&claim).await?;
        let step = match operation.stage() {
            FileTransferStage::Completed
            | FileTransferStage::Cancelled
            | FileTransferStage::Failed => {
                self.project(&operation).await?;
                WorkerStep::Settled
            }
            FileTransferStage::RecoveryRequired => {
                self.waiting(
                    &id,
                    "Recovery Required; the original transfer remains protected",
                )
                .await?;
                WorkerStep::RecoveryRequired
            }
            FileTransferStage::Queued => self.dispatch(&mut claim, &operation).await?,
            FileTransferStage::Dispatched | FileTransferStage::Containing => {
                self.contain(&mut claim, FileInterruption::ExecutionUnknown)
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
        operation: &FileOperation,
    ) -> Result<WorkerStep> {
        let id = operation.task_id().to_string();
        if self.tasks.is_cancel_requested(&id).await? {
            return self
                .refuse(claim, FileRefusal::CancelledBeforeDispatch)
                .await;
        }
        if chrono::Utc::now() - operation.created_at() > chrono::TimeDelta::seconds(301) {
            return self.refuse(claim, FileRefusal::PreparationExpired).await;
        }
        if !self
            .templates
            .contains(&operation.binding().template_fingerprint)
        {
            self.waiting(
                &id,
                "Update this Computer to a template qualified for file transfers",
            )
            .await?;
            return Ok(WorkerStep::Waiting);
        }
        if operation.file_capability_request(&self.keys)?.is_some() {
            self.waiting(&id, "Preparing authorized file access")
                .await?;
            return Ok(WorkerStep::Waiting);
        }
        let preparation = match tokio::time::timeout(
            Duration::from_secs(10),
            self.store.prepare_file_transfer(claim, &self.keys),
        )
        .await
        {
            Ok(Ok(preparation)) => preparation,
            Ok(Err(ComputerError::Forbidden | ComputerError::NotFound)) => {
                return self.refuse(claim, FileRefusal::AuthorityDenied).await;
            }
            Ok(Err(ComputerError::InvalidState | ComputerError::StateConflict)) => {
                return self.refuse(claim, FileRefusal::RunChanged).await;
            }
            _ => {
                self.waiting(&id, "Waiting for current transfer authority")
                    .await?;
                return Ok(WorkerStep::Waiting);
            }
        };
        let capture = Arc::new(Mutex::new(data::Buffer::new(
            preparation.authority.maximum_bytes,
        )));
        self.waiting(&id, "Preparing source file").await?;
        // Borrow the private preparation while the guard owns its independent timer.
        let initial = FileRunAuthority {
            valid_until: preparation.authority.valid_until,
            execution_deadline: preparation.authority.execution_deadline,
            maximum_bytes: preparation.authority.maximum_bytes,
            deadline_reason: preparation.authority.deadline_reason,
        };
        let prepared = guard::run(
            initial,
            self.prepare_source(&preparation, capture.clone()),
            Duration::from_secs(1),
            || self.refresh(claim, &preparation.operation),
            |maximum| {
                capture
                    .lock()
                    .is_ok_and(|mut buffer| buffer.constrain(maximum))
            },
        )
        .await;
        self.recover_lease(claim).await?;
        let hash = match prepared {
            Ok(Ok(hash)) => hash,
            Ok(Err(_)) => return self.refuse(claim, FileRefusal::ArtifactUnavailable).await,
            Err(guard::Interrupted::Domain(FileInterruption::Cancelled)) => {
                return self
                    .refuse(claim, FileRefusal::CancelledBeforeDispatch)
                    .await;
            }
            Err(guard::Interrupted::Domain(FileInterruption::Deadline)) => {
                return self.refuse(claim, FileRefusal::PreparationExpired).await;
            }
            Err(_) => return self.refuse(claim, FileRefusal::AuthorityDenied).await,
        };
        // Source hashing and authorization are preparation. Only now reserve the
        // one native attempt under current Computer, Task and named-grant authority.
        let ticket = match self.store.begin_file_dispatch(claim, &self.keys).await {
            Ok(ticket) => ticket,
            Err(ComputerError::Forbidden) => {
                return self.refuse(claim, FileRefusal::AuthorityDenied).await;
            }
            Err(
                ComputerError::InvalidState
                | ComputerError::StateConflict
                | ComputerError::OperationBusy,
            ) => return self.refuse(claim, FileRefusal::RunChanged).await,
            Err(_) => {
                self.waiting(&id, "Waiting for current transfer authority")
                    .await?;
                return Ok(WorkerStep::Waiting);
            }
        };
        if hash.is_some()
            && !preparation
                .retained_labels
                .is_subset(ticket.retained_labels())
        {
            return self.contain(claim, FileInterruption::AuthorityLost).await;
        }
        let body = if hash.is_some() {
            capture
                .lock()
                .map_err(|_| FileWorkerError::Configuration)?
                .take()
        } else {
            vec![]
        };
        let count = body.len() as u64;
        if count > ticket.limits().maximum_bytes
            || !capture
                .lock()
                .map_err(|_| FileWorkerError::Configuration)?
                .constrain(ticket.limits().maximum_bytes)
        {
            return self.contain(claim, FileInterruption::AuthorityLost).await;
        }
        let path = ticket.payload().transfer().path().as_str().to_owned();
        let request = match hash {
            Some(hash) => FileRequest::import(path, count, hash),
            None => FileRequest::export(path, ticket.limits().maximum_bytes),
        }
        .map_err(|_| FileWorkerError::Configuration)?;
        let binding = Binding::from_instance(
            ticket.binding().computer_id,
            ticket.binding().instance_id,
            ticket.binding().template_fingerprint.clone(),
        )
        .map_err(|_| FileWorkerError::Configuration)?;
        let expected = Observation {
            sandbox_id: ticket.binding().resource_id.clone(),
            main_process_instance_id: ticket.binding().process_id.clone(),
            phase: Phase::Ready,
            exit_code: None,
        };
        let mut input = body.as_slice();
        let source = hash.map(|_| &mut input as &mut (dyn tokio::io::AsyncRead + Unpin + Send));
        self.waiting(&id, "Transferring file").await?;
        let initial = FileRunAuthority {
            valid_until: ticket.authority_deadline(),
            execution_deadline: ticket.execution_deadline(),
            maximum_bytes: ticket.limits().maximum_bytes,
            deadline_reason: FileInterruption::Deadline,
        };
        let output = capture.clone();
        let observed = guard::run(
            initial,
            self.runtime.transfer_file(
                &binding,
                &expected,
                &request,
                source,
                ticket.limits().maximum_seconds,
                move |bytes| {
                    let result = output
                        .lock()
                        .map_err(|_| veoveo_computers_runtime::RuntimeFailure::ExecutionUnknown)
                        .and_then(|mut buffer| {
                            buffer.append(&bytes).map_err(|_| {
                                veoveo_computers_runtime::RuntimeFailure::ExecutionUnknown
                            })
                        });
                    async move { result }
                },
            ),
            Duration::from_secs(1),
            || self.refresh(claim, ticket.operation()),
            |maximum| {
                maximum >= count
                    && capture
                        .lock()
                        .is_ok_and(|mut buffer| buffer.constrain(maximum))
            },
        )
        .await;
        self.recover_lease(claim).await?;
        let result = match observed {
            Ok(Ok(FileTransferResult::Completed(receipt))) => Ok(receipt),
            Ok(Ok(FileTransferResult::Rejected(reason))) => Err(reason),
            Err(guard::Interrupted::Domain(reason)) => return self.contain(claim, reason).await,
            Err(guard::Interrupted::LeaseLost) => {
                return self.contain(claim, FileInterruption::AuthorityLost).await;
            }
            Ok(Err(_)) => {
                return self
                    .contain(claim, FileInterruption::ExecutionUnknown)
                    .await;
            }
        };
        let exit = match ticket.observe_file_result(result) {
            Ok(exit) => exit,
            Err(_) => {
                return self
                    .contain(claim, FileInterruption::ExecutionUnknown)
                    .await;
            }
        };
        self.waiting(&id, "Completing file transfer").await?;
        let bytes = capture
            .lock()
            .map_err(|_| FileWorkerError::Configuration)?
            .take();
        match self
            .with_lease(
                claim,
                exit.publication_deadline(),
                self.publish(&exit, bytes),
            )
            .await
        {
            Ok(Ok(artifact)) => {
                let completed = self
                    .store
                    .complete_file_result(claim, exit, artifact)
                    .await?;
                self.project(&completed).await?;
                Ok(WorkerStep::Settled)
            }
            _ => {
                self.contain(claim, FileInterruption::ExecutionUnknown)
                    .await
            }
        }
    }
    async fn refuse(&self, claim: &ClaimedTask, reason: FileRefusal) -> Result<WorkerStep> {
        let operation = self.store.abort_queued_file(claim, reason).await?;
        self.project(&operation).await?;
        Ok(WorkerStep::Settled)
    }
}
