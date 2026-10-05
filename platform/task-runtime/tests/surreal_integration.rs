#[path = "../../../testing/fixtures/store.rs"]
mod fixture;
#[path = "support/result_shape_cases.rs"]
mod result_shape_cases;
#[path = "support/task_type_cases.rs"]
mod task_type_cases;
use std::collections::BTreeSet;
use std::time::Duration;
use veoveo_platform_store::task_record_id;

use futures::StreamExt;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use veoveo_platform_store::TaskStatus;
use veoveo_task_runtime::{
    CreateTask, PrincipalKind, RecoveryClass, TaskError, TaskFailure, TaskInputRequest, TaskOwner,
    TaskPayloadState, TaskRetentionPin, TaskRuntime, TaskTransition,
};
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, TenantId, WorkContextId,
};
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy};

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
        principal_kind: PrincipalKind::User,
        issuer: "https://issuer.integration.example".to_owned(),
        subject: "integration-subject".to_owned(),
        profile: "integration-profile".to_owned(),
        tenant_key: Some("integration-tenant".to_owned()),
        data_labels: BTreeSet::from(["internal".to_owned()]),
        authority: authority(),
    }
}

fn draft(task_type: &str, recovery_class: RecoveryClass) -> CreateTask {
    CreateTask {
        task_id: veoveo_types::TaskId::new(),
        owner: owner(),
        server: "integration-server".to_owned(),
        task_type: veoveo_types::TaskTypeName::new(task_type).unwrap(),
        request: json!({"value": 7}),
        recovery_class,
        idempotency_key: None,
        ttl_ms: Some(60_000),
        poll_interval_ms: Some(100),
        retention_pins: BTreeSet::new(),
    }
}

async fn runtime(worker: &str) -> (fixture::TestDb, TaskRuntime) {
    let db = fixture::TestDb::new().await;
    let runtime = TaskRuntime::new(db.a.clone(), "integration-server", worker);
    (db, runtime)
}

#[tokio::test]
async fn typed_native_admission_rejects_non_rfc_v7_without_creating_tasks() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (_db, runtime) = runtime("id-admission").await;
        for invalid in [
            "01983da0-0000-4000-8000-000000000001",
            "01983da0-0000-7000-0000-000000000001",
        ] {
            let id = veoveo_types::TaskId::from_uuid(invalid.parse().unwrap());
            let mut request = draft("forecast", RecoveryClass::Resume);
            request.task_id = id;
            assert!(matches!(
                runtime.create(request).await,
                Err(TaskError::InvalidRecord(_))
            ));
            assert!(matches!(
                runtime.get(id).await,
                Err(TaskError::InvalidRecord(_))
            ));
            assert!(matches!(
                runtime.claim(id, Duration::from_secs(30)).await,
                Err(TaskError::InvalidRecord(_))
            ));
            assert!(matches!(
                runtime.renew_lease(id, Duration::from_secs(30)).await,
                Err(TaskError::InvalidRecord(_))
            ));
            assert!(matches!(
                runtime.cancel(id).await,
                Err(TaskError::InvalidRecord(_))
            ));
        }
        assert!(runtime.list().await.unwrap().is_empty());
    })
    .await
    .expect("native identity admission exceeded 60 seconds");
}

#[tokio::test]
async fn task_lifecycle_is_durable_atomic_and_idempotent() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (db, runtime) = runtime("worker-a").await;
        let mut create = draft("forecast", RecoveryClass::Resume);
        create.idempotency_key = Some("same-request".to_owned());
        let first = runtime.create(create.clone()).await.unwrap();
        let duplicate = runtime.create(create).await.unwrap();
        assert!(first.created);
        assert!(!duplicate.created);
        assert_eq!(first.snapshot.task_id, duplicate.snapshot.task_id);
        assert_eq!(first.snapshot.task_id.as_uuid().get_version_num(), 7);

        let claimed = runtime
            .claim(first.snapshot.task_id, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(claimed.lease_owner, "worker-a");
        let completed = runtime
            .transition(first.snapshot.task_id,
                TaskTransition::Succeeded {
                    message: "done".to_owned(),
                    result: json!({"answer": 42}),
                },
            )
            .await
            .unwrap();
        assert!(completed.is_terminal());
        assert_eq!(
            runtime
                .await_payload_state(first.snapshot.task_id)
                .await
                .unwrap(),
            TaskPayloadState::Completed(json!({"answer": 42}))
        );
        let states = db.committed(veoveo_platform_store::PlatformTable::Task).await;
        assert!(states.iter().any(|row| row["status"] == "succeeded" && row["result"]["payload"]["answer"] == 42));
    })
    .await
    .expect("task_lifecycle_is_durable_atomic_and_idempotent exceeded 60 seconds");
}

