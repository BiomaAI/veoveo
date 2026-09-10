#![allow(dead_code)] // Shared native fixtures expose scenario-specific operations.
#[path = "../../../platform/runtimes/computers/tests/native_support/block_home.rs"]
mod block_home;
#[path = "../../../platform/runtimes/computers/tests/native_support/mod.rs"]
mod provider;
#[path = "../../../platform/computers/tests/support/mod.rs"]
mod support;
#[path = "../../../platform/runtimes/computers/tests/native_support/template.rs"]
mod template;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};
use uuid::Uuid;
use veoveo_computers::{
    CapacityPolicy, ComputersStore, Operation, OperationStage, Reservation,
    api::{Action, ComputerPhase},
};
use veoveo_computers_mcp::{
    DispatchPermit, LifecycleWorker, Preflight, PreflightError, WorkerStep,
};
use veoveo_computers_runtime::{Binding, DevelopmentTemplate, ExecIntent, PERSISTENT_HOME};
use veoveo_task_runtime::{TaskRuntime, TaskStatus};

/// Test-only gate over the explicitly prepared isolated block volume. This is not
/// evidence for production policy, delegation or allocator implementation.
#[derive(Clone)]
struct FixtureGate {
    computer: Uuid,
    fingerprint: String,
    denied: Arc<AtomicBool>,
    preparations: Arc<AtomicU32>,
}
impl Preflight for FixtureGate {
    async fn prepare_home(
        &self,
        operation: &Operation,
        binding: &Binding,
        template: &DevelopmentTemplate,
    ) -> Result<(), PreflightError> {
        assert_eq!(operation.computer_id, self.computer);
        assert_eq!(binding.computer_id(), self.computer);
        assert_eq!(template.fingerprint(), self.fingerprint);
        self.preparations.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn authorize(&self, operation: &Operation) -> Result<DispatchPermit, PreflightError> {
        let started = tokio::time::Instant::now();
        if self.denied.load(Ordering::SeqCst) {
            return Err(PreflightError::Denied);
        }
        DispatchPermit::issue(operation.operation_id, started, Duration::from_secs(30))
    }
}
async fn expire(tasks: &TaskRuntime, operation: &Operation) {
    tasks
        .platform_store()
        .client()
        .query("UPDATE ONLY $task SET lease_expires_at = time::now() - 1s;")
        .bind(("task", operation.task_id().record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
}
async fn shell(
    runtime: &veoveo_computers_runtime::OpenShellRuntime,
    binding: &Binding,
    script: &str,
) {
    let intent = ExecIntent::new(
        vec!["/bin/bash".into(), "-c".into(), script.into()],
        PERSISTENT_HOME.into(),
        15,
        65536,
        vec![],
    )
    .unwrap();
    let result = runtime
        .execute(binding, &intent, |_| async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0);
}

#[tokio::test]
#[ignore = "requires pinned native provider/image; owns isolated database and privileged 512 MiB block-volume fixture"]
async fn worker_runs_retained_lifecycle_repairs_crashes_and_keeps_unknown_work_fenced() {
    let db = support::TestDb::new().await;
    let mut provider = provider::Provider::start().await;
    let selected = template::retained_template(provider.image.clone());
    let a = ComputersStore::new(db.a.clone(), Uuid::from_u128(100)).unwrap();
    let b = ComputersStore::new(db.b.clone(), Uuid::from_u128(100)).unwrap();
    a.install_capacity(
        None,
        CapacityPolicy {
            per_owner: 2,
            per_tenant: 4,
            provider: 4,
        },
    )
    .await
    .unwrap();
    let actor = support::owner("alice");
    let computer = a
        .reserve(
            &actor,
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development".into(),
                template_fingerprint: selected.fingerprint(),
            },
        )
        .await
        .unwrap();
    let home = block_home::BlockHome::create(
        provider.dir.clone(),
        provider.image.clone(),
        computer.computer_id,
    );
    let gate = FixtureGate {
        computer: computer.computer_id,
        fingerprint: selected.fingerprint(),
        denied: Arc::new(AtomicBool::new(false)),
        preparations: Arc::new(AtomicU32::new(0)),
    };
    let tasks_a = TaskRuntime::new(db.a.clone(), "computers", "worker-a");
    let tasks_b = TaskRuntime::new(db.b.clone(), "computers", "worker-b");
    let worker_a = LifecycleWorker::new(
        a.clone(),
        tasks_a.clone(),
        provider.runtime.clone(),
        vec![selected.clone()],
        gate.clone(),
    )
    .unwrap();
    let worker_b = LifecycleWorker::new(
        b.clone(),
        tasks_b.clone(),
        provider.runtime.clone(),
        vec![selected.clone()],
        gate.clone(),
    )
    .unwrap();
    let binding = Binding::new(computer.computer_id, selected.fingerprint()).unwrap();

    // The worker repairs the missing Task link, then the two replicas compete.
    let create = a
        .queue_operation(
            support::authenticated(&actor),
            computer.computer_id,
            Uuid::now_v7(),
            Action::Create,
        )
        .await
        .unwrap();
    let (left, right) = tokio::join!(worker_a.step(create.clone()), worker_b.step(create.clone()));
    assert!(
        matches!(left, Ok(WorkerStep::Settled)) || matches!(right, Ok(WorkerStep::Settled)),
        "a worker must settle the native Create"
    );
    let ready = a.get(&actor, computer.computer_id).await.unwrap();
    assert_eq!(ready.phase, ComputerPhase::Ready);
    assert!(ready.active_operation.is_none());
    let result = tasks_a
        .get(&create.task_id().to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.status, TaskStatus::Succeeded);
    assert!(result.retention_pins.is_empty());
    assert!(a.pending_operations(None, 100).await.unwrap().is_empty());
    shell(
        &provider.runtime,
        &binding,
        "printf 'worker retained this file' > proof.txt",
    )
    .await;
    let stop = a
        .queue_operation(
            support::authenticated(&actor),
            computer.computer_id,
            Uuid::now_v7(),
            Action::Stop,
        )
        .await
        .unwrap();
    assert_eq!(worker_b.step(stop).await.unwrap(), WorkerStep::Settled);
    let start = a
        .queue_operation(
            support::authenticated(&actor),
            computer.computer_id,
            Uuid::now_v7(),
            Action::Start,
        )
        .await
        .unwrap();
    assert_eq!(worker_a.step(start).await.unwrap(), WorkerStep::Settled);
    let restarted = a.get(&actor, computer.computer_id).await.unwrap();
    assert_ne!(ready.process_id, restarted.process_id);
    shell(
        &provider.runtime,
        &binding,
        "test \"$(cat proof.txt)\" = 'worker retained this file'",
    )
    .await;

    // Simulate worker death after native Stop completed but before domain settlement.
    let stop = a
        .queue_operation(
            support::authenticated(&actor),
            computer.computer_id,
            Uuid::now_v7(),
            Action::Stop,
        )
        .await
        .unwrap();
    a.ensure_operation_task(&actor, stop.operation_id)
        .await
        .unwrap();
    let claim = tasks_a
        .claim_observation(&stop.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    drop(a.begin_dispatch(&claim).await.unwrap());
    let before = provider.runtime.get(&binding).await.unwrap().unwrap();
    let checkpoint = veoveo_computers_runtime::LifecycleCheckpoint::stop(
        Uuid::from_u128(100),
        stop.operation_id,
        binding.clone(),
        &before,
    )
    .unwrap();
    let stopping = provider.runtime.stop(&binding, &before).await.unwrap();
    provider
        .runtime
        .wait_for_lifecycle(&checkpoint, &stopping, Duration::from_secs(30))
        .await
        .unwrap();
    tasks_a.cancel(&stop.task_id().to_string()).await.unwrap();
    expire(&tasks_a, &stop).await;
    let preparations = gate.preparations.load(Ordering::SeqCst);
    assert_eq!(
        worker_b.step(stop.clone()).await.unwrap(),
        WorkerStep::Settled
    );
    assert_eq!(
        gate.preparations.load(Ordering::SeqCst),
        preparations,
        "recovery cannot enter dispatch preflight"
    );
    let result = tasks_b
        .get(&stop.task_id().to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.status, TaskStatus::Succeeded);
    assert!(result.cancel_requested_at.is_some());
    assert_eq!(
        a.operation(&actor, stop.operation_id)
            .await
            .unwrap()
            .observation_reads,
        1
    );

    // Cancel before dispatch and deny current authority without touching the process.
    for cancelled in [true, false] {
        let start = a
            .queue_operation(
                support::authenticated(&actor),
                computer.computer_id,
                Uuid::now_v7(),
                Action::Start,
            )
            .await
            .unwrap();
        a.ensure_operation_task(&actor, start.operation_id)
            .await
            .unwrap();
        if cancelled {
            tasks_a.cancel(&start.task_id().to_string()).await.unwrap();
        }
        gate.denied.store(!cancelled, Ordering::SeqCst);
        assert_eq!(
            worker_a.step(start.clone()).await.unwrap(),
            WorkerStep::Settled
        );
        let result = tasks_a
            .get(&start.task_id().to_string())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            result.status,
            if cancelled {
                TaskStatus::Cancelled
            } else {
                TaskStatus::Failed
            }
        );
        assert!(
            a.operation(&actor, start.operation_id)
                .await
                .unwrap()
                .dispatch_id
                .is_none()
        );
        assert_eq!(
            a.get(&actor, computer.computer_id).await.unwrap().phase,
            ComputerPhase::Stopped
        );
    }
    gate.denied.store(false, Ordering::SeqCst);

    // Lost dispatch ticket before an RPC: even a known stopped resource cannot
    // authorize Start replay. The persisted budget eventually requires recovery.
    let start = a
        .queue_operation(
            support::authenticated(&actor),
            computer.computer_id,
            Uuid::now_v7(),
            Action::Start,
        )
        .await
        .unwrap();
    a.ensure_operation_task(&actor, start.operation_id)
        .await
        .unwrap();
    let claim = tasks_a
        .claim_observation(&start.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    drop(a.begin_dispatch(&claim).await.unwrap());
    expire(&tasks_a, &start).await;
    let preparations = gate.preparations.load(Ordering::SeqCst);
    for _ in 0..8 {
        db.a.client()
            .query("UPDATE ONLY $operation SET next_observation_at = time::now() - 1s;")
            .bind((
                "operation",
                surrealdb::types::RecordId::new(
                    "computer_operation",
                    surrealdb::types::Uuid::from(start.operation_id),
                ),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            worker_b.step(start.clone()).await.unwrap(),
            WorkerStep::Waiting
        );
    }
    assert_eq!(
        worker_b.step(start.clone()).await.unwrap(),
        WorkerStep::RecoveryRequired
    );
    assert_eq!(gate.preparations.load(Ordering::SeqCst), preparations);
    let protected = a.get(&actor, computer.computer_id).await.unwrap();
    assert_eq!(protected.phase, ComputerPhase::RecoveryRequired);
    assert_eq!(protected.active_operation, Some(start.operation_id));
    assert_eq!(
        a.operation(&actor, start.operation_id).await.unwrap().stage,
        OperationStage::RecoveryRequired
    );
    assert!(a.pending_operations(None, 100).await.unwrap().is_empty());
    assert!(
        tasks_b
            .get(&start.task_id().to_string())
            .await
            .unwrap()
            .unwrap()
            .lease_owner
            .is_none()
    );
    assert!(
        provider.runtime.get(&binding).await.unwrap().unwrap().phase
            == veoveo_computers_runtime::Phase::Stopped
    );
    provider.assert_running();
    home.finish();
}
