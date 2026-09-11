use super::*;
use veoveo_computers::{
    maintenance::{
        MaintenanceEvidence as Evidence, MaintenanceObservationAdmission as ReadAdmission,
        MaintenanceRecovery, MaintenanceStep,
    },
    secrets::{ComputerKeyRing, ComputerSealingKey, MaintenanceCheckpoint},
};
use veoveo_task_runtime::ClaimedTask;
use zeroize::Zeroizing;

fn keys(n: u8) -> ComputerKeyRing {
    ComputerKeyRing::new(
        Uuid::from_u128(n.into()),
        vec![ComputerSealingKey::new(Uuid::from_u128(n.into()), Zeroizing::new([n; 32])).unwrap()],
    )
    .unwrap()
}
async fn adoption_ticket(
    store: &ComputersStore,
    claim: &ClaimedTask,
) -> veoveo_computers::maintenance::MaintenanceTicket {
    let ReadAdmission::Read(ticket) = store.observe_maintenance_adoption(claim).await.unwrap()
    else {
        panic!("bounded adoption read")
    };
    ticket
}
async fn queued(db: &TestDb) -> (ComputersStore, ComputersStore, TaskRuntime, ClaimedTask) {
    let (a, b, actor, id) = ready(db).await;
    let operation = a
        .queue_maintenance(&actor, id, Uuid::now_v7(), &target())
        .await
        .unwrap();
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "maintenance-a");
    let claim = tasks
        .claim_observation(&operation.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    (a, b, tasks, claim)
}
fn stopped(operation: &veoveo_computers::maintenance::MaintenanceOperation) -> Evidence {
    let MaintenanceSource::Ready {
        resource_id,
        process_id,
    } = &operation.source
    else {
        panic!("ready source")
    };
    Evidence::Stopped {
        resource_id: resource_id.clone(),
        process_id: process_id.clone(),
    }
}

