#[path = "maintenance/journal.rs"]
mod journal;
#[path = "maintenance/reads.rs"]
mod reads;
#[path = "maintenance/resume.rs"]
mod resume;
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
use veoveo_platform_store::task_record_id;
use veoveo_task_runtime::{RecoveryClass, TaskRuntime};

fn control() -> GatewayControlPlane {
    let mut control = support::interactive::control();
    for name in ["update_template", "resume_update"] {
        let name = LocalToolName::parse(name).unwrap();
        control.servers[0].tools.push(name.clone());
        control.policies[0].rules[0].tools.insert(name);
    }
    control
}
fn target() -> MaintenanceTarget {
    MaintenanceTarget {
        template_id: "development-next".parse().unwrap(),
        template_fingerprint: "b".repeat(64),
    }
}
async fn ready(
    db: &TestDb,
) -> (
    ComputersStore,
    ComputersStore,
    ComputerActor,
    veoveo_computers_contract::ComputerId,
) {
    let actor = support::authenticated(&owner("alice"));
    let (a, b, id) = support::interactive::ready(db, &actor).await;
    support::policy::install(&db.a, control()).await;
    (a, b, actor, id)
}

#[tokio::test]
async fn maintenance_source_projection_mismatch_rejects_reads_and_worker_claims() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let db = support::database().await;
        let (a, b, actor, computer_id) = ready(&db).await;
        let operation = a
            .queue_maintenance(
                &actor,
                computer_id,
                veoveo_computers::api::RequestId::new(),
                &target(),
            )
            .await
            .unwrap();
        let runtime = TaskRuntime::new(db.a.clone(), "computers", "projection-check");
        let claim = runtime
            .claim_observation(operation.task_id(), Duration::from_secs(60))
            .await
            .unwrap();
        for kind in ["stopped", "ready"] {
            db.a.client()
                .query(include_str!("queries/maintenance/source_kind.surql"))
                .bind((
                    "operation",
                    RecordId::new(
                        "computer_maintenance",
                        StoreUuid::from(operation.operation_id.as_uuid()),
                    ),
                ))
                .bind(("source_kind", kind))
                .await
                .unwrap()
                .check()
                .unwrap();
            if kind == "stopped" {
                assert!(matches!(
                    b.maintenance(actor.owner(), operation.operation_id).await,
                    Err(ComputerError::Unavailable)
                ));
                assert!(matches!(
                    a.maintenance_for_claim(&claim).await,
                    Err(ComputerError::Unavailable)
                ));
            } else {
                assert_eq!(
                    b.maintenance(actor.owner(), operation.operation_id)
                        .await
                        .unwrap()
                        .source,
                    operation.source
                );
                assert_eq!(
                    a.maintenance_for_claim(&claim).await.unwrap().source,
                    operation.source
                );
            }
        }
    })
    .await
    .expect("maintenance source projection checks exceeded 180 seconds");
}

