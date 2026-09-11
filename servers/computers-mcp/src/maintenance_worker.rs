//! One retained maintenance journal drives provider execution across replicas.
mod native;
mod profiles;
mod scheduler;
mod task;
use crate::{RetainedHomes, WorkerError, WorkerStep};
use futures::FutureExt;
pub use profiles::{MaintenanceProfiles, MaintenanceTransition};
use std::{sync::Arc, time::Duration};
use veoveo_computers::{
    ComputersStore,
    maintenance::{
        MaintenanceObservationAdmission, MaintenanceOperation, MaintenanceRecovery,
        MaintenanceStage,
    },
    secrets::ComputerKeyRing,
};
use veoveo_computers_runtime::OpenShellRuntime;
use veoveo_task_runtime::{ClaimedTask, TaskError, TaskRuntime};

type Result<T> = std::result::Result<T, WorkerError>;
const LEASE: Duration = Duration::from_secs(60);

pub struct MaintenanceWorker {
    store: ComputersStore,
    tasks: TaskRuntime,
    runtime: OpenShellRuntime,
    profiles: MaintenanceProfiles,
    homes: RetainedHomes,
    keys: Arc<ComputerKeyRing>,
}
impl MaintenanceWorker {
    pub fn new(
        store: ComputersStore,
        tasks: TaskRuntime,
        runtime: OpenShellRuntime,
        profiles: MaintenanceProfiles,
        homes: RetainedHomes,
        keys: Arc<ComputerKeyRing>,
    ) -> Result<Self> {
        if tasks.server() != "computers"
            || store.provider_instance_id() != runtime.provider_instance_id()
            || homes.provider_id() != store.provider_instance_id()
        {
            return Err(WorkerError::Configuration);
        }
        Ok(Self {
            store,
            tasks,
            runtime,
            profiles,
            homes,
            keys,
        })
    }
    pub async fn step(&self, operation: MaintenanceOperation) -> Result<WorkerStep> {
        let id = operation.task_id().to_string();
        let task = match self.tasks.get(&id).await? {
            Some(task) => task,
            None => {
                self.store
                    .ensure_maintenance_task(&operation.actor, operation.operation_id)
                    .await?;
                self.tasks
                    .get(&id)
                    .await?
                    .ok_or_else(|| TaskError::NotFound(id.clone()))?
            }
        };
        if task.is_terminal() {
            let current = self
                .store
                .maintenance(&operation.actor, operation.operation_id)
                .await?;
            self.acknowledge(&current).await?;
            return Ok(WorkerStep::Settled);
        }
        let mut claim = match self.tasks.claim_observation(&id, LEASE).await {
            Ok(claim) => claim,
            Err(TaskError::Conflict(_) | TaskError::LeaseHeld(_)) => return Ok(WorkerStep::Busy),
            Err(error) => return Err(error.into()),
        };
        let result = self.advance(&mut claim).boxed().await;
        if matches!(&result, Ok(WorkerStep::RecoveryRequired)) {
            let current = self.store.maintenance_for_claim(&claim).await?;
            self.waiting(
                &current,
                "Recovery required; files and operation history are retained",
            )
            .await?;
        }
        if matches!(
            &result,
            Ok(WorkerStep::Waiting | WorkerStep::RecoveryRequired)
        ) {
            self.tasks.release_observation(&claim).await?;
        }
        result
    }
    async fn advance(&self, claim: &mut ClaimedTask) -> Result<WorkerStep> {
        // At most six provider stages and one adoption; each has its own durable
        // deadline. Yield after a deferred observation rather than spinning.
        for _ in 0..8 {
            let operation = self.store.maintenance_for_claim(claim).await?;
            match operation.stage {
                MaintenanceStage::Succeeded | MaintenanceStage::Cancelled => {
                    self.project(&operation).await?;
                    return Ok(WorkerStep::Settled);
                }
                MaintenanceStage::RecoveryRequired => {
                    self.waiting(
                        &operation,
                        "Recovery required; files and operation history are retained",
                    )
                    .await?;
                    return Ok(WorkerStep::RecoveryRequired);
                }
                _ => {}
            }
            let cancel = self
                .tasks
                .is_cancel_requested(&operation.task_id().to_string())
                .await?;
            if cancel && operation.steps().is_empty() {
                let cancelled = self.store.cancel_undispatched_maintenance(claim).await?;
                self.project(&cancelled).await?;
                return Ok(WorkerStep::Settled);
            }
            let pending = operation
                .steps()
                .last()
                .is_some_and(|step| step.evidence.is_none());
            if cancel && !pending {
                self.store
                    .pause_maintenance(claim, MaintenanceRecovery::CancellationRequested)
                    .await?;
                return Ok(WorkerStep::RecoveryRequired);
            }
            if self.profiles.for_operation(&operation).is_err() {
                self.waiting(
                    &operation,
                    "The recorded environment transition is unavailable",
                )
                .await?;
                return Ok(WorkerStep::Waiting);
            }
            if operation.stage == MaintenanceStage::Adopting {
                let ticket = match self.store.observe_maintenance_adoption(claim).await? {
                    MaintenanceObservationAdmission::Read(ticket) => ticket,
                    MaintenanceObservationAdmission::Wait { .. } => return Ok(WorkerStep::Waiting),
                    MaintenanceObservationAdmission::RecoveryRequired => {
                        return Ok(WorkerStep::RecoveryRequired);
                    }
                };
                let saved = claim.clone();
                let native = self
                    .with_lease(
                        claim,
                        tokio::time::timeout(
                            ticket.remaining(),
                            self.qualify_target(&saved, &operation),
                        ),
                    )
                    .await?;
                if !matches!(native, Ok(Ok(()))) {
                    self.waiting(
                        &operation,
                        "Checking the replacement environment; access remains fenced",
                    )
                    .await?;
                    return Ok(WorkerStep::Waiting);
                }
                match self.store.adopt_maintenance(claim, ticket).await {
                    Ok(complete) => {
                        self.project(&complete).await?;
                        return Ok(WorkerStep::Settled);
                    }
                    Err(veoveo_computers::ComputerError::Forbidden) => {
                        self.store
                            .pause_maintenance(claim, MaintenanceRecovery::AuthorityDenied)
                            .await?;
                        return Ok(WorkerStep::RecoveryRequired);
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            if !pending
                && operation.next_step()
                    == Some(veoveo_computers::maintenance::MaintenanceStep::Create)
            {
                let prepared = self
                    .with_lease(
                        claim,
                        tokio::time::timeout(
                            Duration::from_secs(180),
                            self.prepare_create(&operation),
                        ),
                    )
                    .await?;
                if !matches!(prepared, Ok(Ok(()))) {
                    self.waiting(&operation, "Preparing the replacement's retained storage")
                        .await?;
                    return Ok(WorkerStep::Waiting);
                }
            }
            let ticket = if pending {
                match self.store.observe_maintenance_step(claim).await? {
                    MaintenanceObservationAdmission::Read(ticket) => ticket,
                    MaintenanceObservationAdmission::Wait { .. } => return Ok(WorkerStep::Waiting),
                    MaintenanceObservationAdmission::RecoveryRequired => {
                        return Ok(WorkerStep::RecoveryRequired);
                    }
                }
            } else {
                match self.store.begin_maintenance_step(claim).await {
                    Ok(ticket) => ticket,
                    Err(veoveo_computers::ComputerError::Forbidden) => {
                        self.store
                            .pause_maintenance(claim, MaintenanceRecovery::AuthorityDenied)
                            .await?;
                        return Ok(WorkerStep::RecoveryRequired);
                    }
                    Err(error) => return Err(error.into()),
                }
            };
            self.waiting(ticket.operation(), task::message(ticket.step()))
                .await?;
            let saved = claim.clone();
            let reached = self
                .with_lease(claim, self.perform(&saved, &ticket))
                .await?;
            match reached {
                Ok(native::Reached::Evidence(evidence)) => {
                    self.store
                        .complete_maintenance_step(claim, ticket, evidence)
                        .await?;
                }
                Ok(native::Reached::Capture(checkpoint)) => {
                    self.store
                        .complete_maintenance_capture(claim, ticket, &self.keys, &checkpoint)
                        .await?;
                }
                Err(_) => return Ok(WorkerStep::Waiting),
            }
        }
        Ok(WorkerStep::Waiting)
    }
}