#[tokio::test]
async fn recovery_classes_and_leases_are_enforced() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (db, runtime) = runtime("worker-a").await;
        let active = runtime
            .create(draft("active_forecast", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        runtime
            .claim(active.task_id, Duration::from_secs(30))
            .await
            .unwrap();
        let active_replica = TaskRuntime::new(db.b.clone(), "integration-server", "worker-b");
        let active_report = active_replica.recover().await.unwrap();
        assert!(
            !active_report
                .resumable
                .iter()
                .any(|task| task.task_id == active.task_id)
        );

        let resumable = runtime
            .create(draft("forecast", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        runtime
            .claim(resumable.task_id, Duration::from_millis(10))
            .await
            .unwrap();

        let mutating = runtime
            .create(draft(
                "duckdb_execute",
                RecoveryClass::InterruptedIndeterminate,
            ))
            .await
            .unwrap()
            .snapshot;
        runtime
            .claim(mutating.task_id, Duration::from_millis(10))
            .await
            .unwrap();

        let cancelling = runtime
            .create(draft("cancel_me", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        runtime
            .claim(cancelling.task_id, Duration::from_millis(10))
            .await
            .unwrap();
        runtime.cancel(cancelling.task_id).await.unwrap();

        let webhook = runtime
            .create(draft("media_generation", RecoveryClass::WebhookWait))
            .await
            .unwrap()
            .snapshot;
        runtime
            .claim(webhook.task_id, Duration::from_millis(10))
            .await
            .unwrap();
        runtime
            .transition(
                webhook.task_id,
                TaskTransition::Waiting {
                    message: "waiting for provider webhook".to_owned(),
                    progress: 0.1,
                },
            )
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;
        let restarted = TaskRuntime::new(db.b.clone(), "integration-server", "worker-c");
        let report = restarted.recover().await.unwrap();
        assert!(
            report
                .resumable
                .iter()
                .any(|task| task.task_id == resumable.task_id)
        );
        assert!(
            report
                .webhook_waiting
                .iter()
                .any(|task| task.task_id == webhook.task_id)
        );
        assert!(
            report
                .cancelled
                .iter()
                .any(|task| task.task_id == cancelling.task_id)
        );
        let failed = report
            .failed_indeterminate
            .iter()
            .find(|task| task.task_id == mutating.task_id)
            .unwrap();
        assert_eq!(
            failed.error.as_ref().map(|error| error.code.as_str()),
            Some("interrupted_indeterminate")
        );
        assert_eq!(
            restarted.payload_state(mutating.task_id).await.unwrap(),
            TaskPayloadState::Failed(TaskFailure::interrupted_indeterminate())
        );
    })
    .await
    .expect("recovery_classes_and_leases_are_enforced exceeded 60 seconds");
}

#[tokio::test]
async fn replicas_use_revision_cas_and_commit_only_accepted_transitions() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (db, first) = runtime("worker-a").await;
        let second = TaskRuntime::new(db.b.clone(), "integration-server", "worker-b");
        let task = first
            .create(draft("forecast", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        let task_id = task.task_id;

        let (left, right) = tokio::join!(
            first.claim(task_id, Duration::from_secs(30)),
            second.claim(task_id, Duration::from_secs(30))
        );
        assert_ne!(
            left.is_ok(),
            right.is_ok(),
            "exactly one replica must claim"
        );
        let (owner_runtime, other_runtime, running) = match (left, right) {
            (Ok(claimed), Err(_)) => (&first, &second, claimed.snapshot),
            (Err(_), Ok(claimed)) => (&second, &first, claimed.snapshot),
            _ => unreachable!("exactly one replica claimed the task"),
        };

        assert!(matches!(
            other_runtime
                .transition_if_current(
                    &running,
                    TaskTransition::Running {
                        message: "unauthorized replica".to_owned(),
                        progress: 0.1,
                    },
                )
                .await,
            Err(TaskError::LeaseHeld(_))
        ));
        let renewed = owner_runtime
            .renew_lease(task_id, Duration::from_secs(30))
            .await
            .unwrap();
        assert_eq!(renewed.updated_at, running.updated_at);
        assert!(renewed.lease_expires_at > running.lease_expires_at);
        let updated = owner_runtime
            .transition_if_current(
                &running,
                TaskTransition::Running {
                    message: "phase one".to_owned(),
                    progress: 0.25,
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.progress, 0.25);
        assert_eq!(updated.lease_expires_at, renewed.lease_expires_at);
        assert!(
            other_runtime
                .transition_if_current(
                    &running,
                    TaskTransition::Running {
                        message: "stale phase".to_owned(),
                        progress: 0.5,
                    },
                )
                .await
                .is_err()
        );
        let changes = db
            .committed(veoveo_platform_store::PlatformTable::Task)
            .await;
        assert_eq!(
            changes.iter().filter(|row| row["progress"] == 0.25).count(),
            1
        );
        assert!(
            !changes
                .iter()
                .any(|row| row["progress"] == 0.1 || row["progress"] == 0.5),
            "failed CAS committed a Task state"
        );
    })
    .await
    .expect("replicas_use_revision_cas_and_commit_only_accepted_transitions exceeded 60 seconds");
}

#[tokio::test]
async fn concurrent_idempotent_creates_converge_on_one_uuid_v7_task() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (db, first) = runtime("worker-a").await;
        let second = TaskRuntime::new(db.b.clone(), "integration-server", "worker-b");
        let mut request = draft("forecast", RecoveryClass::Resume);
        request.idempotency_key = Some("concurrent-create".to_owned());
        let left_request = request.clone();
        request.task_id = veoveo_types::TaskId::new();
        let (left, right) = tokio::join!(first.create(left_request), second.create(request));
        let left = left.unwrap();
        let right = right.unwrap();
        assert_eq!(left.snapshot.task_id, right.snapshot.task_id);
        assert_ne!(left.created, right.created);
        assert_eq!(left.snapshot.task_id.as_uuid().get_version_num(), 7);
    })
    .await
    .expect("concurrent_idempotent_creates_converge_on_one_uuid_v7_task exceeded 60 seconds");
}