#[tokio::test]
async fn replicas_share_one_replacement_task_fence_and_retained_capacity() {
    let db = support::database().await;
    let (a, b, actor, computer_id) = ready(&db).await;
    let before = a.get(actor.owner(), computer_id).await.unwrap();
    let request_id = veoveo_computers::api::RequestId::new();
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
        assert_eq!(operation.source_instance_id, computer_id.as_uuid());
        assert_eq!(operation.stage, MaintenanceStage::Queued);
        assert_eq!(operation.target, target);
        assert!(matches!(operation.source, MaintenanceSource::Ready { .. }));
    }
    assert_ne!(expected.target_instance_id, computer_id.as_uuid());
    let after = b.get(actor.owner(), computer_id).await.unwrap();
    assert_eq!(after.active_operation, Some(expected.operation_id));
    assert_eq!(after.phase, before.phase);
    assert_eq!(after.template_fingerprint, before.template_fingerprint);
    assert_eq!(after.instance_id(), before.instance_id());
    assert_eq!(after.provider_resource_id, before.provider_resource_id);
    let tasks = TaskRuntime::new(db.b.clone(), "computers", "other-worker");
    let task = tasks.get(expected.task_id()).await.unwrap().unwrap();
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
        a.queue_maintenance(
            &actor,
            computer_id,
            veoveo_computers::api::RequestId::new(),
            &target
        )
        .await,
        Err(ComputerError::OperationBusy)
    ));
    assert!(matches!(
        a.queue_operation(
            support::authenticated(actor.owner()),
            computer_id,
            veoveo_computers::api::RequestId::new(),
            Action::Stop
        )
        .await,
        Err(ComputerError::OperationBusy)
    ));
    let mut reply = db.a.client().query(include_str!("queries/maintenance/replicas_share_one_replacement_task_fence_and_retained_capacity/statement_1.surql"))
        .await.unwrap().check().unwrap();
    let retained: Vec<i64> = reply.take(0).unwrap();
    assert_eq!(retained, vec![1, 1, 1]);
    assert_eq!(
        reply.take::<Vec<RecordId>>(1).unwrap(),
        vec![RecordId::new(
            "computer",
            StoreUuid::from(computer_id.as_uuid())
        )]
    );
    assert_eq!(
        reply.take::<Vec<Uuid>>(2).unwrap(),
        vec![expected.operation_id.as_uuid()]
    );
    // Simulate lost Task linking on this isolated store. Repair keeps the exact
    // original maintenance and target rather than allocating another instance.
    db.a.client()
        .query(include_str!("queries/maintenance/replicas_share_one_replacement_task_fence_and_retained_capacity/statement_2.surql"))
        .bind(("task", task_record_id(expected.task_id())))
        .await
        .unwrap().check().unwrap();
    let repaired = b
        .ensure_maintenance_task(actor.owner(), expected.operation_id)
        .await
        .unwrap();
    assert_eq!(repaired.target_instance_id, expected.target_instance_id);
    assert_eq!(
        tasks
            .get(expected.task_id())
            .await
            .unwrap()
            .unwrap()
            .request,
        task.request
    );
    // Corrupt private persisted metadata is rejected without releasing the fence.
    db.a.client()
        .query(include_str!("queries/maintenance/replicas_share_one_replacement_task_fence_and_retained_capacity/statement_3.surql"))
        .bind((
            "operation",
            RecordId::new(
                "computer_maintenance",
                StoreUuid::from(expected.operation_id.as_uuid()),
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
    let db = support::database().await;
    support::policy::install(&db.a, control()).await;
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
            &actor,
            &Reservation {
                request_id: veoveo_computers::api::RequestId::new(),
                template_id: "development".parse().unwrap(),
                template_fingerprint: FINGERPRINT.into(),
            },
        )
        .await
        .unwrap();
    let original = a
        .queue_operation(
            support::authenticated(actor.owner()),
            computer.computer_id,
            veoveo_computers::api::RequestId::new(),
            Action::Create,
        )
        .await
        .unwrap();
    a.ensure_operation_task(actor.owner(), original.operation_id)
        .await
        .unwrap();
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "source-worker");
    let claim = tasks
        .claim_observation(original.task_id(), Duration::from_secs(60))
        .await
        .unwrap();
    drop(a.begin_dispatch(&claim).await.unwrap());
    assert!(matches!(
        a.queue_maintenance(
            &actor,
            computer.computer_id,
            veoveo_computers::api::RequestId::new(),
            &target()
        )
        .await,
        Err(ComputerError::InvalidState)
    ));
    // Isolated failure injection advances the already consumed operation budget.
    db.a.client()
        .query(include_str!("queries/maintenance/failed_initial_create_is_fenced_without_reclassifying_its_unknown_effect/statement_1.surql"))
        .bind((
            "operation",
            RecordId::new(
                "computer_operation",
                StoreUuid::from(original.operation_id.as_uuid()),
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
    let replacement = b
        .queue_maintenance(
            &actor,
            computer.computer_id,
            veoveo_computers::api::RequestId::new(),
            &target(),
        )
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
    assert_eq!(retained.instance_id(), computer.computer_id.as_uuid());
    assert!(retained.provider_resource_id.is_none());
    assert!(matches!(
        a.reserve(
            &actor,
            &Reservation {
                request_id: veoveo_computers::api::RequestId::new(),
                template_id: "development".parse().unwrap(),
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
    let db = support::database().await;
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
            veoveo_computers::api::RequestId::new(),
            &target
        )
        .await,
        Err(ComputerError::NotFound)
    ));
    let mut denied = control();
    denied.policies[0].rules[0]
        .tools
        .remove(&LocalToolName::parse("update_template").unwrap());
    support::policy::install(&db.a, denied).await;
    assert!(matches!(
        a.queue_maintenance(
            &actor,
            computer_id,
            veoveo_computers::api::RequestId::new(),
            &target
        )
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
        .query(include_str!("queries/maintenance/maintenance_requires_current_named_policy_private_owner_and_no_execution_slot/statement_1.surql"))
        .bind((
            "slot",
            RecordId::new(
                "computer_execution_slot",
                StoreUuid::from(computer_id.as_uuid()),
            ),
        ))
        .bind(("computer", computer_id.as_uuid()))
        .bind((
            "execution",
            RecordId::new("computer_execution", StoreUuid::from(Uuid::now_v7())),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        a.queue_maintenance(
            &actor,
            computer_id,
            veoveo_computers::api::RequestId::new(),
            &target
        )
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

#[tokio::test]
async fn browser_admits_maintenance_for_an_existing_replacement() {
    let db = support::database().await;
    let actor =
        ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
    let (a, _, id) = support::interactive::ready(&db, &actor).await;
    support::policy::install(&db.a, control()).await;
    db.a.client()
        .query(include_str!("queries/maintenance/browser_admits_maintenance_for_an_existing_replacement/statement_1.surql"))
        .bind((
            "computer",
            RecordId::new("computer", StoreUuid::from(id.as_uuid())),
        ))
        .bind(("replacement", Uuid::now_v7()))
        .await
        .unwrap()
        .check()
        .unwrap();
    a.queue_maintenance(
        &actor,
        id,
        veoveo_computers::api::RequestId::new(),
        &target(),
    )
    .await
    .unwrap();
}
