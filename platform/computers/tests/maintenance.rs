#[path = "maintenance/journal.rs"]
mod journal;
mod support;
use std::time::Duration;
use support::*;
use surrealdb::types::{RecordId, Uuid as StoreUuid};
use uuid::Uuid;
use veoveo_computers::{
    CapacityPolicy, ComputerActor, ComputerError, ComputersStore, ObservationAdmission,
    OperationStage, Reservation,
    api::{Action, ComputerPhase},
    maintenance::{MaintenanceSource, MaintenanceStage, MaintenanceTarget},
};
use veoveo_mcp_contract::{GatewayControlPlane, LocalToolName};
use veoveo_task_runtime::{RecoveryClass, TaskRuntime};

fn control() -> GatewayControlPlane {
    let mut control = support::interactive::control();
    let name = LocalToolName::new("update_template").unwrap();
    control.servers[0].tools.push(name.clone());
    control.policies[0].rules[0].tools.insert(name);
    control
}
fn target() -> MaintenanceTarget {
    MaintenanceTarget {
        template_id: "development-next".into(),
        template_fingerprint: "b".repeat(64),
    }
}
async fn ready(db: &TestDb) -> (ComputersStore, ComputersStore, ComputerActor, Uuid) {
    let actor = support::authenticated(&owner("alice"));
    let (a, b, id) = support::interactive::ready(db, &actor).await;
    support::policy::install(&db.a, control()).await;
    (a, b, actor, id)
}

#[tokio::test]
async fn replicas_share_one_replacement_task_fence_and_retained_capacity() {
    let db = TestDb::new().await;
    let (a, b, actor, computer_id) = ready(&db).await;
    let before = a.get(actor.owner(), computer_id).await.unwrap();
    let request_id = Uuid::now_v7();
    let target = target();
    let results = futures::future::join_all((0..4).map(|i| {
        let store = if i % 2 == 0 { &a } else { &b };
        store.queue_maintenance(&actor, computer_id, request_id, &target)
    }))
    .await;
    let expected = results[0].as_ref().unwrap().clone();
    for result in results {
        let operation = result.unwrap();
        assert_eq!(operation.operation_id, expected.operation_id);
        assert_eq!(operation.target_instance_id, expected.target_instance_id);
        assert_eq!(operation.source_instance_id, computer_id);
        assert_eq!(operation.stage, MaintenanceStage::Queued);
        assert_eq!(operation.target, target);
        assert!(matches!(operation.source, MaintenanceSource::Ready { .. }));
    }
    assert_ne!(expected.target_instance_id, computer_id);
    let after = b.get(actor.owner(), computer_id).await.unwrap();
    assert_eq!(after.active_operation, Some(expected.operation_id));
    assert_eq!(after.phase, before.phase);
    assert_eq!(after.template_fingerprint, before.template_fingerprint);
    assert_eq!(after.instance_id(), before.instance_id());
    assert_eq!(after.provider_resource_id, before.provider_resource_id);
    let tasks = TaskRuntime::new(db.b.clone(), "computers", "other-worker");
    let task = tasks
        .get(&expected.task_id().to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.owner, *actor.owner());
    assert_eq!(task.recovery_class, RecoveryClass::ProviderWait);
    assert_eq!(
        task.request,
        serde_json::json!({"computerId": computer_id, "maintenanceId": expected.operation_id})
    );
    assert_eq!(task.retention_pins.len(), 1);
    assert!(matches!(
        a.maintenance(&owner("bob"), expected.operation_id).await,
        Err(ComputerError::NotFound)
    ));
    let changed = MaintenanceTarget {
        template_fingerprint: "c".repeat(64),
        ..target.clone()
    };
    assert!(matches!(
        b.queue_maintenance(&actor, computer_id, request_id, &changed)
            .await,
        Err(ComputerError::RequestConflict)
    ));
    assert!(matches!(
        a.queue_maintenance(&actor, computer_id, Uuid::now_v7(), &target)
            .await,
        Err(ComputerError::OperationBusy)
    ));
    assert!(matches!(
        a.queue_operation(
            support::authenticated(actor.owner()),
            computer_id,
            Uuid::now_v7(),
            Action::Stop
        )
        .await,
        Err(ComputerError::OperationBusy)
    ));
    let mut reply = db.a.client().query("SELECT VALUE retained FROM computer_usage; SELECT VALUE aggregate_id FROM outbox_event WHERE event_type = 'computer.maintenance_queued'; SELECT VALUE operation_id FROM computer_maintenance;")
        .await.unwrap().check().unwrap();
    let retained: Vec<i64> = reply.take(0).unwrap();
    assert_eq!(retained, vec![1, 1, 1]);
    assert_eq!(
        reply.take::<Vec<String>>(1).unwrap(),
        vec![computer_id.to_string()]
    );
    assert_eq!(
        reply.take::<Vec<Uuid>>(2).unwrap(),
        vec![expected.operation_id]
    );
    // Simulate lost Task linking on this isolated store. Repair keeps the exact
    // original maintenance and target rather than allocating another instance.
    db.a.client()
        .query("BEGIN TRANSACTION; DELETE task_idempotency WHERE task = $task; DELETE $task; COMMIT TRANSACTION;")
        .bind(("task", expected.task_id().record_id()))
        .await
        .unwrap().check().unwrap();
    let repaired = b
        .ensure_maintenance_task(actor.owner(), expected.operation_id)
        .await
        .unwrap();
    assert_eq!(repaired.target_instance_id, expected.target_instance_id);
    assert_eq!(
        tasks
            .get(&expected.task_id().to_string())
            .await
            .unwrap()
            .unwrap()
            .request,
        task.request
    );
    // Corrupt private persisted metadata is rejected without releasing the fence.
    db.a.client()
        .query("UPDATE ONLY $operation SET target_template_fingerprint = $invalid;")
        .bind((
            "operation",
            RecordId::new(
                "computer_maintenance",
                StoreUuid::from(expected.operation_id),
            ),
        ))
        .bind(("invalid", "g".repeat(64)))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        b.maintenance(actor.owner(), expected.operation_id).await,
        Err(ComputerError::Unavailable)
    ));
    assert_eq!(
        a.get(actor.owner(), computer_id)
            .await
            .unwrap()
            .active_operation,
        Some(expected.operation_id)
    );
}

