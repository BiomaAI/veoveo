//! Production scheduling admission controls, with no renderer or GPU claim.
use super::{test_support::*, *};
use veoveo_task_runtime::TaskError;

#[tokio::test]
async fn capture_claim_preserves_admitted_owner_and_immutable_input_before_dispatch() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let identity = identity();
        let request = capture(&identity);
        let runtime =
            veoveo_task_runtime::TaskRuntime::new(db.a.clone(), SERVER_SLUG, "capture-admission");
        let id = create(&runtime, &identity, &request).await;
        let admitted = runtime.get(id).await.unwrap().unwrap();
        let claimed = runtime
            .claim(id, Duration::from_secs(30))
            .await
            .unwrap()
            .snapshot;
        validate_claimed_snapshot(&admitted, &claimed).unwrap();
        let mut changed = claimed.clone();
        changed.request = serde_json::json!({"changed":true});
        assert!(validate_claimed_snapshot(&admitted, &changed).is_err());
        changed = claimed.clone();
        changed.owner.profile = "other-profile".into();
        assert!(validate_claimed_snapshot(&admitted, &changed).is_err());
        changed = claimed.clone();
        changed.server = "other-server".into();
        assert!(validate_claimed_snapshot(&admitted, &changed).is_err());
        changed = claimed.clone();
        changed.task_type = veoveo_types::TaskTypeName::new("other-operation").unwrap();
        assert!(validate_claimed_snapshot(&admitted, &changed).is_err());
        changed = claimed.clone();
        changed.recovery_class = RecoveryClass::InterruptedIndeterminate;
        assert!(validate_claimed_snapshot(&admitted, &changed).is_err());
        let resource_owner = request.validate_owner(&claimed.owner).unwrap();
        assert_eq!(
            resource_owner,
            request.validate_owner(&admitted.owner).unwrap()
        );
    })
    .await
    .expect("capture admission control exceeded 60 seconds");
}

#[tokio::test]
async fn claim_error_handoff_requires_current_settlement_or_another_live_worker() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let identity = identity();
        let request = capture(&identity);
        let winner = veoveo_task_runtime::TaskRuntime::new(db.a.clone(), SERVER_SLUG, "winner");
        let replacement =
            veoveo_task_runtime::TaskRuntime::new(db.b.clone(), SERVER_SLUG, "replacement");
        let id = create(&winner, &identity, &request).await;
        let admitted = winner.get(id).await.unwrap().unwrap();
        let live_error = replacement
            .claim(id, Duration::from_secs(30))
            .await
            .unwrap_err();
        assert!(matches!(live_error, TaskError::LeaseHeld(_)));
        replacement
            .reconcile_recovery_claim(&admitted, live_error.into())
            .await
            .unwrap();
        let mut changed_admission = admitted.clone();
        changed_admission.owner.profile = "changed-owner".into();
        assert!(
            replacement
                .reconcile_recovery_claim(
                    &changed_admission,
                    TaskError::Conflict(id.to_string()).into()
                )
                .await
                .is_err()
        );
        // Inject claim error categories at the production reconciliation seam. The
        // durable states below are real Store rows; this does not induce contention.
        for error in [
            TaskError::Conflict(id.to_string()),
            TaskError::LeaseHeld(id.to_string()),
        ] {
            replacement
                .reconcile_recovery_claim(&admitted, error.into())
                .await
                .unwrap();
        }
        assert!(
            winner
                .reconcile_recovery_claim(&admitted, TaskError::Conflict(id.to_string()).into())
                .await
                .is_err(),
            "own lease is not a replacement worker"
        );
        for state in ["unowned", "expired"] {
            let sql = match state {
                "unowned" => "UPDATE $task SET lease_owner = NONE, lease_expires_at = NONE",
                _ => "UPDATE $task SET lease_owner = 'winner', lease_expires_at = time::now() - 1s",
            };
            db.a.client()
                .query(sql)
                .bind(("task", veoveo_platform_store::task_record_id(id)))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(
                replacement
                    .reconcile_recovery_claim(&admitted, TaskError::Conflict(id.to_string()).into())
                    .await
                    .is_err(),
                "{state} Task was silently abandoned"
            );
        }
        // A foreign-server row is present even when normal runtime visibility hides it.
        // Its malformed payload must never be decoded by recovery reconciliation.
        for malformed in [false, true] {
            db.a.client()
                .query(if malformed {
                    include_str!(
                        "../../../queries/server/tasks/recovery_tests/reassign_malformed.surql"
                    )
                } else {
                    include_str!("../../../queries/server/tasks/recovery_tests/reassign.surql")
                })
                .bind(("task", veoveo_platform_store::task_record_id(id)))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(replacement.get(id).await.unwrap().is_none());
            if malformed {
                let foreign = veoveo_task_runtime::TaskRuntime::new(
                    db.a.clone(),
                    "foreign",
                    "foreign-reader",
                );
                assert!(
                    foreign.get(id).await.is_err(),
                    "fixture payload was not malformed"
                );
            }
            let error = replacement
                .reconcile_recovery_claim(&admitted, TaskError::NotFound(id.to_string()).into())
                .await
                .unwrap_err();
            assert!(
                matches!(
                    error.downcast_ref::<TaskError>(),
                    Some(TaskError::WrongServer(_))
                ),
                "foreign Task was decoded or treated as absent: {error:#}"
            );
        }
        db.a.client()
            .query(include_str!(
                "../../../queries/server/tasks/recovery_tests/restore_owner.surql"
            ))
            .bind(("task", veoveo_platform_store::task_record_id(id)))
            .bind((
                "owner",
                veoveo_platform_store::TaskOwnerRecord::try_from(&admitted.owner).unwrap(),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        winner.claim(id, Duration::from_secs(30)).await.unwrap();
        finish(&winner, id, completed(&request)).await;
        assert!(
            replacement
                .reconcile_recovery_claim(
                    &changed_admission,
                    TaskError::Conflict(id.to_string()).into()
                )
                .await
                .is_err(),
            "terminal Task with a different admitted owner proved settlement"
        );
        let terminal_error = replacement
            .claim(id, Duration::from_secs(30))
            .await
            .unwrap_err();
        replacement
            .reconcile_recovery_claim(&admitted, terminal_error.into())
            .await
            .unwrap();
        for error in [
            TaskError::Conflict(id.to_string()),
            TaskError::InvalidTransition {
                from: veoveo_task_runtime::TaskStatus::Succeeded,
                to: veoveo_task_runtime::TaskStatus::Running,
            },
        ] {
            replacement
                .reconcile_recovery_claim(&admitted, error.into())
                .await
                .unwrap();
        }
        db.a.client()
            .query(include_str!(
                "../../../queries/server/tasks/recovery_tests/delete.surql"
            ))
            .bind(("task", veoveo_platform_store::task_record_id(id)))
            .await
            .unwrap()
            .check()
            .unwrap();
        replacement
            .reconcile_recovery_claim(
                &admitted,
                replacement
                    .claim(id, Duration::from_secs(30))
                    .await
                    .unwrap_err()
                    .into(),
            )
            .await
            .unwrap();
    })
    .await
    .expect("claim handoff control exceeded 60 seconds");
}
