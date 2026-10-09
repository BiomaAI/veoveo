use super::*;
use veoveo_task_runtime::{TaskPayloadState, TaskRuntime};
use veoveo_types::{
    AccessSubject, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId, TenantId,
    WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
};

fn authority() -> InvocationAuthority {
    let principal = PrincipalId::parse("integration-principal").unwrap();
    InvocationAuthority {
        work_context: WorkContextId::parse("integration-mission").unwrap(),
        tenant: TenantId::parse("integration-tenant").unwrap(),
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: PolicyVersion::parse("r1").unwrap(),
        output_policy: WorkContextOutputPolicy {
            owner: AccessSubject::Principal(principal.clone()),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: BTreeSet::new(),
        },
        provenance: InvocationProvenance::Direct {
            initiator: principal,
        },
    }
}

fn owner() -> TaskOwner {
    TaskOwner {
        principal_key: "integration-principal".to_owned(),
        principal_kind: veoveo_task_runtime::PrincipalKind::User,
        issuer: "https://issuer.integration.example".to_owned(),
        subject: "integration-subject".to_owned(),
        profile: "integration-profile".to_owned(),
        tenant_key: Some("integration-tenant".to_owned()),
        data_labels: BTreeSet::from(["internal".to_owned()]),
        authority: authority(),
    }
}

async fn running(runtime: &TaskRuntime) -> TaskSnapshot {
    let request = TimeTaskRequest::ValidateTimeline(
        crate::contract::ValidateTimelineRequestValue {
            points: vec![],
            constraints: vec![],
        }
        .build()
        .unwrap(),
    );
    let created = runtime
        .create(CreateTask {
            task_id: TaskId::new(),
            owner: owner(),
            server: SERVER_SLUG.into(),
            task_type: request.task_type(),
            request: serde_json::to_value(request).unwrap(),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(TASK_TTL_MS),
            poll_interval_ms: Some(TASK_POLL_INTERVAL_MS),
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap()
        .snapshot;
    runtime
        .claim(created.task_id, TASK_LEASE_DURATION)
        .await
        .unwrap();
    runtime
        .transition(
            created.task_id,
            TaskTransition::Running {
                message: "validating mission timeline".into(),
                progress: 0.05,
            },
        )
        .await
        .unwrap()
}

fn success() -> TaskTransition {
    TaskTransition::Succeeded {
        message: "Temporal calculation completed".into(),
        result: serde_json::json!({"calculation": "complete"}),
        result_uri: None,
    }
}

#[tokio::test]
async fn remote_cancellation_winning_final_time_settlement_is_terminal() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let executor = TaskRuntime::new(db.a.clone(), SERVER_SLUG, "time-executor");
        let remote = TaskRuntime::new(db.b.clone(), SERVER_SLUG, "time-canceller");
        let before = running(&executor).await;
        // Exact interleave: the executing worker has selected its completion CAS,
        // then another worker commits cancellation before that CAS executes.
        remote.cancel(before.task_id).await.unwrap();
        settlement::settle(&executor, &before, success(), &CancellationToken::new())
            .await
            .unwrap();
        let after = remote.get(before.task_id).await.unwrap().unwrap();
        assert_eq!(after.task_id, before.task_id);
        assert_eq!(after.owner, before.owner);
        assert_eq!(after.request, before.request);
        assert_eq!(
            executor.payload_state(before.task_id).await.unwrap(),
            TaskPayloadState::Cancelled
        );
        assert!(after.result.is_none());
    })
    .await
    .expect("Time cancellation settlement exceeded 60 seconds");
}

#[tokio::test]
async fn time_settlement_preserves_cancellation_completion_failure_and_execution_fences() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let executor = TaskRuntime::new(db.a.clone(), SERVER_SLUG, "time-executor");
        let remote = TaskRuntime::new(db.b.clone(), SERVER_SLUG, "time-canceller");

        // Cancellation committed on another runtime is visible without a token.
        let before = running(&executor).await;
        remote.cancel(before.task_id).await.unwrap();
        assert!(
            !settlement::checkpoint(&executor, before.task_id)
                .await
                .unwrap()
        );
        assert_eq!(
            remote.payload_state(before.task_id).await.unwrap(),
            TaskPayloadState::Cancelled
        );

        // The normal local-worker cancellation path reaches the same settlement.
        let before = running(&executor).await;
        let token = CancellationToken::new();
        let worker_token = token.clone();
        let (finished, done) = tokio::sync::oneshot::channel();
        let worker = executor.clone();
        let id = before.task_id;
        let join = tokio::spawn(async move {
            worker_token.cancelled().await;
            let result = settlement::checkpoint(&worker, id).await;
            let _ = finished.send(result);
        });
        executor
            .register_worker(id, token.clone(), join)
            .await
            .unwrap();
        executor.cancel(id).await.unwrap();
        assert!(token.is_cancelled());
        assert!(!done.await.unwrap().unwrap());
        assert_eq!(
            executor.payload_state(id).await.unwrap(),
            TaskPayloadState::Cancelled
        );

        // Completion that commits first is preserved by a later cancel request.
        let before = running(&executor).await;
        let completed =
            settlement::settle(&executor, &before, success(), &CancellationToken::new())
                .await
                .unwrap();
        assert_eq!(remote.cancel(before.task_id).await.unwrap(), completed);
        assert_eq!(
            settlement::settle(&executor, &before, success(), &CancellationToken::new())
                .await
                .unwrap(),
            completed
        );
        assert_eq!(
            completed.result,
            Some(serde_json::json!({"calculation": "complete"}))
        );

        // A real calculation error is not replaced by a fabricated success or
        // discarded merely because cancellation wins the completion CAS.
        let before = running(&executor).await;
        remote.cancel(before.task_id).await.unwrap();
        let failure = TaskFailure::new("temporal_calculation_failed", "civil time is ambiguous");
        let failed = settlement::settle(
            &executor,
            &before,
            TaskTransition::Failed(failure.clone()),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(failed.error, Some(failure));
        assert!(failed.result.is_none());
        assert_eq!(failed.status, veoveo_platform_store::TaskStatus::Failed);

        // Reconciliation cannot convert unrelated progress conflicts into success.
        let before = running(&executor).await;
        executor
            .transition(
                before.task_id,
                TaskTransition::Running {
                    message: "new current progress".into(),
                    progress: 0.5,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            settlement::settle(&executor, &before, success(), &CancellationToken::new()).await,
            Err(veoveo_task_runtime::TaskError::Conflict(_))
        ));
        assert_eq!(
            executor.payload_state(before.task_id).await.unwrap(),
            TaskPayloadState::Running
        );

        // Another worker cannot use cancellation settlement to bypass a live lease.
        let before = running(&executor).await;
        remote.cancel(before.task_id).await.unwrap();
        assert!(matches!(
            settlement::update(
                &remote,
                before.task_id,
                success(),
                &CancellationToken::new()
            )
            .await,
            Err(veoveo_task_runtime::TaskError::LeaseHeld(_))
        ));
        assert_eq!(
            executor.get(before.task_id).await.unwrap().unwrap().status,
            veoveo_platform_store::TaskStatus::CancelRequested
        );
        settlement::update(
            &executor,
            before.task_id,
            success(),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            remote.payload_state(before.task_id).await.unwrap(),
            TaskPayloadState::Cancelled
        );
    })
    .await
    .expect("Time settlement controls exceeded 60 seconds");
}