#[tokio::test]
async fn input_requests_are_lifetime_unique_and_responses_are_deduplicated() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (db, first) = runtime("worker-a").await;
        let second = TaskRuntime::new(db.b.clone(), "integration-server", "worker-b");
        let task = first
            .create(draft("interactive", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        first
            .claim(task.task_id, Duration::from_secs(30))
            .await
            .unwrap();
        let request = TaskInputRequest {
            method: "elicitation/create".to_owned(),
            params: std::collections::BTreeMap::from([(
                "message".to_owned(),
                json!("choose a value"),
            )]),
        };
        first
            .request_input(task.task_id, "choice", request.clone())
            .await
            .unwrap();
        assert_eq!(
            first
                .outstanding_inputs(task.task_id)
                .await
                .unwrap()
                .get("choice"),
            Some(&request)
        );
        assert!(matches!(
            first
                .request_input(task.task_id, "choice", request)
                .await,
            Err(TaskError::DuplicateInputKey(key)) if key == "choice"
        ));

        let responses = std::collections::BTreeMap::from([(
            "choice".to_owned(),
            std::collections::BTreeMap::from([
                ("action".to_owned(), json!("accept")),
                ("content".to_owned(), json!({"value": 7})),
            ]),
        )]);
        let task_id = task.task_id;
        let (left, right) = tokio::join!(
            first.submit_input_responses(task_id, responses.clone()),
            second.submit_input_responses(task_id, responses),
        );
        let left = left.unwrap();
        let right = right.unwrap();
        assert_eq!(left.accepted + right.accepted, 1);
        assert_eq!(left.ignored + right.ignored, 1);
        assert!(
            first
                .outstanding_inputs(task.task_id)
                .await
                .unwrap()
                .is_empty()
        );
    })
    .await
    .expect(
        "input_requests_are_lifetime_unique_and_responses_are_deduplicated exceeded 60 seconds",
    );
}

