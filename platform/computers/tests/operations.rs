mod support;
use support::*;
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_computers::{
    CapacityPolicy, ComputerError, ComputersStore, OperationStage, Reservation,
    api::{Action, ComputerPhase},
};
use veoveo_task_runtime::{RecoveryClass, TaskRuntime, TaskStatus};

async fn installed(db: veoveo_platform_store::PlatformStore) -> ComputersStore {
    let store = ComputersStore::new(db, Uuid::from_u128(1)).unwrap();
    store
        .install_capacity(
            None,
            CapacityPolicy {
                per_owner: 1,
                per_tenant: 10,
                provider: 10,
            },
        )
        .await
        .unwrap();
    store
}
fn request() -> Reservation {
    Reservation {
        request_id: Uuid::now_v7(),
        template_id: "development".into(),
        template_fingerprint: FINGERPRINT.into(),
    }
}

#[tokio::test]
async fn contended_operation_updates_never_overwrite_a_previously_acquired_fence() {
    let db = TestDb::new().await;
    let a = installed(db.a.clone()).await;
    let b = installed(db.b.clone()).await;
    for round in 0..8 {
        let actor = owner(&format!("contended-{round}"));
        let computer = a.reserve(&actor, &request()).await.unwrap();
        let results = futures::future::join_all((0..32).map(|index| {
            let store = if index % 2 == 0 { &a } else { &b };
            store.queue_operation(&actor, computer.computer_id, Uuid::now_v7(), Action::Create)
        }))
        .await;
        let winners: Vec<_> = results.into_iter().filter_map(Result::ok).collect();
        assert_eq!(
            winners.len(),
            1,
            "one operation per Computer under contention"
        );
        assert_eq!(
            a.get(&actor, computer.computer_id)
                .await
                .unwrap()
                .active_operation,
            Some(winners[0].operation_id)
        );
    }
}

#[tokio::test]
async fn concurrent_operation_retry_has_one_fence_task_and_audit_identity() {
    let db = TestDb::new().await;
    let a = installed(db.a.clone()).await;
    let b = installed(db.b.clone()).await;
    let alice = owner("alice");
    let computer = a.reserve(&alice, &request()).await.unwrap();
    let id = computer.computer_id;
    let request_id = Uuid::now_v7();
    let pending = futures::future::join_all((0..8).map(|i| {
        let store = if i % 2 == 0 { &a } else { &b };
        store.queue_operation(&alice, id, request_id, Action::Create)
    }))
    .await;
    let op_id = pending[0].as_ref().unwrap().operation_id;
    for result in pending {
        assert_eq!(result.unwrap().operation_id, op_id);
    }
    let computer = a.get(&alice, id).await.unwrap();
    assert_eq!(computer.active_operation, Some(op_id));
    assert_eq!(computer.phase, ComputerPhase::Provisioning);
    let task_runtime = TaskRuntime::new(db.a.clone(), "computers", "worker");
    assert!(
        task_runtime
            .get(&op_id.to_string())
            .await
            .unwrap()
            .is_none()
    );
    // Simulate a crash after journal commit: another replica repairs only the Task link.
    let repaired = b.ensure_operation_task(&alice, op_id).await.unwrap();
    let retries =
        futures::future::join_all((0..4).map(|_| a.ensure_operation_task(&alice, op_id))).await;
    assert!(retries.iter().all(Result::is_ok));
    let task = task_runtime.get(&op_id.to_string()).await.unwrap().unwrap();
    assert_eq!(task.task_id, repaired.task_id());
    assert_eq!(task.recovery_class, RecoveryClass::ProviderWait);
    assert_eq!(task.status, TaskStatus::Queued);
    assert_eq!(task.retention_pins.len(), 1);
    assert_eq!(repaired.stage, OperationStage::Queued);
    assert_eq!(repaired.previous_phase, ComputerPhase::Reserved);
    assert_eq!(repaired.provider_instance_id, Uuid::from_u128(1));
    assert_eq!(repaired.template_fingerprint, FINGERPRINT);
    assert!(matches!(
        a.queue_operation(&alice, id, request_id, Action::Stop)
            .await,
        Err(ComputerError::RequestConflict)
    ));
    assert!(matches!(
        a.queue_operation(&alice, id, Uuid::now_v7(), Action::Create)
            .await,
        Err(ComputerError::OperationBusy)
    ));
    let events = db.a.read_outbox(0, 100).await.unwrap().events;
    assert_eq!(
        events
            .iter()
            .filter(|e| e.event_type == "computer.operation_queued")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.event_type == "task.created")
            .count(),
        1
    );
    // Task recovery neither queues a second effect nor clears the Computer fence.
    assert_eq!(
        task_runtime.recover().await.unwrap().provider_waiting.len(),
        1
    );
    assert_eq!(
        a.get(&alice, id).await.unwrap().active_operation,
        Some(op_id)
    );
}