#[tokio::test]
async fn local_shutdown_token_stops_time_work_without_inventing_cancel_intent() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let executor = TaskRuntime::new(db.a.clone(), SERVER_SLUG, "time-shutdown");
        let before = running(&executor).await;
        let token = CancellationToken::new();
        assert!(
            settlement::continue_work(&executor, before.task_id, &token)
                .await
                .unwrap()
        );
        assert!(
            !settlement::local_stop(&executor, before.task_id, &token)
                .await
                .unwrap()
        );
        // Shutdown cancels the local token; it does not issue runtime.cancel.
        token.cancel();
        assert!(
            !settlement::continue_work(&executor, before.task_id, &token)
                .await
                .unwrap()
        );
        assert!(
            settlement::local_stop(&executor, before.task_id, &token)
                .await
                .unwrap()
        );
        let after = executor.get(before.task_id).await.unwrap().unwrap();
        assert_eq!(after, before);
        assert!(after.cancel_requested_at.is_none());
        assert!(after.result.is_none());
        assert_eq!(after.status, veoveo_platform_store::TaskStatus::Running);

        // A real durable cancel still settles even when that same token stopped.
        executor.cancel(before.task_id).await.unwrap();
        assert!(
            !settlement::local_stop(&executor, before.task_id, &token)
                .await
                .unwrap()
        );
        assert!(
            !settlement::continue_work(&executor, before.task_id, &token)
                .await
                .unwrap()
        );
        assert_eq!(
            executor.payload_state(before.task_id).await.unwrap(),
            TaskPayloadState::Cancelled
        );
    })
    .await
    .expect("Time local shutdown control exceeded 60 seconds");
}

#[tokio::test]
async fn local_stop_at_selected_snapshot_seams_never_dispatches_success() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = crate::test_database(crate::test_store::StoreBackend::Memory).await;
        let executor = TaskRuntime::new(db.a.clone(), SERVER_SLUG, "time-executor");
        let remote = TaskRuntime::new(db.b.clone(), SERVER_SLUG, "time-canceller");

        // The same selected-snapshot seam used after update's awaited read.
        let before = running(&executor).await;
        let selected = executor.get(before.task_id).await.unwrap().unwrap();
        let token = CancellationToken::new();
        token.cancel();
        settlement::settle(&executor, &selected, success(), &token)
            .await
            .unwrap();
        assert_eq!(remote.get(before.task_id).await.unwrap().unwrap(), before);
        settlement::update(&executor, before.task_id, success(), &token)
            .await
            .unwrap();
        assert_eq!(remote.get(before.task_id).await.unwrap().unwrap(), before);

        // Stopping against a stale selected snapshot must still observe a
        // durable cancel committed after selection, rather than strand it.
        let before = running(&executor).await;
        remote.cancel(before.task_id).await.unwrap();
        settlement::settle(&executor, &before, success(), &token)
            .await
            .unwrap();
        let after = remote.get(before.task_id).await.unwrap().unwrap();
        assert_eq!(after.status, veoveo_platform_store::TaskStatus::Cancelled);
        assert_eq!(after.request, before.request);
        assert_eq!(after.owner, before.owner);
        assert!(after.result.is_none());

        // Actual failed CAS and reread, then stop at the reconciliation
        // dispatch seam. Durable cancellation may settle; success may not.
        let before = running(&executor).await;
        remote.cancel(before.task_id).await.unwrap();
        assert!(matches!(
            executor.transition_if_current(&before, success()).await,
            Err(veoveo_task_runtime::TaskError::Conflict(_))
        ));
        let observed = executor.get(before.task_id).await.unwrap().unwrap();
        let token = CancellationToken::new();
        token.cancel();
        settlement::dispatch(&executor, &observed, TaskTransition::Cancelled, &token)
            .await
            .unwrap();
        let after = remote.get(before.task_id).await.unwrap().unwrap();
        assert_eq!(after.status, veoveo_platform_store::TaskStatus::Cancelled);
        assert!(after.result.is_none());
    })
    .await
    .expect("Time selected snapshot stop controls exceeded 60 seconds");
}
