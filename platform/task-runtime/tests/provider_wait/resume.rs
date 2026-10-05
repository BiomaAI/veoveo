use super::*;

#[tokio::test]
async fn resumption_is_atomic_and_requires_the_exact_current_cancellation_and_lease() {
    let db = TestDb::new().await;
    let a = TaskRuntime::new(db.a.clone(), "computers-test", "worker-a");
    let b = TaskRuntime::new(db.b.clone(), "computers-test", "worker-b");
    db.a.client().query(include_str!("../queries/provider_wait/resume/resumption_is_atomic_and_requires_the_exact_current_cancellation_and_lease/statement_1.surql")).await.unwrap().check().unwrap();
    let task = a
        .create(draft(RecoveryClass::ProviderWait))
        .await
        .unwrap()
        .snapshot;
    let id = task.task_id;
    let claim = a
        .claim_observation(id, Duration::from_secs(30))
        .await
        .unwrap();
    let body = include_str!(
        "../queries/provider_wait/resume/resumption_is_atomic_and_requires_the_exact_current_cancellation_and_lease/statement_2.surql"
    );
    assert!(
        b.resume_provider_journal(&claim, None, body, vec![])
            .await
            .is_err()
    );
    // Cancellation arriving after a read cannot be acknowledged by that snapshot.
    let cancelled = a.cancel(id).await.unwrap();
    assert!(
        a.resume_provider_journal(&claim, None, body, vec![])
            .await
            .is_err()
    );
    let claim = a
        .claim_observation(id, Duration::from_secs(30))
        .await
        .unwrap();
    assert!(
        a.resume_provider_journal(&claim, None, body, vec![])
            .await
            .is_err()
    );
    assert!(
        a.resume_provider_journal(&claim, Some(chrono::Utc::now()), body, vec![])
            .await
            .is_err()
    );
    assert!(
        a.resume_provider_journal(
            &claim,
            cancelled.cancel_requested_at,
            include_str!("../queries/provider_wait/resume/resumption_is_atomic_and_requires_the_exact_current_cancellation_and_lease/statement_3.surql"),
            vec![]
        )
        .await
        .is_err()
    );
    assert_eq!(current(&a, &task).await.status, TaskStatus::CancelRequested);
    let resumed = a
        .resume_provider_journal(&claim, cancelled.cancel_requested_at, body, vec![])
        .await
        .unwrap();
    assert_eq!(resumed.status, TaskStatus::Waiting);
    assert_eq!(resumed.cancel_requested_at, cancelled.cancel_requested_at);
    assert_eq!(resumed.request, task.request);
    assert_eq!(resumed.retention_pins, task.retention_pins);
    assert_eq!(current(&a, &task).await.updated_at, resumed.updated_at);
    let changes = db
        .committed(veoveo_platform_store::PlatformTable::Task)
        .await;
    assert_eq!(
        changes
            .iter()
            .filter(|row| row["status"] == "waiting")
            .count(),
        1
    );
    // An unknown successful reply cannot cause another window with this receipt.
    assert!(
        a.resume_provider_journal(&claim, cancelled.cancel_requested_at, body, vec![])
            .await
            .is_err()
    );
    let claim = a
        .claim_observation(id, Duration::from_secs(30))
        .await
        .unwrap();
    assert!(
        a.resume_provider_journal(&claim, cancelled.cancel_requested_at, body, vec![])
            .await
            .is_err()
    );
    let recancelled = a.cancel(id).await.unwrap();
    assert!(recancelled.cancel_requested_at > cancelled.cancel_requested_at);
    assert!(
        a.resume_provider_journal(&claim, None, body, vec![])
            .await
            .is_err()
    );
    let claim = a
        .claim_observation(id, Duration::from_secs(30))
        .await
        .unwrap();
    expire(&a, &task).await;
    assert!(
        a.resume_provider_journal(&claim, recancelled.cancel_requested_at, body, vec![])
            .await
            .is_err()
    );
    let count: Option<i64> =
        db.a.client()
            .query(include_str!("../queries/provider_wait/resume/resumption_is_atomic_and_requires_the_exact_current_cancellation_and_lease/statement_4.surql"))
            .await
            .unwrap()
            .check()
            .unwrap()
            .take(0)
            .unwrap();
    assert_eq!(count, Some(1));
}

#[tokio::test]
async fn resumption_rejects_other_recovery_classes_and_preserves_uncancelled_provider_work() {
    let db = TestDb::new().await;
    let runtime = TaskRuntime::new(db.a.clone(), "computers-test", "worker-a");
    for class in [
        RecoveryClass::Resume,
        RecoveryClass::WebhookWait,
        RecoveryClass::InterruptedIndeterminate,
        RecoveryClass::ProviderWait,
    ] {
        let task = runtime.create(draft(class)).await.unwrap().snapshot;
        let claim = if class == RecoveryClass::ProviderWait {
            runtime
                .claim_observation(task.task_id, Duration::from_secs(30))
                .await
                .unwrap()
        } else {
            runtime
                .claim(task.task_id, Duration::from_secs(30))
                .await
                .unwrap()
        };
        let result = runtime
            .resume_provider_journal(&claim, None, include_str!("../queries/provider_wait/resume/resumption_rejects_other_recovery_classes_and_preserves_uncancelled_provider_work/statement_1.surql"), vec![])
            .await;
        assert_eq!(result.is_ok(), class == RecoveryClass::ProviderWait);
        if let Ok(resumed) = result {
            assert_eq!(resumed.cancel_requested_at, None);
            assert_eq!(resumed.status, TaskStatus::Waiting);
        }
    }
}
