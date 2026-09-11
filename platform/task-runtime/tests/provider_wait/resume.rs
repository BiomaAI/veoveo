use super::*;

#[tokio::test]
async fn resumption_is_atomic_and_requires_the_exact_current_cancellation_and_lease() {
    let db = TestDb::new().await;
    let a = TaskRuntime::new(db.a.clone(), "computers-test", "worker-a");
    let b = TaskRuntime::new(db.b.clone(), "computers-test", "worker-b");
    db.a.client().query("DEFINE TABLE recovery_fixture SCHEMALESS; CREATE recovery_fixture:one SET windows = 0;").await.unwrap().check().unwrap();
    let task = a
        .create(draft(RecoveryClass::ProviderWait))
        .await
        .unwrap()
        .snapshot;
    let id = task.task_id.to_string();
    let claim = a
        .claim_observation(&id, Duration::from_secs(30))
        .await
        .unwrap();
    let body = "UPDATE ONLY recovery_fixture:one SET windows += 1;";
    assert!(
        b.resume_provider_journal(&claim, None, body, vec![])
            .await
            .is_err()
    );
    // Cancellation arriving after a read cannot be acknowledged by that snapshot.
    let cancelled = a.cancel(&id).await.unwrap();
    assert!(
        a.resume_provider_journal(&claim, None, body, vec![])
            .await
            .is_err()
    );
    let claim = a
        .claim_observation(&id, Duration::from_secs(30))
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
            "UPDATE ONLY recovery_fixture:one SET windows += 100; THROW 'domain_refused';",
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
    // An unknown successful reply cannot cause another window with this receipt.
    assert!(
        a.resume_provider_journal(&claim, cancelled.cancel_requested_at, body, vec![])
            .await
            .is_err()
    );
    let claim = a
        .claim_observation(&id, Duration::from_secs(30))
        .await
        .unwrap();
    assert!(
        a.resume_provider_journal(&claim, cancelled.cancel_requested_at, body, vec![])
            .await
            .is_err()
    );
    let recancelled = a.cancel(&id).await.unwrap();
    assert!(recancelled.cancel_requested_at > cancelled.cancel_requested_at);
    assert!(
        a.resume_provider_journal(&claim, None, body, vec![])
            .await
            .is_err()
    );
    let claim = a
        .claim_observation(&id, Duration::from_secs(30))
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
            .query("SELECT VALUE windows FROM ONLY recovery_fixture:one;")
            .await
            .unwrap()
            .check()
            .unwrap()
            .take(0)
            .unwrap();
    assert_eq!(count, Some(1));
    let events: Vec<veoveo_platform_store::OutboxEventRecord> =
        db.a.client()
            .query("SELECT * FROM outbox_event WHERE event_type = 'task.recovery_resumed';")
            .await
            .unwrap()
            .check()
            .unwrap()
            .take(0)
            .unwrap();
    assert_eq!(events.len(), 1);
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
                .claim_observation(&task.task_id.to_string(), Duration::from_secs(30))
                .await
                .unwrap()
        } else {
            runtime
                .claim(&task.task_id.to_string(), Duration::from_secs(30))
                .await
                .unwrap()
        };
        let result = runtime
            .resume_provider_journal(&claim, None, "RETURN NONE;", vec![])
            .await;
        assert_eq!(result.is_ok(), class == RecoveryClass::ProviderWait);
        if let Ok(resumed) = result {
            assert_eq!(resumed.cancel_requested_at, None);
            assert_eq!(resumed.status, TaskStatus::Waiting);
        }
    }
}