#[tokio::test]
async fn durable_steps_capture_encrypted_policy_and_adopt_exactly_one_instance() {
    let db = TestDb::new().await;
    let (a, b, tasks, claim) = queued(&db).await;
    let operation = a.maintenance_for_claim(&claim).await.unwrap();
    let before = a
        .get(&operation.actor, operation.computer_id)
        .await
        .unwrap();
    let (one, two) = tokio::join!(
        a.begin_maintenance_step(&claim),
        b.begin_maintenance_step(&claim)
    );
    assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
    let ticket = one.or(two).unwrap();
    assert_eq!(ticket.step(), MaintenanceStep::Stop);
    assert!(ticket.is_dispatch());
    assert!(ticket.authority_deadline().unwrap() > std::time::Instant::now());
    a.complete_maintenance_step(&claim, ticket, stopped(&operation))
        .await
        .unwrap();
    assert_eq!(
        b.get(&operation.actor, operation.computer_id)
            .await
            .unwrap()
            .phase,
        ComputerPhase::Stopped
    );
    let ticket = b.begin_maintenance_step(&claim).await.unwrap();
    assert_eq!(ticket.step(), MaintenanceStep::Capture);
    let secret = b"PRIVATE MAINTENANCE POLICY FIXTURE";
    let checkpoint = MaintenanceCheckpoint::new(Zeroizing::new(secret.to_vec())).unwrap();
    let ring = keys(1);
    a.complete_maintenance_capture(&claim, ticket, &ring, &checkpoint)
        .await
        .unwrap();
    drop(checkpoint);
    assert_eq!(
        b.maintenance_checkpoint(&claim, &ring)
            .await
            .unwrap()
            .bytes(),
        secret
    );
    assert!(b.maintenance_checkpoint(&claim, &keys(2)).await.is_err());
    let raw: Option<veoveo_platform_store::OpenObject> =
        db.a.client()
            .select(RecordId::new(
                "computer_maintenance_policy",
                StoreUuid::from(operation.operation_id),
            ))
            .await
            .unwrap();
    assert!(
        !serde_json::to_string(&raw)
            .unwrap()
            .contains(std::str::from_utf8(secret).unwrap())
    );
    for (step, evidence) in [
        (MaintenanceStep::Retire, Evidence::Retired),
        (MaintenanceStep::Transfer, Evidence::Transferred),
        (
            MaintenanceStep::Create,
            Evidence::Created {
                resource_id: "replacement-resource".into(),
                process_id: "replacement-process".into(),
            },
        ),
        (
            MaintenanceStep::Restore,
            Evidence::Restored {
                policy_version: 2,
                policy_hash: "d".repeat(64),
            },
        ),
    ] {
        let ticket = b.begin_maintenance_step(&claim).await.unwrap();
        assert_eq!(ticket.step(), step);
        assert!(a.begin_maintenance_step(&claim).await.is_err());
        a.complete_maintenance_step(&claim, ticket, evidence)
            .await
            .unwrap();
        let current = b
            .get(&operation.actor, operation.computer_id)
            .await
            .unwrap();
        assert_eq!(current.instance_id(), before.instance_id());
        assert_eq!(current.active_operation, Some(operation.operation_id));
    }
    assert_eq!(
        a.maintenance_for_claim(&claim).await.unwrap().stage,
        MaintenanceStage::Adopting
    );
    let complete = b
        .adopt_maintenance(&claim, adoption_ticket(&a, &claim).await)
        .await
        .unwrap();
    assert_eq!(complete.stage, MaintenanceStage::Succeeded);
    let current = b
        .get(&operation.actor, operation.computer_id)
        .await
        .unwrap();
    assert_eq!(current.instance_id(), operation.target_instance_id);
    assert_eq!(
        current.template_fingerprint,
        operation.target.template_fingerprint
    );
    assert_eq!(
        current.provider_resource_id.as_deref(),
        Some("replacement-resource")
    );
    assert_eq!(current.process_id.as_deref(), Some("replacement-process"));
    assert_eq!(current.active_operation, None);
    assert_eq!(current.phase, ComputerPhase::Ready);
    assert_eq!(
        complete
            .steps()
            .iter()
            .map(|s| s.dispatch_id)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        6
    );
    assert!(a.observe_maintenance_adoption(&claim).await.is_err());
    assert_eq!(a.pending_maintenance(None, 100).await.unwrap().len(), 1);
    assert!(a.acknowledge_maintenance_task(&complete).await.is_err());
    tasks
        .transition(
            &complete.task_id().to_string(),
            veoveo_task_runtime::TaskTransition::Succeeded {
                message: "isolated domain adoption proof".into(),
                result: serde_json::json!({"maintenanceId":complete.operation_id}),
            },
        )
        .await
        .unwrap();
    a.acknowledge_maintenance_task(&complete).await.unwrap();
    tasks
        .acknowledge_retention_pin(
            &complete.task_id().to_string(),
            &veoveo_task_runtime::TaskRetentionPin::new(format!(
                "computer-maintenance/{}",
                complete.operation_id
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(b.pending_maintenance(None, 100).await.unwrap().is_empty());
    let retained: Vec<i64> =
        db.a.client()
            .query("SELECT VALUE retained FROM computer_usage;")
            .await
            .unwrap()
            .check()
            .unwrap()
            .take(0)
            .unwrap();
    assert_eq!(retained, vec![1, 1, 1]);
}

#[tokio::test]
async fn lost_worker_ticket_allows_observation_under_new_lease_without_redispatch() {
    let db = TestDb::new().await;
    let (a, b, tasks, claim) = queued(&db).await;
    let ticket = a.begin_maintenance_step(&claim).await.unwrap();
    let operation = ticket.operation().clone();
    let dispatch_id = operation.steps()[0].dispatch_id;
    tasks.release_observation(&claim).await.unwrap();
    let other = TaskRuntime::new(db.b.clone(), "computers", "maintenance-b");
    let next = other
        .claim_observation(&operation.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    assert!(
        a.complete_maintenance_step(&claim, ticket, stopped(&operation))
            .await
            .is_err()
    );
    assert!(b.begin_maintenance_step(&next).await.is_err());
    let ReadAdmission::Read(ticket) = b.observe_maintenance_step(&next).await.unwrap() else {
        panic!("bounded recovery read")
    };
    assert!(!ticket.is_dispatch());
    assert_eq!(ticket.authority_deadline(), None);
    assert_eq!(ticket.operation().steps()[0].dispatch_id, dispatch_id);
    assert!(ticket.remaining() <= Duration::from_secs(10));
    assert!(matches!(
        a.observe_maintenance_step(&next).await.unwrap(),
        ReadAdmission::Wait { .. }
    ));
    a.complete_maintenance_step(&next, ticket, stopped(&operation))
        .await
        .unwrap();
    assert_eq!(
        b.maintenance_for_claim(&next).await.unwrap().next_step(),
        Some(MaintenanceStep::Capture)
    );
    // A Capture commit lost to a new Task lease cannot persist a checkpoint.
    let ticket = b.begin_maintenance_step(&next).await.unwrap();
    other.release_observation(&next).await.unwrap();
    let next = tasks
        .claim_observation(&operation.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    let checkpoint =
        MaintenanceCheckpoint::new(Zeroizing::new(b"ATOMIC CHECKPOINT FIXTURE".to_vec())).unwrap();
    assert!(
        b.complete_maintenance_capture(&claim, ticket, &keys(1), &checkpoint)
            .await
            .is_err()
    );
    let mut read =
        db.a.client()
            .query("SELECT VALUE operation_id FROM computer_maintenance_policy;")
            .await
            .unwrap()
            .check()
            .unwrap();
    assert!(read.take::<Vec<Uuid>>(0).unwrap().is_empty());
    let ReadAdmission::Read(ticket) = a.observe_maintenance_step(&next).await.unwrap() else {
        panic!("read capture again")
    };
    a.complete_maintenance_capture(&next, ticket, &keys(1), &checkpoint)
        .await
        .unwrap();
    assert_eq!(
        b.maintenance_checkpoint(&next, &keys(1))
            .await
            .unwrap()
            .bytes(),
        checkpoint.bytes()
    );
}

pub(super) async fn initial_failure_retains_unknown_source_through_cancel_and_adoption(
    a: &ComputersStore,
    b: &ComputersStore,
    tasks: &TaskRuntime,
    actor: &ComputerActor,
    original: &veoveo_computers::Operation,
    first: &veoveo_computers::maintenance::MaintenanceOperation,
) {
    let claim = tasks
        .claim_observation(&first.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    a.cancel_undispatched_maintenance(&claim).await.unwrap();
    assert_eq!(
        b.get(actor.owner(), original.computer_id)
            .await
            .unwrap()
            .active_operation,
        Some(original.operation_id)
    );
    let operation = b
        .queue_maintenance(actor, original.computer_id, Uuid::now_v7(), &target())
        .await
        .unwrap();
    assert_ne!(operation.target_instance_id, first.target_instance_id);
    let claim = tasks
        .claim_observation(&operation.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    for (step, evidence) in [
        (MaintenanceStep::Transfer, Evidence::Transferred),
        (
            MaintenanceStep::Create,
            Evidence::Created {
                resource_id: "first-known-resource".into(),
                process_id: "first-known-process".into(),
            },
        ),
    ] {
        let ticket = a.begin_maintenance_step(&claim).await.unwrap();
        assert_eq!(ticket.step(), step);
        b.complete_maintenance_step(&claim, ticket, evidence)
            .await
            .unwrap();
    }
    let complete = a
        .adopt_maintenance(&claim, adoption_ticket(b, &claim).await)
        .await
        .unwrap();
    assert_eq!(complete.steps().len(), 2);
    let old = b
        .operation(actor.owner(), original.operation_id)
        .await
        .unwrap();
    assert_eq!(old.stage, OperationStage::RecoveryRequired);
    assert!(old.dispatch_id.is_some());
    let computer = b.get(actor.owner(), old.computer_id).await.unwrap();
    assert_eq!(computer.instance_id(), complete.target_instance_id);
    assert_eq!(computer.phase, ComputerPhase::Ready);
    // These are isolated domain receipts. Actual allocator abandonment remains a
    // required provider/storage worker proof before the production adoption call.
}

#[tokio::test]
async fn observation_count_budget_is_durable_and_independent_of_worker_restarts() {
    let db = TestDb::new().await;
    let (a, b, _tasks, claim) = queued(&db).await;
    let ticket = a.begin_maintenance_step(&claim).await.unwrap();
    let operation = ticket.operation().clone();
    drop(ticket);
    db.a.client().query("UPDATE ONLY $operation SET progress.steps[0].observation_reads = 7, progress.steps[0].last_observation_id = $id, progress.steps[0].next_observation_at = <string>(time::now() - 1s);")
        .bind(("operation",RecordId::new("computer_maintenance",StoreUuid::from(operation.operation_id))))
        .bind(("id",Uuid::now_v7().to_string())).await.unwrap().check().unwrap();
    let ReadAdmission::Read(ticket) = b.observe_maintenance_step(&claim).await.unwrap() else {
        panic!("eighth read")
    };
    assert_eq!(ticket.operation().steps()[0].observation_reads, 8);
    drop(ticket);
    assert!(matches!(
        a.observe_maintenance_step(&claim).await.unwrap(),
        ReadAdmission::RecoveryRequired
    ));
    assert_eq!(
        b.maintenance_for_claim(&claim).await.unwrap().steps()[0].observation_reads,
        8
    );
}

#[tokio::test]
async fn exhausted_step_budget_retains_the_fence_and_cannot_cancel_or_redispatch() {
    let db = TestDb::new().await;
    let (a, b, _tasks, claim) = queued(&db).await;
    let ticket = a.begin_maintenance_step(&claim).await.unwrap();
    let operation = ticket.operation().clone();
    drop(ticket);
    // Isolated journal clock fault: preserve a valid 180-second historical window.
    db.a.client().query("LET $now = time::now(); UPDATE ONLY $operation SET progress.steps[0].dispatched_at = <string>($now - 181s), progress.steps[0].observation_deadline = <string>($now - 1s);")
        .bind(("operation",RecordId::new("computer_maintenance",StoreUuid::from(operation.operation_id))))
        .await.unwrap().check().unwrap();
    assert!(matches!(
        b.observe_maintenance_step(&claim).await.unwrap(),
        ReadAdmission::RecoveryRequired
    ));
    let current = b.maintenance_for_claim(&claim).await.unwrap();
    assert_eq!(
        current.recovery(),
        Some(MaintenanceRecovery::BudgetExhausted)
    );
    assert_eq!(current.steps()[0].observation_reads, 0);
    assert_eq!(
        b.get(&current.actor, current.computer_id)
            .await
            .unwrap()
            .active_operation,
        Some(current.operation_id)
    );
    assert!(a.begin_maintenance_step(&claim).await.is_err());
    assert!(a.cancel_undispatched_maintenance(&claim).await.is_err());
}

#[tokio::test]
async fn cancellation_before_dispatch_preserves_run_and_policy_revocation_blocks_next_step() {
    let db = TestDb::new().await;
    let (a, b, tasks, claim) = queued(&db).await;
    let operation = a.maintenance_for_claim(&claim).await.unwrap();
    tasks
        .cancel(&operation.task_id().to_string())
        .await
        .unwrap();
    assert!(b.begin_maintenance_step(&claim).await.is_err());
    a.cancel_undispatched_maintenance(&claim).await.unwrap();
    let current = b
        .get(&operation.actor, operation.computer_id)
        .await
        .unwrap();
    assert_eq!(current.active_operation, None);
    assert_eq!(current.phase, ComputerPhase::Ready);
    assert_eq!(current.instance_id(), operation.source_instance_id);
    let actor = support::authenticated(&operation.actor);
    let operation = a
        .queue_maintenance(&actor, current.computer_id, Uuid::now_v7(), &target())
        .await
        .unwrap();
    let claim = tasks
        .claim_observation(&operation.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    let ticket = a.begin_maintenance_step(&claim).await.unwrap();
    a.complete_maintenance_step(&claim, ticket, stopped(&operation))
        .await
        .unwrap();
    let mut denied = control();
    denied.policies[0].rules[0]
        .tools
        .remove(&LocalToolName::new("update_template").unwrap());
    support::policy::install(&db.b, denied).await;
    assert!(matches!(
        b.begin_maintenance_step(&claim).await,
        Err(ComputerError::Forbidden)
    ));
    let paused = a
        .pause_maintenance(&claim, MaintenanceRecovery::AuthorityDenied)
        .await
        .unwrap();
    assert_eq!(paused.steps().len(), 1);
    assert_eq!(
        b.get(&paused.actor, paused.computer_id)
            .await
            .unwrap()
            .active_operation,
        Some(paused.operation_id)
    );
}
