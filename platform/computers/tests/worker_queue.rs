mod support;
use std::time::Duration;
use support::*;
use uuid::Uuid;
use veoveo_computers::{
    CapacityPolicy, ComputersStore, OperationStage, Reservation, UndispatchedOutcome,
    api::{Action, ComputerPhase},
};
use veoveo_task_runtime::{TaskRetentionPin, TaskRuntime, TaskStatus, TaskTransition};

#[tokio::test]
async fn queued_cancellation_is_atomic_and_projection_ack_survives_pin_interruption() {
    let db = TestDb::new().await;
    let a = ComputersStore::new(db.a.clone(), Uuid::from_u128(1)).unwrap();
    let b = ComputersStore::new(db.b.clone(), Uuid::from_u128(1)).unwrap();
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
    let actor = owner("alice");
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "worker-a");
    for cancel in [false, true] {
        let computer = a
            .reserve(
                &actor,
                &Reservation {
                    request_id: Uuid::now_v7(),
                    template_id: "development".into(),
                    template_fingerprint: FINGERPRINT.into(),
                },
            )
            .await
            .unwrap();
        let op = a
            .queue_operation(&actor, computer.computer_id, Uuid::now_v7(), Action::Create)
            .await
            .unwrap();
        // Discovery sees the orphaned operation before its Task link exists.
        assert_eq!(
            b.pending_operations(None, 1).await.unwrap()[0].operation_id,
            op.operation_id
        );
        a.ensure_operation_task(&actor, op.operation_id)
            .await
            .unwrap();
        let id = op.task_id().to_string();
        let claim = tasks
            .claim_observation(&id, Duration::from_secs(60))
            .await
            .unwrap();
        assert!(
            a.abort_undispatched(&claim, UndispatchedOutcome::CancelledBeforeDispatch)
                .await
                .is_err()
        );
        let outcome = if cancel {
            tasks.cancel(&id).await.unwrap();
            UndispatchedOutcome::CancelledBeforeDispatch
        } else {
            UndispatchedOutcome::AuthorityDenied
        };
        let settled = b.abort_undispatched(&claim, outcome).await.unwrap();
        assert_eq!(settled.completion_code, Some(outcome));
        assert_eq!(
            settled.stage,
            if cancel {
                OperationStage::Cancelled
            } else {
                OperationStage::Failed
            }
        );
        let computer = b.get(&actor, op.computer_id).await.unwrap();
        assert_eq!(computer.phase, ComputerPhase::Reserved);
        assert!(computer.active_operation.is_none());
        assert!(computer.provider_resource_id.is_none());
        assert!(a.begin_dispatch(&claim).await.is_err());
        assert!(a.acknowledge_task_projection(&settled).await.is_err());
        tasks
            .transition(
                &id,
                if cancel {
                    TaskTransition::Cancelled
                } else {
                    TaskTransition::Failed(veoveo_task_runtime::TaskFailure::new(
                        "authority_denied",
                        "denied",
                    ))
                },
            )
            .await
            .unwrap();
        a.acknowledge_task_projection(&settled).await.unwrap();
        // The projection committed but the process died before releasing its pin.
        let repair = b.pending_operations(None, 100).await.unwrap();
        assert_eq!(repair.len(), 1);
        assert!(repair[0].task_projected_at.is_some());
        tasks
            .acknowledge_retention_pin(
                &id,
                &TaskRetentionPin::new(format!("computer-operation/{}", op.operation_id)).unwrap(),
            )
            .await
            .unwrap();
        assert!(b.pending_operations(None, 100).await.unwrap().is_empty());
        assert_eq!(
            tasks.get(&id).await.unwrap().unwrap().status,
            if cancel {
                TaskStatus::Cancelled
            } else {
                TaskStatus::Failed
            }
        );
        // Later Task retention cleanup cannot reconstruct completed work.
        db.a.client()
            .query("DELETE ONLY $task;")
            .bind(("task", op.task_id().record_id()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(b.pending_operations(None, 100).await.unwrap().is_empty());
    }
}

#[tokio::test]
async fn undispatched_abort_cannot_clear_an_uncertain_dispatch() {
    let db = TestDb::new().await;
    let a = ComputersStore::new(db.a.clone(), Uuid::from_u128(1)).unwrap();
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
    let actor = owner("alice");
    let computer = a
        .reserve(
            &actor,
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development".into(),
                template_fingerprint: FINGERPRINT.into(),
            },
        )
        .await
        .unwrap();
    let op = a
        .queue_operation(&actor, computer.computer_id, Uuid::now_v7(), Action::Create)
        .await
        .unwrap();
    a.ensure_operation_task(&actor, op.operation_id)
        .await
        .unwrap();
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "worker-a");
    let id = op.task_id().to_string();
    let claim = tasks
        .claim_observation(&id, Duration::from_secs(60))
        .await
        .unwrap();
    drop(a.begin_dispatch(&claim).await.unwrap());
    tasks.cancel(&id).await.unwrap();
    for outcome in [
        UndispatchedOutcome::AuthorityDenied,
        UndispatchedOutcome::CancelledBeforeDispatch,
    ] {
        assert!(a.abort_undispatched(&claim, outcome).await.is_err());
    }
    let current = a.get(&actor, computer.computer_id).await.unwrap();
    assert_eq!(current.active_operation, Some(op.operation_id));
    assert_eq!(current.phase, ComputerPhase::Provisioning);
}