#[tokio::test]
async fn competing_requests_and_private_authority_cannot_replace_the_active_operation() {
    let db = TestDb::new().await;
    let a = installed(db.a.clone()).await;
    let b = installed(db.b.clone()).await;
    let alice = owner("alice");
    let bob = owner("bob");
    let computer = a.reserve(&alice, &request()).await.unwrap();
    let id = computer.computer_id;
    assert!(matches!(
        a.queue_operation(&bob, id, Uuid::now_v7(), Action::Create)
            .await,
        Err(ComputerError::NotFound)
    ));
    assert!(matches!(
        a.queue_operation(&alice, id, Uuid::now_v7(), Action::Start)
            .await,
        Err(ComputerError::InvalidState)
    ));
    let (left, right) = tokio::join!(
        a.queue_operation(&alice, id, Uuid::now_v7(), Action::Create),
        b.queue_operation(&alice, id, Uuid::now_v7(), Action::Create)
    );
    let selected = match (left, right) {
        (Ok(op), Err(ComputerError::OperationBusy))
        | (Err(ComputerError::OperationBusy), Ok(op)) => op,
        other => panic!("unexpected operation race: {other:?}"),
    };
    assert!(matches!(
        b.operation(&bob, selected.operation_id).await,
        Err(ComputerError::NotFound)
    ));
    assert!(matches!(
        b.ensure_operation_task(&bob, selected.operation_id).await,
        Err(ComputerError::NotFound)
    ));
    assert_eq!(
        a.get(&alice, id).await.unwrap().active_operation,
        Some(selected.operation_id)
    );
}

#[tokio::test]
async fn unreadable_output_policy_is_rejected_before_consuming_capacity() {
    let db = TestDb::new().await;
    let a = installed(db.a.clone()).await;
    let mut alice = owner("alice");
    alice
        .authority
        .output_policy
        .data_labels
        .insert(veoveo_mcp_contract::DataLabelId::new("restricted").unwrap());
    assert!(matches!(
        a.reserve(&alice, &request()).await,
        Err(ComputerError::Forbidden)
    ));
    alice.data_labels.insert("restricted".into());
    a.reserve(&alice, &request()).await.unwrap();
    let mut query =
        db.a.client()
            .query("SELECT * FROM computer_usage;")
            .await
            .unwrap()
            .check()
            .unwrap();
    let usage: Vec<surrealdb::types::Value> = query.take(0).unwrap();
    assert_eq!(usage.len(), 3);
    assert!(
        usage
            .iter()
            .all(|row| row.get("retained") == 1_i64.into_value())
    );
}

#[tokio::test]
async fn action_admission_preserves_the_previous_run_and_checks_current_membership() {
    let db = TestDb::new().await;
    let store = installed(db.a.clone()).await;
    let mut alice = owner("alice");
    alice.authority.membership = veoveo_mcp_contract::WorkContextMembershipLevel::Viewer;
    assert!(matches!(
        store.reserve(&alice, &request()).await,
        Err(ComputerError::Forbidden)
    ));
    alice.authority.membership = veoveo_mcp_contract::WorkContextMembershipLevel::Contributor;
    let computer = store.reserve(&alice, &request()).await.unwrap();
    let record = surrealdb::types::RecordId::new(
        "computer",
        surrealdb::types::Uuid::from(computer.computer_id),
    );
    // Simulated completed provider observation; this fixture tests durable admission.
    db.a.client().query("UPDATE ONLY $computer SET phase = 'stopped', provider_resource_id = 'sandbox-1', process_id = 'run-1', updated_at = time::now();").bind(("computer", record)).await.unwrap().check().unwrap();
    alice.authority.membership = veoveo_mcp_contract::WorkContextMembershipLevel::Viewer;
    assert!(matches!(
        store
            .queue_operation(&alice, computer.computer_id, Uuid::now_v7(), Action::Start)
            .await,
        Err(ComputerError::Forbidden)
    ));
    assert!(
        store
            .get(&alice, computer.computer_id)
            .await
            .unwrap()
            .active_operation
            .is_none()
    );
    alice.authority.membership = veoveo_mcp_contract::WorkContextMembershipLevel::Contributor;
    let operation = store
        .queue_operation(&alice, computer.computer_id, Uuid::now_v7(), Action::Start)
        .await
        .unwrap();
    assert_eq!(operation.previous_phase, ComputerPhase::Stopped);
    assert_eq!(operation.previous_resource_id.as_deref(), Some("sandbox-1"));
    assert_eq!(operation.previous_process_id.as_deref(), Some("run-1"));
    assert_eq!(
        store.get(&alice, computer.computer_id).await.unwrap().phase,
        ComputerPhase::Starting
    );
}
