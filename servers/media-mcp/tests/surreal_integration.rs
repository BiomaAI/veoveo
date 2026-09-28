#[path = "../../../testing/fixtures/store.rs"]
mod store;
use std::{collections::BTreeSet, time::Duration};

use chrono::{TimeDelta, Utc};
use futures::StreamExt;
use serde_json::json;
use veoveo_mcp_contract::{
    ArtifactWriteCapabilityId, ArtifactWriteCapabilitySecret, InvocationAuthority,
    IssuedArtifactWriteCapability, WorkContextMembershipLevel, WorkContextOutputPolicy,
};
use veoveo_media_mcp::{
    provider::Prediction,
    state::{MediaState, ProviderCancellationOutcome},
};
use veoveo_platform_store::{ProviderJobState, StoreError, TaskStatus};
use veoveo_task_runtime::{
    CreateTask, PrincipalKind, RecoveryClass, TaskFailure, TaskOwner, TaskRuntime,
};
use veoveo_types::TaskId;
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, TenantId, WorkContextId,
};

fn authority() -> InvocationAuthority {
    let principal = PrincipalId::new("https://idp.example.com#alice").unwrap();
    InvocationAuthority {
        work_context: WorkContextId::new("mission").unwrap(),
        tenant: TenantId::new("tenant-a").unwrap(),
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: PolicyVersion::new("r1").unwrap(),
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
    let db = store::TestDb::new().await;
    let first = TaskRuntime::new(db.a.clone(), "media", "media-replica-a");
    let second = TaskRuntime::new(db.b.clone(), "media", "media-replica-b");
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
        model: "fake/image".into(),
        outputs: Vec::new(),
        urls: None,
        status: status.into(),
        created_at: Some(Utc::now()),
        error: (status == "failed").then(|| "provider rejected input".into()),
        execution_time: Some(12.0),
        timings: None,
        input: None,
    }
}

async fn create_waiting_task(
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
            request: json!({"model": "fake/image", "input": {"prompt": "test"}}),
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
        .claim(&task_id.to_string(), Duration::from_secs(30))
        .await
        .unwrap();
    state
        .bind_submission_and_wait(
            runtime,
            &task_id.to_string(),
            &prediction(external_job_id, "processing"),
        )
        .await
        .unwrap();
    task_id
}

#[tokio::test]
async fn webhook_on_other_replica_is_idempotent_and_restart_recoverable() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let (_db, first, second, first_state, second_state) = fixture().await;
        let task_id = create_waiting_task(&first, &first_state, "provider-job-1").await;
        let restarted =
            TaskRuntime::new(first.platform_store().clone(), "media", "media-restarted");
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
            .receive_webhook(&second, &task_id.to_string(), "webhook-1", &terminal)
            .await
            .unwrap();
        assert!(receipt.inserted);
        let duplicate = first_state
            .receive_webhook(&first, &task_id.to_string(), "webhook-1", &terminal)
            .await
            .unwrap();
        assert!(!duplicate.inserted);
        let other_task_id = create_waiting_task(&first, &first_state, "provider-job-2").await;
        assert!(matches!(
            second_state
                .receive_webhook(&second, &other_task_id.to_string(), "webhook-1", &terminal,)
                .await,
            Err(StoreError::ArtifactWriteConflict { .. })
        ));
        let payload = json!({"value":{"artifacts": [], "prediction": {"id": terminal.id}, "count": u64::MAX}});
        let mut updates = first.live_updates_for(&[task_id.to_string()]).await.unwrap();
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
        let completed = first.get(&task_id.to_string()).await.unwrap().unwrap();
        assert_eq!(completed.status, TaskStatus::Succeeded);
        assert_eq!(completed.result, Some(payload.clone()));
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
                &task_id.to_string(),
                "webhook-2",
                &prediction("provider-job-1", "failed"),
            )
            .await
            .unwrap();
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
                .get(&task_id.to_string())
                .await
                .unwrap()
                .unwrap()
                .status,
            TaskStatus::Succeeded,
            "a reordered terminal webhook cannot replace the first terminal result"
        );
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
            .receive_webhook(&second, &task_id.to_string(), "webhook-failed", &terminal)
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
        let failed = first.get(&task_id.to_string()).await.unwrap().unwrap();
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
        let cancelled = first.cancel(&task_id.to_string()).await.unwrap();
        assert_eq!(cancelled.status, TaskStatus::Cancelled);
        assert!(cancelled.result.is_none());
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
        assert_eq!(provider_cancelled.state, ProviderJobState::Cancelled);
        assert_eq!(provider_cancelled.prediction.status, "cancelled");

        let terminal = prediction("provider-job-cancelled", "completed");
        let receipt = second_state
            .receive_webhook(
                &second,
                &task_id.to_string(),
                "webhook-after-cancellation",
                &terminal,
            )
            .await
            .unwrap();
        assert!(receipt.inserted);
        second_state
            .acknowledge_cancelled_event(&second, &receipt.event)
            .await
            .unwrap();

        let still_cancelled = first.get(&task_id.to_string()).await.unwrap().unwrap();
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

        let outbox = first.platform_store().read_outbox(0, 1_000).await.unwrap();
        for event_type in [
            "provider_job.cancel_requested",
            "provider_job.cancel_not_deleted",
            "provider_job.cancel_failed",
            "provider_job.cancel_accepted",
            "provider_job.webhook_received",
        ] {
            assert!(
                outbox
                    .events
                    .iter()
                    .any(|event| event.event_type == event_type),
                "missing durable cancellation event {event_type}"
            );
        }
    })
    .await
    .expect("Media lifecycle qualification exceeded 90 seconds");
}