#[tokio::test]
async fn live_updates_deliver_durable_cross_replica_transitions() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (db, first) = runtime("worker-a").await;
        let second = TaskRuntime::new(db.b.clone(), "integration-server", "worker-b");
        let task = first
            .create(draft("live", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        let mut updates = first.live_updates().await.unwrap();
        let baseline = updates.next().await.unwrap().unwrap();
        assert_eq!(baseline.snapshot.task_id, task.task_id);
        assert_eq!(baseline.snapshot.status, TaskStatus::Queued);
        second
            .claim(task.task_id, Duration::from_secs(30))
            .await
            .unwrap();
        let update = tokio::time::timeout(Duration::from_secs(2), updates.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(update.snapshot.task_id, task.task_id);
        assert_eq!(update.snapshot.lease_owner.as_deref(), Some("worker-b"));
    })
    .await
    .expect("live_updates_deliver_durable_cross_replica_transitions exceeded 60 seconds");
}

#[tokio::test]
async fn durable_cursor_replays_every_transition_after_live_disconnect() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (_db, runtime) = runtime("worker-a").await;
        let task = runtime
            .create(draft("reconnect", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        let mut first_stream = runtime.live_updates().await.unwrap();
        let baseline = first_stream.next().await.unwrap().unwrap();
        assert_eq!(baseline.snapshot.status, TaskStatus::Queued);
        let cursor = baseline.cursor;
        drop(first_stream);

        runtime
            .claim(task.task_id, Duration::from_secs(30))
            .await
            .unwrap();
        runtime
            .transition(
                task.task_id,
                TaskTransition::Running {
                    message: "forecasting".to_owned(),
                    progress: 0.5,
                },
            )
            .await
            .unwrap();
        runtime
            .transition(
                task.task_id,
                TaskTransition::Succeeded {
                    message: "complete".to_owned(),
                    result: json!({"answer": 42}),
                },
            )
            .await
            .unwrap();

        let mut resumed = runtime.live_updates_after(cursor).await.unwrap();
        let mut observed = Vec::new();
        for _ in 0..3 {
            observed.push(resumed.next().await.unwrap().unwrap().snapshot);
        }
        assert_eq!(
            observed
                .iter()
                .map(|snapshot| snapshot.status)
                .collect::<Vec<_>>(),
            vec![
                TaskStatus::Running,
                TaskStatus::Running,
                TaskStatus::Succeeded
            ]
        );
        assert_eq!(
            observed
                .iter()
                .map(|snapshot| snapshot.status_message.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("Running"), Some("forecasting"), Some("complete")]
        );
    })
    .await
    .expect("durable_cursor_replays_every_transition_after_live_disconnect exceeded 60 seconds");
}

#[tokio::test]
async fn cancellation_is_durable_and_terminal_without_a_worker() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (_db, runtime) = runtime("worker-a").await;
        let task = runtime
            .create(draft("queued", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        let cancelled = runtime.cancel(task.task_id).await.unwrap();
        assert!(cancelled.is_terminal());
        assert_eq!(
            runtime.payload_state(task.task_id).await.unwrap(),
            TaskPayloadState::Cancelled
        );
    })
    .await
    .expect("cancellation_is_durable_and_terminal_without_a_worker exceeded 60 seconds");
}

#[tokio::test]
async fn cancellation_signals_an_active_worker_and_reaches_terminal_state() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (_db, runtime) = runtime("worker-a").await;
        let task = runtime
            .create(draft("queued", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        runtime
            .claim(task.task_id, Duration::from_secs(30))
            .await
            .unwrap();

        let cancellation = CancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let worker_runtime = runtime.clone();
        let task_id = task.task_id;
        let worker_task_id = task_id;
        let join = tokio::spawn(async move {
            worker_cancellation.cancelled().await;
            worker_runtime
                .transition(worker_task_id, TaskTransition::Cancelled)
                .await
                .unwrap();
        });
        runtime
            .register_worker(task_id, cancellation, join)
            .await
            .unwrap();

        let requested = runtime.cancel(task_id).await.unwrap();
        assert_eq!(requested.status, TaskStatus::CancelRequested);
        let terminal =
            tokio::time::timeout(Duration::from_secs(5), runtime.await_payload_state(task_id))
                .await
                .expect("active worker cancellation timed out")
                .unwrap();
        assert_eq!(terminal, TaskPayloadState::Cancelled);
        assert!(runtime.cancel(task_id).await.unwrap().is_terminal());
    })
    .await
    .expect("cancellation_signals_an_active_worker_and_reaches_terminal_state exceeded 60 seconds");
}

