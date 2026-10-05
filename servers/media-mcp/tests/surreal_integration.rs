#[path = "support/generation.rs"]
mod generation_fixture;
#[path = "../../../testing/fixtures/store.rs"]
mod store;
use std::{collections::BTreeSet, time::Duration};

use chrono::{TimeDelta, Utc};
use futures::StreamExt;
use serde_json::json;
use veoveo_mcp_contract::{
    ArtifactWriteCapabilityId, ArtifactWriteCapabilitySecret, IssuedArtifactWriteCapability,
};
use veoveo_media_mcp::{
    contract::MediaGenerationResult,
    provider::Prediction,
    state::{MediaState, ProviderCancellationOutcome},
    task_lookup, task_results,
};
use veoveo_platform_store::{ProviderJobState, TaskStatus};
use veoveo_task_runtime::{
    CreateTask, DispatchPreparation, PrincipalKind, RecoveryClass, TaskFailure, TaskOwner,
    TaskRuntime, TaskTransition,
};
use veoveo_types::TaskId;
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, TenantId, WorkContextId,
};
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy};

fn authority() -> InvocationAuthority {
    let principal = PrincipalId::parse("https://idp.example.com#alice").unwrap();
    InvocationAuthority {
        work_context: WorkContextId::parse("mission").unwrap(),
        tenant: TenantId::parse("tenant-a").unwrap(),
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

async fn fixture() -> (
    store::TestDb,
    TaskRuntime,
    TaskRuntime,
    MediaState,
    MediaState,
) {
    let db = store::TestDb::with_modules(vec![
        veoveo_media_mcp::schema::module_setup(store::module_lanes::execution("media").unwrap())
            .unwrap(),
    ])
    .await;
    let first =
        task_lookup::bind(TaskRuntime::new(db.a.clone(), "media", "media-replica-a")).unwrap();
    let second =
        task_lookup::bind(TaskRuntime::new(db.b.clone(), "media", "media-replica-b")).unwrap();
    let first_state = MediaState::new(db.a.clone());
    let second_state = MediaState::new(db.b.clone());
    (db, first, second, first_state, second_state)
}

fn owner() -> TaskOwner {
    TaskOwner {
        principal_key: "https://idp.example.com#alice".into(),
        principal_kind: PrincipalKind::User,
        issuer: "https://idp.example.com".into(),
        subject: "alice".into(),
        profile: "operator".into(),
        tenant_key: Some("tenant-a".into()),
        data_labels: BTreeSet::from(["cui".into()]),
        authority: authority(),
    }
}

fn prediction(id: &str, status: &str) -> Prediction {
    Prediction {
        id: id.parse().unwrap(),
        model: "test/image".parse().unwrap(),
        outputs: if status == "completed" {
            vec![
                "https://provider.test/output-1".into(),
                "https://provider.test/output-2".into(),
            ]
        } else {
            Vec::new()
        },
        urls: None,
        status: status.into(),
        created_at: Some(Utc::now()),
        error: (status == "failed").then(|| "provider rejected input".into()),
        execution_time: Some(12.0),
        timings: None,
        input: None,
    }
}

async fn create_prepared_task(
    runtime: &TaskRuntime,
    state: &MediaState,
    external_job_id: &str,
) -> TaskId {
    let task_id = TaskId::new();
    let created = runtime
        .create(CreateTask {
            task_id,
            owner: owner(),
            server: "media".into(),
            task_type: const { veoveo_types::TaskTypeName::from_static("run") },
            request: json!({"model": "test/image", "input": {"prompt": "test"}}),
            recovery_class: RecoveryClass::WebhookWait,
            idempotency_key: None,
            ttl_ms: Some(60_000),
            poll_interval_ms: Some(100),
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap()
        .snapshot;
    let capability = IssuedArtifactWriteCapability {
        capability_id: ArtifactWriteCapabilityId::new(),
        secret: ArtifactWriteCapabilitySecret::new("s".repeat(32)).unwrap(),
        task_id: task_id.to_string(),
        expires_at: Utc::now() + TimeDelta::hours(1),
    };
    state
        .persist_task_context(&created, &capability)
        .await
        .unwrap();
    runtime
        .claim(task_id, Duration::from_secs(30))
        .await
        .unwrap();
    let running = runtime
        .transition(
            task_id,
            TaskTransition::Running {
                message: "preparing generation".into(),
                progress: 0.0,
            },
        )
        .await
        .unwrap();
    let dispatch =
        task_lookup::dispatch(&running, veoveo_types::Sha256Digest::from_bytes([7; 32])).unwrap();
    let prepared = runtime
        .webhooks("media".parse().unwrap())
        .prepare_dispatch(task_id, dispatch)
        .await
        .unwrap();
    assert_eq!(
        prepared,
        DispatchPreparation::NewlyPrepared,
        "only a new receipt authorizes fixture submission"
    );
    let prepared_task = runtime.get(task_id).await.unwrap().unwrap();
    let replay = task_lookup::dispatch(
        &prepared_task,
        veoveo_types::Sha256Digest::from_bytes([7; 32]),
    )
    .unwrap();
    assert_eq!(
        runtime
            .webhooks("media".parse().unwrap())
            .prepare_dispatch(task_id, replay)
            .await
            .unwrap(),
        DispatchPreparation::AlreadyPrepared
    );
    let wrong_binding = task_lookup::dispatch(
        &prepared_task,
        veoveo_types::Sha256Digest::from_bytes([8; 32]),
    )
    .unwrap();
    assert!(
        runtime
            .webhooks("media".parse().unwrap())
            .prepare_dispatch(task_id, wrong_binding)
            .await
            .is_err()
    );
    assert_eq!(
        runtime.get(task_id).await.unwrap().unwrap(),
        prepared_task,
        "mismatched dispatch receipt rolls back"
    );
    let mut wrong_model = prediction(external_job_id, "processing");
    wrong_model.model = "other/image".parse().unwrap();
    assert!(
        state
            .bind_submission_and_wait(runtime, task_id, &wrong_model)
            .await
            .is_err()
    );
    let payload = serde_json::to_value(prediction(external_job_id, "processing")).unwrap();
    let payload = veoveo_platform_store::OpenObject::new(
        payload.as_object().unwrap().clone().into_iter().collect(),
    );
    assert!(
        runtime
            .webhooks("foreign".parse().unwrap())
            .bind_submission(
                task_id,
                veoveo_platform_store::ProviderJobKey::parse(external_job_id).unwrap(),
                payload,
                "foreign binding".into()
            )
            .await
            .is_err()
    );
    assert_eq!(
        runtime.get(task_id).await.unwrap().unwrap(),
        prepared_task,
        "wrong model/provider binding does not mutate the Task"
    );
    let mut response = runtime
        .platform_store()
        .client()
        .query(include_str!(
            "queries/surreal_integration/jobs_for_task.surql"
        ))
        .bind(("task", veoveo_platform_store::task_record_id(task_id)))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        response
            .take::<Vec<veoveo_platform_store::RecordId>>(0)
            .unwrap()
            .is_empty(),
        "failed binding leaves no foreign journal rows"
    );

    task_id
}

async fn create_waiting_task(
    runtime: &TaskRuntime,
    state: &MediaState,
    external_job_id: &str,
) -> TaskId {
    let task_id = create_prepared_task(runtime, state, external_job_id).await;
    state
        .bind_submission_and_wait(runtime, task_id, &prediction(external_job_id, "processing"))
        .await
        .unwrap();
    task_id
}

#[tokio::test]
async fn concurrent_provider_associations_keep_one_dispatch_and_one_task_parent() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let (_db, first, second, first_state, second_state) = fixture().await;
        let task = create_prepared_task(&first, &first_state, "submission-race").await;
        let submission = prediction("submission-race", "processing");
        let callback = prediction("callback-race", "failed");
        let (submitted, received) = tokio::join!(
            first_state.bind_submission_and_wait(&first, task, &submission),
            second_state.receive_webhook(&second, task, "dispatch-race", &callback),
        );
        assert_ne!(
            submitted.is_ok(),
            received.is_ok(),
            "different provider IDs cannot both bind one dispatch"
        );
        let retained = first_state
            .provider_job_for_task(task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            retained.external_job_id,
            if submitted.is_ok() {
                submission.id
            } else {
                callback.id
            }
        );
        let left = create_prepared_task(&first, &first_state, "shared-provider-job").await;
        let right = create_prepared_task(&second, &second_state, "shared-provider-job").await;
        let terminal = prediction("shared-provider-job", "failed");
        let (a, b) = tokio::join!(
            first_state.receive_webhook(&first, left, "parent-race-a", &terminal),
            second_state.receive_webhook(&second, right, "parent-race-b", &terminal),
        );
        assert_ne!(
            a.is_ok(),
            b.is_ok(),
            "one provider operation cannot acquire two Task parents"
        );
        let (winner, loser, event) = match (a, b) {
            (Ok(receipt), Err(_)) => (left, right, receipt.event),
            (Err(_), Ok(receipt)) => (right, left, receipt.event),
            _ => unreachable!("one association transaction must win"),
        };
        if submitted.is_ok() {
            let terminal = prediction(retained.external_job_id.as_str(), "failed");
            second_state
                .receive_webhook(&second, task, "first-job-terminal", &terminal)
                .await
                .unwrap();
        }
        let pending = first_state.pending_events(10).await.unwrap();
        assert_eq!(
            pending.len(),
            2,
            "both distinct jobs retain their own pending receipt"
        );
        let pending_tasks = pending
            .iter()
            .map(|receipt| receipt.job.task_id)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            pending_tasks,
            [task, winner].into_iter().collect(),
            "pending reads dereference each event's exact job"
        );
        assert!(
            first_state
                .provider_job_for_task(loser)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            first.get(loser).await.unwrap().unwrap().status,
            TaskStatus::Waiting
        );
        first_state
            .complete_event(
                &first,
                &event,
                Err(veoveo_task_runtime::TaskFailure::new(
                    "provider_failed",
                    "fixture failure",
                )),
                "provider failed".into(),
            )
            .await
            .unwrap();
        assert_eq!(
            second.get(winner).await.unwrap().unwrap().status,
            TaskStatus::Failed
        );
    })
    .await
    .expect("concurrent provider association exceeded 90 seconds");
}

