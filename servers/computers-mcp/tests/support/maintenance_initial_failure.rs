//! Lost initial dispatch uses the real allocator's unclaimed-home fence.
use crate::{schedulers, settle, shell, support};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use uuid::Uuid;
use veoveo_computers::{
    ComputerActor, ComputersStore, ObservationAdmission, OperationStage, Reservation,
    api::{Action, ComputerPhase},
    maintenance::*,
};
use veoveo_computers_mcp::MaintenanceWorker;
use veoveo_computers_runtime::{Binding, DevelopmentTemplate, HomeAllocator, OpenShellRuntime};
use veoveo_task_runtime::TaskRuntime;

pub struct Scenario<'a> {
    pub db: &'a support::TestDb,
    pub store: &'a ComputersStore,
    pub tasks: &'a TaskRuntime,
    pub owner: &'a ComputerActor,
    pub source: (&'a str, &'a DevelopmentTemplate),
    pub target: (&'a str, &'a DevelopmentTemplate),
    pub allocator: HomeAllocator,
    pub runtime: &'a OpenShellRuntime,
    pub workers: [Arc<MaintenanceWorker>; 2],
}
impl Scenario<'_> {
    pub async fn run(self) {
        let Self {
            db,
            store,
            tasks,
            owner,
            source,
            target,
            allocator,
            runtime,
            workers,
        } = self;
        let computer = store
            .reserve(
                owner.owner(),
                &Reservation {
                    request_id: Uuid::now_v7(),
                    template_id: source.0.into(),
                    template_fingerprint: source.1.fingerprint(),
                },
            )
            .await
            .unwrap();
        let binding = Binding::new(computer.computer_id, source.1.fingerprint()).unwrap();
        allocator.prepare(&binding).await.unwrap();
        let original = store
            .queue_operation(
                ComputerActor::from_verified(&support::browser::identity(db, "alice").await)
                    .unwrap(),
                computer.computer_id,
                Uuid::now_v7(),
                Action::Create,
            )
            .await
            .unwrap();
        store
            .ensure_operation_task(owner.owner(), original.operation_id)
            .await
            .unwrap();
        let claim = tasks
            .claim_observation(&original.task_id().to_string(), Duration::from_secs(60))
            .await
            .unwrap();
        let ticket = store.begin_dispatch(&claim).await.unwrap();
        let dispatch_id = ticket.operation().dispatch_id.unwrap();
        drop(ticket); // Submission is lost before delivery; durable effect remains unknown.
        // Advance only the owned fixture's original finite window. No provider
        // absence observation is substituted for historical effect evidence.
        db.a.client()
            .query("UPDATE ONLY $operation SET observation_deadline = time::now() - 1s;")
            .bind((
                "operation",
                surrealdb::types::RecordId::new(
                    "computer_operation",
                    surrealdb::types::Uuid::from(original.operation_id),
                ),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(matches!(
            store.admit_observation(&claim).await.unwrap(),
            ObservationAdmission::RecoveryRequired
        ));
        tasks.release_observation(&claim).await.unwrap();
        let replacement = store
            .queue_maintenance(
                owner,
                computer.computer_id,
                Uuid::now_v7(),
                &MaintenanceTarget {
                    template_id: target.0.into(),
                    template_fingerprint: target.1.fingerprint(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            replacement.source,
            MaintenanceSource::InitialFailure {
                operation_id: original.operation_id
            }
        );
        let started = Instant::now();
        let running = schedulers::Schedulers::maintenance(workers);
        let completed = settle(store, tasks, owner, &replacement).await;
        running.stop().await;
        assert_eq!(
            completed
                .steps()
                .iter()
                .map(|step| step.step)
                .collect::<Vec<_>>(),
            vec![MaintenanceStep::Transfer, MaintenanceStep::Create]
        );
        let adopted = store
            .get(owner.owner(), computer.computer_id)
            .await
            .unwrap();
        assert_eq!(adopted.phase, ComputerPhase::Ready);
        assert_eq!(adopted.instance_id(), replacement.target_instance_id);
        assert_eq!(adopted.template_fingerprint, target.1.fingerprint());
        assert!(adopted.active_operation.is_none());
        assert!(
            allocator.restore(&binding).await.is_err(),
            "late original allocator access must be denied"
        );
        let current = Binding::from_instance(
            adopted.computer_id,
            adopted.instance_id(),
            adopted.template_fingerprint,
        )
        .unwrap();
        shell(runtime, &current, "printf recovered > initial-failure-marker; test -x /usr/local/bin/veoveo-computer-exec").await;
        let unknown = store
            .operation(owner.owner(), original.operation_id)
            .await
            .unwrap();
        assert_eq!(unknown.stage, OperationStage::RecoveryRequired);
        assert_eq!(unknown.dispatch_id, Some(dispatch_id));
        assert!(unknown.previous_resource_id.is_none());
        assert!(unknown.result_resource_id.is_none());
        assert!(store.begin_dispatch(&claim).await.is_err());
        let usage: Vec<i64> =
            db.a.client()
                .query("SELECT VALUE retained FROM computer_usage;")
                .await
                .unwrap()
                .check()
                .unwrap()
                .take(0)
                .unwrap();
        assert_eq!(
            usage,
            vec![2, 2, 2],
            "recovery cannot release retained quota"
        );
        eprintln!(
            "Native initial-create recovery through allocator Abandon and product worker: {} ms",
            started.elapsed().as_millis()
        );
    }
}