#[tokio::test]
async fn zero_length_leases_are_rejected() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (_db, runtime) = runtime("worker-a").await;
        let task = runtime
            .create(draft("queued", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        assert!(runtime.claim(task.task_id, Duration::ZERO).await.is_err());
    })
    .await
    .expect("zero_length_leases_are_rejected exceeded 60 seconds");
}

#[tokio::test]
async fn pruning_a_terminal_task_also_releases_its_idempotency_key() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (_db, runtime) = runtime("worker-a").await;
        let mut request = draft("short-lived", RecoveryClass::Resume);
        request.idempotency_key = Some("retained-key".to_owned());
        request.ttl_ms = Some(1);
        let pin = TaskRetentionPin::new("agent-episode:integration").unwrap();
        request.retention_pins.insert(pin.clone());

        let first = runtime.create(request.clone()).await.unwrap().snapshot;
        runtime
            .claim(first.task_id, Duration::from_secs(30))
            .await
            .unwrap();
        runtime
            .request_input(first.task_id,
                "retained-input",
                TaskInputRequest {
                    method: "elicitation/create".to_owned(),
                    params: Default::default(),
                },
            )
            .await
            .unwrap();
        runtime
            .transition(first.task_id,
                TaskTransition::Failed(TaskFailure::new("expected", "test completion")),
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;

        assert!(runtime.prune_expired().await.unwrap().is_empty());
        assert!(
            runtime
                .get(first.task_id)
                .await
                .unwrap()
                .is_some()
        );
        let rolled_back = runtime.platform_store().client()
            .query(include_str!("queries/surreal_integration/pruning_a_terminal_task_also_releases_its_idempotency_key/statement_1.surql"))
            .bind(("task", task_record_id(first.task_id)))
            .await.unwrap().check();
        assert!(rolled_back.is_err());
        let mut children = runtime.platform_store().client()
            .query(include_str!("queries/surreal_integration/pruning_a_terminal_task_also_releases_its_idempotency_key/statement_2.surql"))
            .bind(("task", task_record_id(first.task_id)))
            .await.unwrap().check().unwrap();
        assert_eq!(children.take::<Vec<surrealdb::types::RecordId>>(0).unwrap().len(), 1);
        assert_eq!(children.take::<Vec<surrealdb::types::RecordId>>(1).unwrap().len(), 1);
        // An unrelated active Task and its idempotency claim outlive this prune.
        let mut active_request = draft("active-retained", RecoveryClass::Resume);
        active_request.idempotency_key = Some("active-key".into());
        let active = runtime.create(active_request.clone()).await.unwrap().snapshot;
        let acknowledged = runtime
            .acknowledge_retention_pin(first.task_id, &pin)
            .await
            .unwrap();
        assert!(acknowledged.retention_pins.is_empty());
        assert!(
            runtime
                .acknowledge_retention_pin(first.task_id, &pin)
                .await
                .unwrap()
                .retention_pins
                .is_empty()
        );

        let deleted = runtime.prune_expired().await.unwrap();
        assert_eq!(deleted, vec![first.task_id]);
        let mut response = runtime
            .platform_store()
            .client()
            .query(include_str!("queries/surreal_integration/pruning_a_terminal_task_also_releases_its_idempotency_key/statement_3.surql"))
            .bind(("task", task_record_id(first.task_id)))
            .await
            .unwrap()
            .check()
            .unwrap();
        let input_ids: Vec<surrealdb::types::RecordId> = response.take(0).unwrap();
        assert!(input_ids.is_empty());
        let active_retry = runtime.create(active_request).await.unwrap();
        assert!(!active_retry.created);
        assert_eq!(active_retry.snapshot.task_id, active.task_id);
        request.task_id = veoveo_types::TaskId::new();
        let second = runtime.create(request).await.unwrap();
        assert!(second.created);
        assert_ne!(second.snapshot.task_id, first.task_id);
    })
    .await
    .expect("pruning_a_terminal_task_also_releases_its_idempotency_key exceeded 60 seconds");
}

#[path = "support/task_storage_interop.rs"]
mod task_storage_interop;

#[tokio::test]
#[ignore = "SDK-driven: the Python fixture owns a fresh database and provides explicit exchange files"]
async fn sdk_task_storage_interop() -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(90), task_storage_interop::exchange())
        .await
        .map_err(|_| anyhow::anyhow!("Task interoperability fixture exceeded 90 seconds"))?
}