#[tokio::test]
async fn webhook_on_other_replica_is_idempotent_and_restart_recoverable() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let (_db, first, second, first_state, second_state) = fixture().await;
        let task_id = create_waiting_task(&first, &first_state, "provider-job-1").await;
        let restarted = task_lookup::bind(
            TaskRuntime::new(first.platform_store().clone(), "media", "media-restarted")
        ).unwrap();
        assert!(
            restarted
                .recover()
                .await
                .unwrap()
                .webhook_waiting
                .iter()
                .any(|task| task.task_id == task_id)
        );

        let terminal = prediction("provider-job-1", "completed");
        let receipt = second_state
            .receive_webhook(&second, task_id, "webhook-1", &terminal)
            .await
            .unwrap();
        assert!(receipt.inserted);
        let duplicate = first_state
            .receive_webhook(&first, task_id, "webhook-1", &terminal)
            .await
            .unwrap();
        assert!(!duplicate.inserted);
        let other_task_id = create_waiting_task(&first, &first_state, "provider-job-2").await;
        assert!(second_state.receive_webhook(&second, other_task_id, "webhook-1", &terminal)
            .await.is_err(), "an existing terminal delivery cannot bind a different Task");
        let mut generation = serde_json::to_value(generation_fixture::generation(task_id, terminal.id.clone())).unwrap();
        generation["prediction"]["timings"] = json!({"count": u64::MAX});
        let generation: MediaGenerationResult = serde_json::from_value(generation).unwrap();
        let expected_result_uri = veoveo_types::ResourceAddress::to_uri(generation.result_uri()).unwrap();
        let payload = serde_json::to_value(task_results::generation_tool_result(generation).unwrap()).unwrap();
        let mut updates = first.live_updates_for(&[task_id]).await.unwrap();
        updates.next().await.unwrap().unwrap();
        second_state
            .complete_event(
                &second,
                &receipt.event,
                Ok(payload.clone()),
                "completed by signed webhook".into(),
            )
            .await
            .unwrap();
        let completed = first.get(task_id).await.unwrap().unwrap();
        assert_eq!(completed.status, TaskStatus::Succeeded);
        assert_eq!(completed.result, Some(payload.clone()));
        assert_eq!(completed.result_uri, Some(expected_result_uri));
        let reloaded = restarted.get(task_id).await.unwrap().unwrap();
        assert_eq!(reloaded.result_uri, completed.result_uri);
        assert_eq!(reloaded.result, completed.result);
        assert_eq!(completed.result.as_ref().unwrap()["structuredContent"]["prediction"]["timings"]["count"], u64::MAX);
        let provider_pin = veoveo_task_runtime::TaskRetentionPin::new("provider:media:webhook").unwrap();
        assert!(completed.retention_pins.contains(&provider_pin), "terminal processing remains protected until billing");
        loop {
            let update = updates.next().await.unwrap().unwrap();
            if update.snapshot.status == TaskStatus::Succeeded {
                assert_eq!(update.snapshot.result, Some(payload.clone()));
                break;
            }
        }

        assert_eq!(
            first_state
                .task_context(&completed)
                .await
                .unwrap()
                .unwrap()
                .artifact_write_capability
                .task_id,
            task_id.to_string()
        );
        assert!(first_state.pending_events(10).await.unwrap().is_empty());

        let reordered = second_state
            .receive_webhook(
                &second,
                task_id,
                "webhook-2",
                &prediction("provider-job-1", "failed"),
            )
            .await
            .unwrap();
        assert!(!reordered.event.authoritative, "first terminal observation survives a reordered status");
        second_state
            .complete_event(
                &second,
                &reordered.event,
                Err(TaskFailure::new("provider_failed", "late failure")),
                "late failure".into(),
            )
            .await
            .unwrap();
        assert_eq!(
            first
                .get(task_id)
                .await
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Succeeded,
            "a reordered terminal webhook cannot replace the first terminal result"
        );
        let preserved = first_state.provider_job_for_task(task_id).await.unwrap().unwrap();
        assert_eq!(preserved.state, ProviderJobState::Succeeded);
        assert_eq!(preserved.prediction.status, "completed");
        assert_eq!(preserved.prediction.id, receipt.event.job.external_job_id);
        let terminal_event = receipt.event.event_id.record_id();
        assert!(first.platform_store().client().query(include_str!("queries/surreal_integration/delete_terminal_event.surql"))
            .bind(("event", terminal_event.clone())).await.unwrap().check().is_err(), "retained job protects its referenced terminal event");
        first.platform_store().client().query(include_str!("queries/surreal_integration/expire_task.surql"))
            .bind(("task", veoveo_platform_store::task_record_id(task_id))).await.unwrap().check().unwrap();
        assert!(!first.prune_expired().await.unwrap().contains(&task_id), "provider pin protects expired Task before billing release");
        first_state.record_usage(&completed, Some(&preserved), &veoveo_mcp_contract::UsageRecord {
            task_id: task_id.to_string(), provider_job_id: Some(preserved.external_job_id.to_string()),
            source_id: Some("native-billing".into()), model_id: "test/image".into(),
            kind: veoveo_mcp_contract::UsageKind::Actual, quantity: Some(1.0), unit: Some("run".into()),
            amount: None, currency: None, recorded_at: Utc::now(), metadata: json!({}),
        }).await.unwrap();
        let restarted_state = MediaState::new(first.platform_store().clone());
        assert!(restarted_state.billing_candidates(None).await.unwrap().jobs.iter().any(|job| job.task_id == task_id), "billed but pinned outcome is selected after restart");
        restarted.webhooks("media".parse().unwrap()).release_retention_after_billing(task_id).await.unwrap();
        assert!(!restarted_state.billing_candidates(None).await.unwrap().jobs.iter().any(|job| job.task_id == task_id));
        assert!(first.prune_expired().await.unwrap().contains(&task_id));
        assert!(first.get(task_id).await.unwrap().is_none());
        // Explicit fixture cleanup removes the referencing job before its event.
        first.platform_store().client().query(include_str!("queries/surreal_integration/delete_journal_fixture.surql"))
            .bind(("job", preserved.job_id.record_id())).await.unwrap().check().unwrap();
        first.platform_store().client().query(include_str!("queries/surreal_integration/delete_terminal_event.surql"))
            .bind(("event", terminal_event)).await.unwrap().check().unwrap();


    })
    .await
    .expect("Media lifecycle qualification exceeded 90 seconds");
}

