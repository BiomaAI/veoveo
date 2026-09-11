//! Readiness observation is separate from the worker's operation-correlated
//! completion profile. Reconnecting never resubmits an uncertain mutation.
use crate::{
    CapacityHealth, CommandWorker, FileWorker, LifecycleWorker, MaintenanceWorker,
    RuntimePublisher, config::PreparedProvider,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use veoveo_computers::{ComputersStore, api::CapacityAvailability};
use veoveo_computers_runtime::{DevelopmentTemplate, OpenShellRuntime};
use veoveo_task_runtime::TaskRuntime;

fn report(health: &watch::Sender<CapacityHealth>, availability: CapacityAvailability) {
    if health.borrow().availability != availability {
        tracing::info!(?availability, "Computer capacity state changed");
    }
    health.send_replace(CapacityHealth {
        availability,
        observed_at: Instant::now(),
    });
}
pub(super) async fn maintain(
    provider: PreparedProvider,
    store: ComputersStore,
    tasks: TaskRuntime,
    templates: Vec<DevelopmentTemplate>,
    health: watch::Sender<CapacityHealth>,
    access: RuntimePublisher,
    shutdown: CancellationToken,
) {
    let mut reconnect = tokio::time::interval(Duration::from_secs(5));
    reconnect.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            _ = reconnect.tick() => {},
        }
        let config = match provider.gateway.config(store.provider_instance_id()) {
            Ok(config) => config,
            Err(_) => {
                report(&health, CapacityAvailability::ComputeUnavailable);
                continue;
            }
        };
        let connection = tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            result = tokio::time::timeout(Duration::from_secs(10), OpenShellRuntime::connect(config)) => result,
        };
        let runtime = match connection {
            Ok(Ok(runtime)) => runtime,
            _ => {
                report(&health, CapacityAvailability::ComputeUnavailable);
                continue;
            }
        };
        let worker = match LifecycleWorker::new(
            store.clone(),
            tasks.clone(),
            runtime.clone(),
            templates.clone(),
            provider.homes.clone(),
        ) {
            Ok(worker) => Arc::new(worker),
            Err(_) => {
                report(&health, CapacityAvailability::Maintenance);
                return;
            }
        };
        let commands = match CommandWorker::new(
            store.clone(),
            tasks.clone(),
            runtime.clone(),
            provider.execution.keys.clone(),
            provider.execution.artifacts.clone(),
            provider.execution.templates.clone(),
        ) {
            Ok(worker) => Arc::new(worker),
            Err(_) => {
                report(&health, CapacityAvailability::Maintenance);
                return;
            }
        };
        let maintenance = match MaintenanceWorker::new(
            store.clone(),
            tasks.clone(),
            runtime.clone(),
            provider.maintenance.clone(),
            provider.homes.clone(),
            provider.execution.keys.clone(),
        ) {
            Ok(worker) => Arc::new(worker),
            Err(_) => {
                report(&health, CapacityAvailability::Maintenance);
                return;
            }
        };
        let files = match FileWorker::new(
            store.clone(),
            tasks.clone(),
            runtime.clone(),
            provider.execution.keys.clone(),
            provider.execution.artifacts.clone(),
            provider.execution.file_templates.clone(),
        ) {
            Ok(worker) => Arc::new(worker),
            Err(_) => {
                report(&health, CapacityAvailability::Maintenance);
                return;
            }
        };
        let stop = shutdown.child_token();
        let _cancel_worker_on_drop = stop.clone().drop_guard();
        let job = tokio::spawn(worker.run(stop.clone()));
        let command_job = tokio::spawn(commands.run(stop.clone()));
        let maintenance_job = tokio::spawn(maintenance.run(stop.clone()));
        let file_job = tokio::spawn(files.run(stop.clone()));
        let mut probes = tokio::time::interval(Duration::from_secs(5));
        probes.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let healthy = tokio::select! {
                biased;
                _ = shutdown.cancelled() => false,
                _ = probes.tick() => {
                    let (compute, storage) = tokio::join!(
                        tokio::time::timeout(Duration::from_secs(5), runtime.ready()),
                        tokio::time::timeout(Duration::from_secs(5), provider.homes.ready()),
                    );
                    let compute = matches!(compute, Ok(Ok(()))) && !job.is_finished()
                        && !command_job.is_finished() && !maintenance_job.is_finished() && !file_job.is_finished();
                    if compute { access.available(runtime.clone()); } else { access.unavailable(); }
                    report(&health, if !compute {CapacityAvailability::ComputeUnavailable}
                        else if matches!(storage, Ok(Ok(()))) {CapacityAvailability::Available}
                        else {CapacityAvailability::StorageUnavailable});
                    compute
                }
            };
            if !healthy {
                break;
            }
        }
        access.unavailable();
        stop.cancel();
        // The worker only abandons its local observation/dispatch futures. Durable
        // leases, provider execution and retained homes survive this process.
        tokio::join!(
            finish_worker(job),
            finish_worker(command_job),
            finish_worker(maintenance_job),
            finish_worker(file_job),
        );
        if shutdown.is_cancelled() {
            return;
        }
    }
}

async fn finish_worker(mut job: tokio::task::JoinHandle<()>) {
    if tokio::time::timeout(Duration::from_secs(5), &mut job)
        .await
        .is_err()
    {
        job.abort();
        let _ = job.await;
    }
}