#[tokio::test]
async fn failed_initial_create_is_fenced_without_reclassifying_its_unknown_effect() {
    let db = TestDb::new().await;
    support::policy::install(&db.a, control()).await;
    let a = ComputersStore::new(db.a.clone(), Uuid::from_u128(1)).unwrap();
    let b = ComputersStore::new(db.b.clone(), Uuid::from_u128(1)).unwrap();
    a.install_capacity(
        None,
        CapacityPolicy {
            per_owner: 1,
            per_tenant: 2,
            provider: 2,
        },
    )
    .await
    .unwrap();
    let actor = support::authenticated(&owner("alice"));
    let computer = a
        .reserve(
            actor.owner(),
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development".into(),
                template_fingerprint: FINGERPRINT.into(),
            },
        )
        .await
        .unwrap();
    let original = a
        .queue_operation(
            support::authenticated(actor.owner()),
            computer.computer_id,
            Uuid::now_v7(),
            Action::Create,
        )
        .await
        .unwrap();
    a.ensure_operation_task(actor.owner(), original.operation_id)
        .await
        .unwrap();
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "source-worker");
    let claim = tasks
        .claim_observation(&original.task_id().to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    drop(a.begin_dispatch(&claim).await.unwrap());
    assert!(matches!(
        a.queue_maintenance(&actor, computer.computer_id, Uuid::now_v7(), &target())
            .await,
        Err(ComputerError::InvalidState)
    ));
    // Isolated failure injection advances the already consumed operation budget.
    db.a.client()
        .query("UPDATE ONLY $operation SET observation_deadline = time::now() - 1s;")
        .bind((
            "operation",
            RecordId::new("computer_operation", StoreUuid::from(original.operation_id)),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        b.admit_observation(&claim).await.unwrap(),
        ObservationAdmission::RecoveryRequired
    ));
    let replacement = b
        .queue_maintenance(&actor, computer.computer_id, Uuid::now_v7(), &target())
        .await
        .unwrap();
    assert_eq!(
        replacement.source,
        MaintenanceSource::InitialFailure {
            operation_id: original.operation_id
        }
    );
    let before = a
        .operation(actor.owner(), original.operation_id)
        .await
        .unwrap();
    assert_eq!(before.stage, OperationStage::RecoveryRequired);
    assert!(before.dispatch_id.is_some());
    assert!(a.begin_dispatch(&claim).await.is_err());
    let retained = a.get(actor.owner(), computer.computer_id).await.unwrap();
    assert_eq!(retained.active_operation, Some(replacement.operation_id));
    assert_eq!(retained.phase, ComputerPhase::RecoveryRequired);
    assert_eq!(retained.instance_id(), computer.computer_id);
    assert!(retained.provider_resource_id.is_none());
    assert!(matches!(
        a.reserve(
            actor.owner(),
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development".into(),
                template_fingerprint: FINGERPRINT.into()
            }
        )
        .await,
        Err(ComputerError::CapacityFull)
    ));
    journal::initial_failure_retains_unknown_source_through_cancel_and_adoption(
        &a,
        &b,
        &tasks,
        &actor,
        &original,
        &replacement,
    )
    .await;
}

#[tokio::test]
async fn maintenance_requires_current_named_policy_private_owner_and_no_execution_slot() {
    let db = TestDb::new().await;
    let (a, _, actor, computer_id) = ready(&db).await;
    let target = target();
    let bob = owner("bob");
    db.a.ensure_identity(
        bob.tenant_key(),
        &bob.principal_key,
        &bob.issuer,
        &bob.subject,
        bob.principal_kind,
    )
    .await
    .unwrap();
    assert!(matches!(
        a.queue_maintenance(
            &support::authenticated(&owner("bob")),
            computer_id,
            Uuid::now_v7(),
            &target
        )
        .await,
        Err(ComputerError::NotFound)
    ));
    let mut denied = control();
    denied.policies[0].rules[0]
        .tools
        .remove(&LocalToolName::new("update_template").unwrap());
    support::policy::install(&db.a, denied).await;
    assert!(matches!(
        a.queue_maintenance(&actor, computer_id, Uuid::now_v7(), &target)
            .await,
        Err(ComputerError::Forbidden)
    ));
    assert!(
        a.get(actor.owner(), computer_id)
            .await
            .unwrap()
            .active_operation
            .is_none()
    );
    support::policy::install(&db.a, control()).await;
    // A registered command slot cannot be silently taken over by template work.
    db.a.client()
        .query("CREATE $slot CONTENT { computer_id: $computer, execution: $execution };")
        .bind((
            "slot",
            RecordId::new("computer_execution_slot", StoreUuid::from(computer_id)),
        ))
        .bind(("computer", computer_id))
        .bind((
            "execution",
            RecordId::new("computer_execution", StoreUuid::from(Uuid::now_v7())),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        a.queue_maintenance(&actor, computer_id, Uuid::now_v7(), &target)
            .await,
        Err(ComputerError::OperationBusy)
    ));
    assert!(
        a.get(actor.owner(), computer_id)
            .await
            .unwrap()
            .active_operation
            .is_none()
    );
}