#[tokio::test]
async fn signed_failure_webhook_completes_task_as_failed() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let (_db, first, second, first_state, second_state) = fixture().await;
        let task_id = create_waiting_task(&first, &first_state, "provider-job-failed").await;
        let terminal = prediction("provider-job-failed", "failed");
        let receipt = second_state
            .receive_webhook(&second, task_id, "webhook-failed", &terminal)
            .await
            .unwrap();
        second_state
            .complete_event(
                &second,
                &receipt.event,
                Err(TaskFailure::new(
                    "provider_failed",
                    "provider rejected input",
                )),
                "provider rejected input".into(),
            )
            .await
            .unwrap();
        let failed = first.get(task_id).await.unwrap().unwrap();
        assert_eq!(failed.status, TaskStatus::Failed);
        assert_eq!(failed.error.unwrap().code, "provider_failed");
    })
    .await
    .expect("Media lifecycle qualification exceeded 90 seconds");
}

#[tokio::test]
async fn cancellation_is_audited_and_late_webhook_cannot_replace_the_task_result() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let (_db, first, second, first_state, second_state) = fixture().await;
        let task_id = create_waiting_task(&first, &first_state, "provider-job-cancelled").await;
        let cancelled = first.cancel(task_id).await.unwrap();
        assert_eq!(cancelled.status, TaskStatus::Cancelled);
        assert!(cancelled.result.is_none());
        let replay =
            task_lookup::dispatch(&cancelled, veoveo_types::Sha256Digest::from_bytes([7; 32]))
                .unwrap();
        assert_eq!(
            first
                .webhooks("media".parse().unwrap())
                .prepare_dispatch(task_id, replay)
                .await
                .unwrap(),
            DispatchPreparation::AlreadyPrepared
        );
        assert_eq!(
            first.get(task_id).await.unwrap().unwrap(),
            cancelled,
            "prepared receipt replay after cancellation cannot authorize another send"
        );
        let waiting_job = first_state
            .provider_job_for_task(task_id)
            .await
            .unwrap()
            .unwrap();

        let cancel_requested = first_state
            .record_provider_cancellation(
                &cancelled,
                &waiting_job,
                ProviderCancellationOutcome::Requested,
            )
            .await
            .unwrap();
        assert_eq!(cancel_requested.state, ProviderJobState::CancelRequested);
        let not_deleted = first_state
            .record_provider_cancellation(
                &cancelled,
                &cancel_requested,
                ProviderCancellationOutcome::NotDeleted { deleted_count: 0 },
            )
            .await
            .unwrap();
        assert_eq!(not_deleted.state, ProviderJobState::CancelRequested);
        let failed_request = first_state
            .record_provider_cancellation(
                &cancelled,
                &not_deleted,
                ProviderCancellationOutcome::Failed {
                    error: "provider unavailable".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(failed_request.state, ProviderJobState::CancelRequested);
        let provider_cancelled = first_state
            .record_provider_cancellation(
                &cancelled,
                &failed_request,
                ProviderCancellationOutcome::Accepted { deleted_count: 1 },
            )
            .await
            .unwrap();
        assert_eq!(provider_cancelled.state, ProviderJobState::CancelRequested);
        assert_eq!(
            provider_cancelled.prediction.status, "processing",
            "local deletion acknowledgement does not rewrite the provider observation"
        );
        assert!(cancelled.retention_pins.contains(
            &veoveo_task_runtime::TaskRetentionPin::new("provider:media:webhook").unwrap()
        ));

        first
            .platform_store()
            .client()
            .query(include_str!(
                "queries/surreal_integration/expire_task.surql"
            ))
            .bind(("task", veoveo_platform_store::task_record_id(task_id)))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            !first.prune_expired().await.unwrap().contains(&task_id),
            "unresolved callback pin protects expired local cancellation"
        );

        let terminal = prediction("provider-job-cancelled", "completed");
        let receipt = second_state
            .receive_webhook(&second, task_id, "webhook-after-cancellation", &terminal)
            .await
            .unwrap();
        assert!(receipt.inserted);
        second_state
            .acknowledge_cancelled_event(&second, &receipt.event)
            .await
            .unwrap();

        let still_cancelled = first.get(task_id).await.unwrap().unwrap();
        assert_eq!(still_cancelled.status, TaskStatus::Cancelled);
        assert!(still_cancelled.result.is_none());
        assert!(still_cancelled.error.is_none());
        let actual_job = first_state
            .provider_job_for_task(task_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(actual_job.state, ProviderJobState::Succeeded);
        assert_eq!(actual_job.prediction.status, "completed");
        assert!(first_state.pending_events(10).await.unwrap().is_empty());

        let jobs = _db
            .committed(veoveo_platform_store::PlatformTable::ProviderJob)
            .await;
        assert!(jobs.iter().any(|row| row["state"] == "cancel_requested"));
        assert!(
            jobs.iter()
                .any(|row| row["cancellation_receipt"]["result"]["outcome"] == "accepted")
        );
        assert!(jobs.iter().any(|row| row["state"] == "succeeded"));
    })
    .await
    .expect("Media lifecycle qualification exceeded 90 seconds");
}
