mod support;
use std::time::Duration;
use support::*;
use uuid::Uuid;
use veoveo_computers::{
    CapacityPolicy, ComputersStore, ObservationAdmission, Operation, OperationStage, ReachedPhase,
    ReachedState, Reservation,
    api::{Action, ComputerPhase},
};
use veoveo_platform_store::task_record_id;
use veoveo_task_runtime::{ClaimedTask, TaskRuntime, TaskStatus};

async fn setup(db: &TestDb) -> (ComputersStore, ComputersStore, TaskRuntime) {
    support::policy::install_default(&db.a).await;
    let a = ComputersStore::new(
        db.a.clone(),
        "00000000-0000-7000-8000-000000000001"
            .parse::<veoveo_computers::api::ProviderInstanceId>()
            .unwrap(),
        veoveo_gateway_catalog::registry().expect("installed owner catalog recipe"),
    )
    .unwrap();
    let b = ComputersStore::new(
        db.b.clone(),
        "00000000-0000-7000-8000-000000000001"
            .parse::<veoveo_computers::api::ProviderInstanceId>()
            .unwrap(),
        veoveo_gateway_catalog::registry().expect("installed owner catalog recipe"),
    )
    .unwrap();
    a.install_capacity(
        None,
        CapacityPolicy {
            per_owner: 3,
            per_tenant: 10,
            provider: 10,
        },
    )
    .await
    .unwrap();
    (
        a,
        b,
        TaskRuntime::new(db.a.clone(), "computers", "worker-a"),
    )
}
#[tokio::test]
async fn replacement_lifecycle_keeps_instance_identity_through_dispatch_and_settlement() {
    use surrealdb::types::{RecordId, Uuid as StoreUuid};
    let db = TestDb::new().await;
    let (store, replica, tasks) = setup(&db).await;
    let actor = owner("alice");
    let computer = store
        .reserve(
            &crate::support::authenticated(&actor),
            &Reservation {
                request_id: veoveo_computers::api::RequestId::new(),
                template_id: "development".parse().unwrap(),
                template_fingerprint: FINGERPRINT.into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(computer.instance_id(), computer.computer_id.into_uuid());
    let record = RecordId::new(
        "computer",
        StoreUuid::from(computer.computer_id.into_uuid()),
    );
    for invalid in [Uuid::nil(), computer.computer_id.into_uuid()] {
        assert!(
            db.a.client()
                .query("UPDATE $computer SET replacement_instance_id=$instance;")
                .bind(("computer", record.clone()))
                .bind(("instance", invalid))
                .await
                .unwrap()
                .check()
                .is_err()
        );
    }
    // This isolated store fixture represents an adopted maintenance instance.
    // It does not authorize maintenance or mutate installed Computer state.
    let instance = Uuid::now_v7();
    db.a.client()
        .query("UPDATE $computer SET replacement_instance_id=$instance;")
        .bind(("computer", record.clone()))
        .bind(("instance", instance))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        replica
            .get(&actor, computer.computer_id)
            .await
            .unwrap()
            .instance_id(),
        instance
    );
    let (operation, claim) = queue(&store, &tasks, computer.computer_id, Action::Create).await;
    assert_eq!(operation.instance_id(), instance);
    let ticket = store.begin_dispatch(&claim).await.unwrap();
    let mut wrong = reached(&operation, ReachedPhase::Ready, "replacement-run");
    wrong.replacement_instance_id = None;
    assert!(
        store
            .complete_dispatch(&claim, ticket, wrong)
            .await
            .is_err()
    );
    let ObservationAdmission::Read(read) = replica.admit_observation(&claim).await.unwrap() else {
        panic!("expected observation")
    };
    replica
        .complete_observation(
            &claim,
            read,
            reached(&operation, ReachedPhase::Ready, "replacement-run"),
        )
        .await
        .unwrap();
    let (stop, stop_claim) = queue(&store, &tasks, computer.computer_id, Action::Stop).await;
    assert_eq!(stop.instance_id(), instance);
    db.a.client()
        .query("UPDATE $computer SET replacement_instance_id=$instance;")
        .bind(("computer", record))
        .bind(("instance", Uuid::now_v7()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(store.begin_dispatch(&stop_claim).await.is_err());
    assert_eq!(
        replica
            .get(&actor, computer.computer_id)
            .await
            .unwrap()
            .active_operation,
        Some(stop.operation_id)
    );
}
async fn create(store: &ComputersStore, tasks: &TaskRuntime) -> (Operation, ClaimedTask) {
    let actor = owner("alice");
    let computer = store
        .reserve(
            &crate::support::authenticated(&actor),
            &Reservation {
                request_id: veoveo_computers::api::RequestId::new(),
                template_id: "development".parse().unwrap(),
                template_fingerprint: FINGERPRINT.into(),
            },
        )
        .await
        .unwrap();
    queue(store, tasks, computer.computer_id, Action::Create).await
}
async fn queue(
    store: &ComputersStore,
    tasks: &TaskRuntime,
    computer: veoveo_computers_contract::ComputerId,
    action: Action,
) -> (Operation, ClaimedTask) {
    let actor = owner("alice");
    let operation = store
        .queue_operation(
            support::authenticated(&actor),
            computer,
            veoveo_computers::api::RequestId::new(),
            action,
        )
        .await
        .unwrap();
    store
        .ensure_operation_task(&actor, operation.operation_id)
        .await
        .unwrap();
    let claimed = tasks
        .claim_observation(operation.task_id(), Duration::from_secs(60))
        .await
        .unwrap();
    (operation, claimed)
}
fn reached(op: &Operation, phase: ReachedPhase, process: &str) -> ReachedState {
    ReachedState {
        provider_instance_id: op.provider_instance_id,
        computer_id: op.computer_id,
        replacement_instance_id: op.replacement_instance_id,
        template_fingerprint: op.template_fingerprint.clone(),
        resource_id: "resource-1".into(),
        process_id: process.into(),
        phase,
    }
}
async fn due(db: &TestDb, op: &Operation) {
    db.a.client()
        .query("UPDATE ONLY $operation SET next_observation_at = time::now() - 1s;")
        .bind((
            "operation",
            surrealdb::types::RecordId::new(
                "computer_operation",
                surrealdb::types::Uuid::from(op.operation_id.as_uuid()),
            ),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
}

#[tokio::test]
async fn only_one_replica_receives_dispatch_and_domain_settles_before_task_projection() {
    let db = TestDb::new().await;
    let (a, b, tasks) = setup(&db).await;
    let (op, claimed) = create(&a, &tasks).await;
    let (left, right) = tokio::join!(a.begin_dispatch(&claimed), b.begin_dispatch(&claimed));
    let ticket = match (left, right) {
        (Ok(ticket), Err(_)) | (Err(_), Ok(ticket)) => ticket,
        _ => panic!("exactly one dispatch ticket"),
    };
    assert!(!ticket.remaining().is_zero());
    assert!(ticket.remaining() <= Duration::from_secs(180));
    let settled = a
        .complete_dispatch(&claimed, ticket, reached(&op, ReachedPhase::Ready, "run-1"))
        .await
        .unwrap();
    assert_eq!(settled.stage, OperationStage::Succeeded);
    let computer = b.get(&owner("alice"), op.computer_id).await.unwrap();
    assert_eq!(computer.phase, ComputerPhase::Ready);
    assert!(computer.active_operation.is_none());
    assert_eq!(computer.process_id.as_deref(), Some("run-1"));
    assert_eq!(
        tasks.get(op.task_id()).await.unwrap().unwrap().status,
        TaskStatus::Queued
    );
    assert!(b.begin_dispatch(&claimed).await.is_err());
    let events = db
        .committed(veoveo_platform_store::PlatformTable::AuditRecord)
        .await;
    assert_eq!(
        events
            .iter()
            .filter(|e| e["draft"]["detail"]["stage"] == "dispatched")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e["draft"]["detail"]["stage"] == "settled")
            .count(),
        1
    );
}

#[tokio::test]
async fn lost_dispatch_receipt_recovers_by_one_charged_observation_without_replay() {
    let db = TestDb::new().await;
    let (a, b, tasks) = setup(&db).await;
    let (op, claimed) = create(&a, &tasks).await;
    drop(a.begin_dispatch(&claimed).await.unwrap());
    let mut denied = support::policy::control();
    denied.policies[0].rules[0].effect = veoveo_mcp_contract::PolicyEffect::Deny;
    support::policy::install(&db.b, denied).await;
    db.a.client()
        .query("UPDATE ONLY $task SET lease_expires_at = time::now() - 1s;")
        .bind(("task", task_record_id(op.task_id())))
        .await
        .unwrap()
        .check()
        .unwrap();
    let successor = TaskRuntime::new(db.b.clone(), "computers", "worker-b");
    let claimed = successor
        .claim_observation(op.task_id(), Duration::from_secs(60))
        .await
        .unwrap();
    assert!(b.begin_dispatch(&claimed).await.is_err());
    let ObservationAdmission::Read(ticket) = b.admit_observation(&claimed).await.unwrap() else {
        panic!("one bounded read")
    };
    assert_eq!(ticket.operation().observation_reads, 1);
    assert!(ticket.remaining() <= Duration::from_secs(10));
    db.a.client()
        .query("UPDATE ONLY $operation SET next_observation_at = time::now() + 1h;")
        .bind((
            "operation",
            surrealdb::types::RecordId::new(
                "computer_operation",
                surrealdb::types::Uuid::from(op.operation_id.as_uuid()),
            ),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        a.admit_observation(&claimed).await.unwrap(),
        ObservationAdmission::Wait { .. }
    ));
    let settled = b
        .complete_observation(&claimed, ticket, reached(&op, ReachedPhase::Ready, "run-1"))
        .await
        .unwrap();
    assert_eq!(settled.stage, OperationStage::Succeeded);
}

#[tokio::test]
async fn exhausted_budget_survives_replica_change_and_retains_the_computer_fence() {
    let db = TestDb::new().await;
    let (a, b, tasks) = setup(&db).await;
    let (op, claimed) = create(&a, &tasks).await;
    drop(a.begin_dispatch(&claimed).await.unwrap());
    for count in 1..=8 {
        due(&db, &op).await;
        let ObservationAdmission::Read(ticket) = b.admit_observation(&claimed).await.unwrap()
        else {
            panic!("budget admits read")
        };
        assert_eq!(ticket.operation().observation_reads, count);
    }
    for store in [&a, &b, &a] {
        assert!(matches!(
            store.admit_observation(&claimed).await.unwrap(),
            ObservationAdmission::RecoveryRequired
        ));
        assert!(store.begin_dispatch(&claimed).await.is_err());
    }
    let computer = a.get(&owner("alice"), op.computer_id).await.unwrap();
    assert_eq!(computer.phase, ComputerPhase::RecoveryRequired);
    assert_eq!(computer.active_operation, Some(op.operation_id));
    assert_eq!(
        a.operation(&owner("alice"), op.operation_id)
            .await
            .unwrap()
            .observation_reads,
        8
    );
    assert_eq!(
        db.committed(veoveo_platform_store::PlatformTable::AuditRecord)
            .await
            .iter()
            .filter(|e| e["draft"]["detail"]["stage"] == "recovery_required")
            .count(),
        1
    );
    let (expired, claim) = create(&a, &tasks).await;
    drop(a.begin_dispatch(&claim).await.unwrap());
    db.a.client()
        .query("UPDATE ONLY $operation SET observation_deadline = time::now() - 1s;")
        .bind((
            "operation",
            surrealdb::types::RecordId::new(
                "computer_operation",
                surrealdb::types::Uuid::from(expired.operation_id.as_uuid()),
            ),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        b.admit_observation(&claim).await.unwrap(),
        ObservationAdmission::RecoveryRequired
    ));
    let expired = a
        .operation(&owner("alice"), expired.operation_id)
        .await
        .unwrap();
    assert_eq!(expired.observation_reads, 0);
    assert_eq!(
        a.get(&owner("alice"), expired.computer_id)
            .await
            .unwrap()
            .active_operation,
        Some(expired.operation_id)
    );
}

#[tokio::test]
async fn cancellation_blocks_dispatch_and_stale_process_cannot_complete_start() {
    let db = TestDb::new().await;
    let (a, _, tasks) = setup(&db).await;
    let (cancelled, claim) = create(&a, &tasks).await;
    tasks.cancel(cancelled.task_id()).await.unwrap();
    assert!(a.begin_dispatch(&claim).await.is_err());
    assert_eq!(
        a.operation(&owner("alice"), cancelled.operation_id)
            .await
            .unwrap()
            .stage,
        OperationStage::Queued
    );
    let (op, claim) = create(&a, &tasks).await;
    let ticket = a.begin_dispatch(&claim).await.unwrap();
    a.complete_dispatch(&claim, ticket, reached(&op, ReachedPhase::Ready, "run-1"))
        .await
        .unwrap();
    let (stop, claim) = queue(&a, &tasks, op.computer_id, Action::Stop).await;
    let ticket = a.begin_dispatch(&claim).await.unwrap();
    a.complete_dispatch(
        &claim,
        ticket,
        reached(&stop, ReachedPhase::Stopped, "run-1"),
    )
    .await
    .unwrap();
    let (start, claim) = queue(&a, &tasks, op.computer_id, Action::Start).await;
    let ticket = a.begin_dispatch(&claim).await.unwrap();
    assert!(
        a.complete_dispatch(
            &claim,
            ticket,
            reached(&start, ReachedPhase::Ready, "run-1")
        )
        .await
        .is_err()
    );
    assert_eq!(
        a.get(&owner("alice"), op.computer_id)
            .await
            .unwrap()
            .active_operation,
        Some(start.operation_id)
    );
    let ObservationAdmission::Read(ticket) = a.admit_observation(&claim).await.unwrap() else {
        panic!("read new run")
    };
    a.complete_observation(
        &claim,
        ticket,
        reached(&start, ReachedPhase::Ready, "run-2"),
    )
    .await
    .unwrap();
    assert_eq!(
        a.get(&owner("alice"), op.computer_id)
            .await
            .unwrap()
            .process_id
            .as_deref(),
        Some("run-2")
    );
}
