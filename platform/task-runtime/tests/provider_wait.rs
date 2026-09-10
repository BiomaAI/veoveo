#[path = "../../../testing/fixtures/store.rs"]
mod store;
use std::{collections::BTreeSet, time::Duration};
use store::TestDb;
use uuid::Uuid;
use veoveo_task_runtime::{
    CreateTask, ProviderCommit, RecoveryClass, TaskError, TaskId, TaskOwner, TaskRetentionPin,
    TaskRuntime, TaskSnapshot, TaskStatus, TaskTransition,
};

fn draft(class: RecoveryClass) -> CreateTask {
    let owner: TaskOwner = serde_json::from_value(serde_json::json!({
        "principal_key": "https://provider.test#owner", "principal_kind": "service", "issuer": "https://provider.test",
        "subject": "owner", "profile": "operator", "tenant_key": "test", "data_labels": [],
        "authority": {"work_context": "test-work", "tenant": "test", "membership": "contributor",
            "policy_revision": "test-1", "output_policy": {"owner": {"kind": "principal", "id": "https://provider.test#owner"}},
            "provenance": {"mode": "automated"}}
    })).unwrap();
    CreateTask {
        task_id: TaskId::new(),
        owner,
        server: "computers-test".into(),
        task_type: "lifecycle".into(),
        request: serde_json::json!({"operationId": Uuid::now_v7(), "providerInstanceId": Uuid::now_v7(), "previousProcessId": "run-1"}),
        recovery_class: class,
        idempotency_key: None,
        ttl_ms: Some(60_000),
        poll_interval_ms: Some(1000),
        retention_pins: BTreeSet::from([TaskRetentionPin::new("computer:unresolved").unwrap()]),
    }
}
async fn expire(runtime: &TaskRuntime, task: &TaskSnapshot) {
    runtime
        .platform_store()
        .client()
        .query("UPDATE ONLY $task SET lease_expires_at = time::now() - 1s;")
        .bind(("task", task.task_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
}
async fn current(runtime: &TaskRuntime, task: &TaskSnapshot) -> TaskSnapshot {
    runtime
        .get(&task.task_id.to_string())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn domain_journal_commits_with_the_current_lease_and_preserves_cancellation() {
    use surrealdb::types::{RecordId, SurrealValue};
    let db = TestDb::new().await;
    let a = TaskRuntime::new(db.a.clone(), "computers-test", "worker-a");
    let b = TaskRuntime::new(db.b.clone(), "computers-test", "worker-b");
    db.a.client().query("DEFINE TABLE lease_journal_fixture SCHEMALESS; CREATE lease_journal_fixture:one SET dispatches = 0;").await.unwrap().check().unwrap();
    let task = a
        .create(draft(RecoveryClass::ProviderWait))
        .await
        .unwrap()
        .snapshot;
    let claimed = a
        .claim_observation(&task.task_id.to_string(), Duration::from_secs(30))
        .await
        .unwrap();
    let body = "UPDATE ONLY $journal SET dispatches += 1;";
    let bindings = || {
        vec![(
            "journal",
            RecordId::new("lease_journal_fixture", "one").into_value(),
        )]
    };
    assert!(
        b.commit_provider_journal(&claimed, ProviderCommit::Dispatch, body, bindings())
            .await
            .is_err()
    );
    a.commit_provider_journal(&claimed, ProviderCommit::Dispatch, body, bindings())
        .await
        .unwrap();
    let unchanged = current(&a, &task).await;
    assert_eq!(unchanged.status, TaskStatus::Queued);
    assert_eq!(unchanged.updated_at, claimed.snapshot.updated_at);
    // A rejected domain write rolls back the complete transaction.
    assert!(
        a.commit_provider_journal(
            &claimed,
            ProviderCommit::Dispatch,
            "UPDATE ONLY $journal SET dispatches += 100; THROW 'fixture_domain_rejection';",
            bindings()
        )
        .await
        .is_err()
    );
    let count: Option<i64> =
        db.a.client()
            .query("SELECT VALUE dispatches FROM ONLY lease_journal_fixture:one;")
            .await
            .unwrap()
            .check()
            .unwrap()
            .take(0)
            .unwrap();
    assert_eq!(count, Some(1));
    a.cancel(&task.task_id.to_string()).await.unwrap();
    assert!(
        a.commit_provider_journal(&claimed, ProviderCommit::Dispatch, body, bindings())
            .await
            .is_err()
    );
    a.commit_provider_journal(&claimed, ProviderCommit::Observe, body, bindings())
        .await
        .unwrap();
    expire(&a, &task).await;
    assert!(
        a.commit_provider_journal(&claimed, ProviderCommit::Observe, body, bindings())
            .await
            .is_err()
    );
    let successor = b
        .claim_observation(&task.task_id.to_string(), Duration::from_secs(30))
        .await
        .unwrap();
    assert!(
        a.commit_provider_journal(&claimed, ProviderCommit::Observe, body, bindings())
            .await
            .is_err()
    );
    b.commit_provider_journal(&successor, ProviderCommit::Observe, body, bindings())
        .await
        .unwrap();
    let count: Option<i64> =
        db.a.client()
            .query("SELECT VALUE dispatches FROM ONLY lease_journal_fixture:one;")
            .await
            .unwrap()
            .check()
            .unwrap()
            .take(0)
            .unwrap();
    assert_eq!(count, Some(3));
    assert_eq!(current(&b, &task).await.status, TaskStatus::CancelRequested);
}

#[tokio::test]
async fn renewing_a_task_lease_invalidates_an_old_journal_receipt() {
    let db = TestDb::new().await;
    let runtime = TaskRuntime::new(db.a.clone(), "computers-test", "worker-a");
    let task = runtime
        .create(draft(RecoveryClass::ProviderWait))
        .await
        .unwrap()
        .snapshot;
    let claimed = runtime
        .claim_observation(&task.task_id.to_string(), Duration::from_secs(30))
        .await
        .unwrap();
    let renewed = runtime
        .renew_lease(&task.task_id.to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    assert!(
        runtime
            .commit_provider_journal(&claimed, ProviderCommit::Observe, "RETURN NONE;", vec![])
            .await
            .is_err()
    );
    let fresh = veoveo_task_runtime::ClaimedTask {
        lease_owner: runtime.worker_id().into(),
        lease_expires_at: renewed.lease_expires_at.unwrap(),
        snapshot: renewed,
    };
    runtime
        .commit_provider_journal(&fresh, ProviderCommit::Observe, "RETURN NONE;", vec![])
        .await
        .unwrap();
    assert!(
        runtime
            .commit_provider_journal(
                &fresh,
                ProviderCommit::Observe,
                "RETURN NONE;",
                vec![("_provider_task", surrealdb::types::Value::None)]
            )
            .await
            .is_err()
    );
}
#[tokio::test]
async fn provider_recovery_preserves_every_nonterminal_state_and_cancel_intent() {
    let db = TestDb::new().await;
    let a = TaskRuntime::new(db.a.clone(), "computers-test", "worker-a");
    let b = TaskRuntime::new(db.b.clone(), "computers-test", "worker-b");
    let mut originals = Vec::new();
    for status in [
        TaskStatus::Queued,
        TaskStatus::Running,
        TaskStatus::Waiting,
        TaskStatus::CancelRequested,
    ] {
        let task = a
            .create(draft(RecoveryClass::ProviderWait))
            .await
            .unwrap()
            .snapshot;
        let id = task.task_id.to_string();
        assert!(matches!(
            a.claim(&id, Duration::from_secs(10)).await,
            Err(TaskError::InvalidRecord(_))
        ));
        if status != TaskStatus::Queued {
            a.claim_observation(&id, Duration::from_secs(30))
                .await
                .unwrap();
            a.transition(
                &id,
                TaskTransition::Running {
                    message: "intent persisted".into(),
                    progress: 0.25,
                },
            )
            .await
            .unwrap();
            if status == TaskStatus::Waiting {
                a.transition(
                    &id,
                    TaskTransition::Waiting {
                        message: "watch lost; retaining operation".into(),
                        progress: 0.25,
                    },
                )
                .await
                .unwrap();
            } else if status == TaskStatus::CancelRequested {
                a.cancel(&id).await.unwrap();
            }
            expire(&a, &task).await;
        }
        originals.push(current(&a, &task).await);
    }
    let before = db.a.read_outbox(0, 100).await.unwrap().events.len();
    for _ in 0..2 {
        let report = b.recover().await.unwrap();
        assert_eq!(report.provider_waiting.len(), 4);
        assert!(
            report.resumable.is_empty()
                && report.cancelled.is_empty()
                && report.failed_indeterminate.is_empty()
                && report.webhook_waiting.is_empty()
        );
        for original in &originals {
            let seen = report
                .provider_waiting
                .iter()
                .find(|s| s.task_id == original.task_id)
                .unwrap();
            assert_eq!(seen.status, original.status);
            assert_eq!(seen.request, original.request);
            assert_eq!(seen.updated_at, original.updated_at);
            assert_eq!(seen.progress, original.progress);
            assert_eq!(seen.cancel_requested_at, original.cancel_requested_at);
            assert_eq!(seen.retention_pins, original.retention_pins);
        }
    }
    assert_eq!(db.a.read_outbox(0, 100).await.unwrap().events.len(), before);
    let cancelled = originals.last().unwrap();
    let id = cancelled.task_id.to_string();
    assert!(
        a.transition(&id, TaskTransition::Cancelled).await.is_err(),
        "expired lease must not settle provider cancellation"
    );
    let claimed = b
        .claim_observation(&id, Duration::from_secs(30))
        .await
        .unwrap();
    assert_eq!(claimed.snapshot.status, TaskStatus::CancelRequested);
    assert_eq!(
        claimed.snapshot.cancel_requested_at,
        cancelled.cancel_requested_at
    );
    assert!(a.transition(&id, TaskTransition::Cancelled).await.is_err());
    // Only the current observer can report an authoritative cancellation outcome.
    let result = b.transition(&id, TaskTransition::Cancelled).await.unwrap();
    assert_eq!(result.status, TaskStatus::Cancelled);
    assert_eq!(result.retention_pins, cancelled.retention_pins);
}
#[tokio::test]
async fn observation_claims_race_across_replicas_and_cannot_claim_other_profiles() {
    let db = TestDb::new().await;
    let a = TaskRuntime::new(db.a.clone(), "computers-test", "worker-a");
    let b = TaskRuntime::new(db.b.clone(), "computers-test", "worker-b");
    let task = a
        .create(draft(RecoveryClass::ProviderWait))
        .await
        .unwrap()
        .snapshot;
    let id = task.task_id.to_string();
    let (left, right) = tokio::join!(
        a.claim_observation(&id, Duration::from_secs(30)),
        b.claim_observation(&id, Duration::from_secs(30))
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    let original = current(&a, &task).await;
    assert_eq!(original.status, TaskStatus::Queued);
    assert_eq!(original.request, task.request);
    let winner = if left.is_ok() { &a } else { &b };
    let renewed = winner
        .renew_lease(&id, Duration::from_secs(60))
        .await
        .unwrap();
    assert_eq!(renewed.status, TaskStatus::Queued);
    assert!(renewed.lease_expires_at > original.lease_expires_at);
    assert!(a.recover().await.unwrap().provider_waiting.is_empty());
    for class in [
        RecoveryClass::Resume,
        RecoveryClass::WebhookWait,
        RecoveryClass::InterruptedIndeterminate,
    ] {
        let task = a.create(draft(class)).await.unwrap().snapshot;
        assert!(matches!(
            b.claim_observation(&task.task_id.to_string(), Duration::from_secs(30))
                .await,
            Err(TaskError::InvalidRecord(_))
        ));
    }
}
#[tokio::test]
async fn existing_recovery_profiles_keep_their_qualified_behavior() {
    let db = TestDb::new().await;
    let a = TaskRuntime::new(db.a.clone(), "computers-test", "worker-a");
    let b = TaskRuntime::new(db.b.clone(), "computers-test", "worker-b");
    let mut created = Vec::new();
    for class in [
        RecoveryClass::Resume,
        RecoveryClass::WebhookWait,
        RecoveryClass::InterruptedIndeterminate,
    ] {
        let task = a.create(draft(class)).await.unwrap().snapshot;
        a.claim(&task.task_id.to_string(), Duration::from_secs(30))
            .await
            .unwrap();
        expire(&a, &task).await;
        created.push(task);
    }
    let report = b.recover().await.unwrap();
    assert!(report.provider_waiting.is_empty());
    assert_eq!(report.resumable[0].task_id, created[0].task_id);
    assert_eq!(report.resumable[0].status, TaskStatus::Queued);
    assert_eq!(report.webhook_waiting[0].task_id, created[1].task_id);
    assert_eq!(report.webhook_waiting[0].status, TaskStatus::Waiting);
    assert_eq!(report.failed_indeterminate[0].task_id, created[2].task_id);
    assert_eq!(report.failed_indeterminate[0].status, TaskStatus::Failed);
    assert_eq!(
        report.failed_indeterminate[0].error.as_ref().unwrap().code,
        "interrupted_indeterminate"
    );
}

#[tokio::test]
async fn additive_schema_expansion_preserves_existing_tasks_and_rejects_early_admission() {
    let db = TestDb::new().await;
    db.a.client().query("DEFINE FIELD OVERWRITE recovery_class ON TABLE task TYPE 'resume' | 'webhook_wait' | 'interrupted_indeterminate';").await.unwrap().check().unwrap();
    let runtime = TaskRuntime::new(db.a.clone(), "computers-test", "worker-a");
    let mut previous = Vec::new();
    for class in [
        RecoveryClass::Resume,
        RecoveryClass::WebhookWait,
        RecoveryClass::InterruptedIndeterminate,
    ] {
        previous.push(runtime.create(draft(class)).await.unwrap().snapshot);
    }
    let rejected = draft(RecoveryClass::ProviderWait);
    let id = rejected.task_id;
    assert!(runtime.create(rejected).await.is_err());
    assert!(runtime.get(&id.to_string()).await.unwrap().is_none());
    db.a.client()
        .query(include_str!(
            "../../store/migrations/0052_provider_wait.surql"
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    for original in previous {
        let seen = current(&runtime, &original).await;
        assert_eq!(seen.recovery_class, original.recovery_class);
        assert_eq!(seen.request, original.request);
        assert_eq!(seen.created_at, original.created_at);
        assert_eq!(seen.status, original.status);
    }
    assert_eq!(
        runtime
            .create(draft(RecoveryClass::ProviderWait))
            .await
            .unwrap()
            .snapshot
            .recovery_class,
        RecoveryClass::ProviderWait
    );
}

#[tokio::test]
async fn provider_cancel_without_a_lease_preserves_a_successful_cancellation_request() {
    let db = TestDb::new().await;
    let a = TaskRuntime::new(db.a.clone(), "computers-test", "worker-a");
    let b = TaskRuntime::new(db.b.clone(), "computers-test", "worker-b");
    let task = a
        .create(draft(RecoveryClass::ProviderWait))
        .await
        .unwrap()
        .snapshot;
    let id = task.task_id.to_string();
    let requested = a.cancel(&id).await.unwrap();
    assert_eq!(requested.status, TaskStatus::CancelRequested);
    assert!(requested.lease_owner.is_none());
    assert_eq!(
        b.cancel(&id).await.unwrap().cancel_requested_at,
        requested.cancel_requested_at
    );
    let recovered = b.recover().await.unwrap();
    assert_eq!(
        recovered.provider_waiting[0].status,
        TaskStatus::CancelRequested
    );
    assert!(recovered.cancelled.is_empty());
    b.claim_observation(&id, Duration::from_secs(30))
        .await
        .unwrap();
    assert_eq!(
        b.transition(&id, TaskTransition::Cancelled)
            .await
            .unwrap()
            .status,
        TaskStatus::Cancelled
    );
}
