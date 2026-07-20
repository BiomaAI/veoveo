use std::collections::BTreeSet;
use std::time::Duration;

use futures::StreamExt;
use secrecy::SecretString;
use serde_json::json;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use veoveo_mcp_contract::{
    AccessSubject, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId, TenantId,
    WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
};
use veoveo_platform_store::{PlatformStore, StoreConfig, StoreCredentials, TaskStatus};
use veoveo_task_runtime::{
    CreateTask, PrincipalKind, RecoveryClass, TaskError, TaskFailure, TaskInputRequest, TaskOwner,
    TaskPayloadState, TaskRetentionPin, TaskRuntime, TaskTransition,
};

fn authority() -> InvocationAuthority {
    let principal = PrincipalId::new("integration-principal").unwrap();
    InvocationAuthority {
        work_context: WorkContextId::new("integration-mission").unwrap(),
        tenant: TenantId::new("integration-tenant").unwrap(),
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
        task_id: veoveo_task_runtime::TaskId::new(),
        owner: owner(),
        server: "integration-server".to_owned(),
        task_type: task_type.to_owned(),
        request: json!({"value": 7}),
        recovery_class,
        idempotency_key: None,
        ttl_ms: Some(60_000),
        poll_interval_ms: Some(100),
        retention_pins: BTreeSet::new(),
    }
}

async fn runtime(worker: &str) -> Option<TaskRuntime> {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return None;
    }
    let endpoint = std::env::var("VEOVEO_SURREAL_ENDPOINT")
        .unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let namespace = std::env::var("VEOVEO_SURREAL_NAMESPACE")
        .unwrap_or_else(|_| "veoveo_task_integration".to_owned());
    let username = std::env::var("VEOVEO_SURREAL_USERNAME").unwrap_or_else(|_| "root".to_owned());
    let password = std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let database = format!("task_runtime_{}", Uuid::now_v7().simple());
    let config = StoreConfig::builder(
        endpoint,
        namespace,
        database,
        StoreCredentials::root(username, SecretString::from(password)),
    )
    .migrate_on_connect(true)
    .build()
    .unwrap();
    let store = PlatformStore::connect(config).await.unwrap();
    Some(TaskRuntime::new(store, "integration-server", worker))
}

#[tokio::test]
async fn task_lifecycle_is_durable_atomic_and_idempotent() {
    let Some(runtime) = runtime("worker-a").await else {
        return;
    };
    let mut create = draft("forecast", RecoveryClass::Resume);
    create.idempotency_key = Some("same-request".to_owned());
    let first = runtime.create(create.clone()).await.unwrap();
    let duplicate = runtime.create(create).await.unwrap();
    assert!(first.created);
    assert!(!duplicate.created);
    assert_eq!(first.snapshot.task_id, duplicate.snapshot.task_id);
    assert_eq!(first.snapshot.task_id.as_uuid().get_version_num(), 7);

    let claimed = runtime
        .claim(&first.snapshot.task_id.to_string(), Duration::from_secs(30))
        .await
        .unwrap();
    assert_eq!(claimed.lease_owner, "worker-a");
    let completed = runtime
        .transition(
            &first.snapshot.task_id.to_string(),
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
            .await_payload_state(&first.snapshot.task_id.to_string())
            .await
            .unwrap(),
        TaskPayloadState::Completed(json!({"answer": 42}))
    );
    let outbox = runtime.platform_store().read_outbox(0, 100).await.unwrap();
    assert!(
        outbox.events.iter().any(|event| {
            event.aggregate_id == first.snapshot.task_id.to_string()
                && event.event_type == "task.succeeded"
        }),
        "outbox events: {:?}",
        outbox.events
    );
}

#[tokio::test]
async fn recovery_classes_and_leases_are_enforced() {
    let Some(runtime) = runtime("worker-a").await else {
        return;
    };
    let active = runtime
        .create(draft("active_forecast", RecoveryClass::Resume))
        .await
        .unwrap()
        .snapshot;
    runtime
        .claim(&active.task_id.to_string(), Duration::from_secs(30))
        .await
        .unwrap();
    let active_replica = TaskRuntime::new(
        runtime.platform_store().clone(),
        "integration-server",
        "worker-b",
    );
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
        .claim(&resumable.task_id.to_string(), Duration::from_millis(10))
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
        .claim(&mutating.task_id.to_string(), Duration::from_millis(10))
        .await
        .unwrap();

    let cancelling = runtime
        .create(draft("cancel_me", RecoveryClass::Resume))
        .await
        .unwrap()
        .snapshot;
    runtime
        .claim(&cancelling.task_id.to_string(), Duration::from_millis(10))
        .await
        .unwrap();
    runtime
        .cancel(&cancelling.task_id.to_string())
        .await
        .unwrap();

    let webhook = runtime
        .create(draft("media_generation", RecoveryClass::WebhookWait))
        .await
        .unwrap()
        .snapshot;
    runtime
        .claim(&webhook.task_id.to_string(), Duration::from_millis(10))
        .await
        .unwrap();
    runtime
        .transition(
            &webhook.task_id.to_string(),
            TaskTransition::Waiting {
                message: "waiting for provider webhook".to_owned(),
                progress: 0.1,
            },
        )
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;
    let restarted = TaskRuntime::new(
        runtime.platform_store().clone(),
        "integration-server",
        "worker-c",
    );
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
        restarted
            .payload_state(&mutating.task_id.to_string())
            .await
            .unwrap(),
        TaskPayloadState::Failed(TaskFailure::interrupted_indeterminate())
    );
}

#[tokio::test]
async fn replicas_use_revision_cas_and_emit_no_phantom_outbox_events() {
    let Some(first) = runtime("worker-a").await else {
        return;
    };
    let second = TaskRuntime::new(
        first.platform_store().clone(),
        "integration-server",
        "worker-b",
    );
    let task = first
        .create(draft("forecast", RecoveryClass::Resume))
        .await
        .unwrap()
        .snapshot;
    let task_id = task.task_id.to_string();

    let (left, right) = tokio::join!(
        first.claim(&task_id, Duration::from_secs(30)),
        second.claim(&task_id, Duration::from_secs(30))
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

    let page = first.platform_store().read_outbox(0, 1_000).await.unwrap();
    let before = page.next_sequence;
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
        .renew_lease(&task_id, Duration::from_secs(30))
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
    let after = first
        .platform_store()
        .read_outbox(before, 100)
        .await
        .unwrap();
    assert_eq!(
        after
            .events
            .iter()
            .filter(|event| event.event_type == "task.running")
            .count(),
        1,
        "failed CAS must not publish an outbox event"
    );
}

#[tokio::test]
async fn concurrent_idempotent_creates_converge_on_one_uuid_v7_task() {
    let Some(first) = runtime("worker-a").await else {
        return;
    };
    let second = TaskRuntime::new(
        first.platform_store().clone(),
        "integration-server",
        "worker-b",
    );
    let mut request = draft("forecast", RecoveryClass::Resume);
    request.idempotency_key = Some("concurrent-create".to_owned());
    let left_request = request.clone();
    request.task_id = veoveo_task_runtime::TaskId::new();
    let (left, right) = tokio::join!(first.create(left_request), second.create(request));
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(left.snapshot.task_id, right.snapshot.task_id);
    assert_ne!(left.created, right.created);
    assert_eq!(left.snapshot.task_id.as_uuid().get_version_num(), 7);
}

#[tokio::test]
async fn input_requests_are_lifetime_unique_and_responses_are_deduplicated() {
    let Some(first) = runtime("worker-a").await else {
        return;
    };
    let second = TaskRuntime::new(
        first.platform_store().clone(),
        "integration-server",
        "worker-b",
    );
    let task = first
        .create(draft("interactive", RecoveryClass::Resume))
        .await
        .unwrap()
        .snapshot;
    first
        .claim(&task.task_id.to_string(), Duration::from_secs(30))
        .await
        .unwrap();
    let request = TaskInputRequest {
        method: "elicitation/create".to_owned(),
        params: std::collections::BTreeMap::from([("message".to_owned(), json!("choose a value"))]),
    };
    first
        .request_input(&task.task_id.to_string(), "choice", request.clone())
        .await
        .unwrap();
    assert_eq!(
        first
            .outstanding_inputs(&task.task_id.to_string())
            .await
            .unwrap()
            .get("choice"),
        Some(&request)
    );
    assert!(matches!(
        first
            .request_input(&task.task_id.to_string(), "choice", request)
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
    let task_id = task.task_id.to_string();
    let (left, right) = tokio::join!(
        first.submit_input_responses(&task_id, responses.clone()),
        second.submit_input_responses(&task_id, responses),
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(left.accepted + right.accepted, 1);
    assert_eq!(left.ignored + right.ignored, 1);
    assert!(
        first
            .outstanding_inputs(&task.task_id.to_string())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn live_updates_deliver_durable_cross_replica_transitions() {
    let Some(first) = runtime("worker-a").await else {
        return;
    };
    let second = TaskRuntime::new(
        first.platform_store().clone(),
        "integration-server",
        "worker-b",
    );
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
        .claim(&task.task_id.to_string(), Duration::from_secs(30))
        .await
        .unwrap();
    let update = tokio::time::timeout(Duration::from_secs(2), updates.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(update.snapshot.task_id, task.task_id);
    assert_eq!(update.snapshot.lease_owner.as_deref(), Some("worker-b"));
}

#[tokio::test]
async fn durable_cursor_replays_every_transition_after_live_disconnect() {
    let Some(runtime) = runtime("worker-a").await else {
        return;
    };
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
        .claim(&task.task_id.to_string(), Duration::from_secs(30))
        .await
        .unwrap();
    runtime
        .transition(
            &task.task_id.to_string(),
            TaskTransition::Running {
                message: "forecasting".to_owned(),
                progress: 0.5,
            },
        )
        .await
        .unwrap();
    runtime
        .transition(
            &task.task_id.to_string(),
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
        vec![
            Some("claimed for execution"),
            Some("forecasting"),
            Some("complete")
        ]
    );
}

#[tokio::test]
async fn cancellation_is_durable_and_terminal_without_a_worker() {
    let Some(runtime) = runtime("worker-a").await else {
        return;
    };
    let task = runtime
        .create(draft("queued", RecoveryClass::Resume))
        .await
        .unwrap()
        .snapshot;
    let cancelled = runtime.cancel(&task.task_id.to_string()).await.unwrap();
    assert!(cancelled.is_terminal());
    assert_eq!(
        runtime
            .payload_state(&task.task_id.to_string())
            .await
            .unwrap(),
        TaskPayloadState::Cancelled
    );
}

#[tokio::test]
async fn cancellation_signals_an_active_worker_and_reaches_terminal_state() {
    let Some(runtime) = runtime("worker-a").await else {
        return;
    };
    let task = runtime
        .create(draft("queued", RecoveryClass::Resume))
        .await
        .unwrap()
        .snapshot;
    runtime
        .claim(&task.task_id.to_string(), Duration::from_secs(30))
        .await
        .unwrap();

    let cancellation = CancellationToken::new();
    let worker_cancellation = cancellation.clone();
    let worker_runtime = runtime.clone();
    let task_id = task.task_id.to_string();
    let worker_task_id = task_id.clone();
    let join = tokio::spawn(async move {
        worker_cancellation.cancelled().await;
        worker_runtime
            .transition(&worker_task_id, TaskTransition::Cancelled)
            .await
            .unwrap();
    });
    runtime
        .register_worker(&task_id, cancellation, join)
        .await
        .unwrap();

    let requested = runtime.cancel(&task_id).await.unwrap();
    assert_eq!(requested.status, TaskStatus::CancelRequested);
    let terminal = tokio::time::timeout(
        Duration::from_secs(5),
        runtime.await_payload_state(&task_id),
    )
    .await
    .expect("active worker cancellation timed out")
    .unwrap();
    assert_eq!(terminal, TaskPayloadState::Cancelled);
    assert!(runtime.cancel(&task_id).await.unwrap().is_terminal());
}

#[tokio::test]
async fn zero_length_leases_are_rejected() {
    let Some(runtime) = runtime("worker-a").await else {
        return;
    };
    let task = runtime
        .create(draft("queued", RecoveryClass::Resume))
        .await
        .unwrap()
        .snapshot;
    assert!(
        runtime
            .claim(&task.task_id.to_string(), Duration::ZERO)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn pruning_a_terminal_task_also_releases_its_idempotency_key() {
    let Some(runtime) = runtime("worker-a").await else {
        return;
    };
    let mut request = draft("short-lived", RecoveryClass::Resume);
    request.idempotency_key = Some("retained-key".to_owned());
    request.ttl_ms = Some(1);
    let pin = TaskRetentionPin::new("agent-episode:integration").unwrap();
    request.retention_pins.insert(pin.clone());

    let first = runtime.create(request.clone()).await.unwrap().snapshot;
    runtime
        .claim(&first.task_id.to_string(), Duration::from_secs(30))
        .await
        .unwrap();
    runtime
        .request_input(
            &first.task_id.to_string(),
            "retained-input",
            TaskInputRequest {
                method: "elicitation/create".to_owned(),
                params: Default::default(),
            },
        )
        .await
        .unwrap();
    runtime
        .transition(
            &first.task_id.to_string(),
            TaskTransition::Failed(TaskFailure::new("expected", "test completion")),
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;

    assert!(runtime.prune_expired().await.unwrap().is_empty());
    assert!(
        runtime
            .get(&first.task_id.to_string())
            .await
            .unwrap()
            .is_some()
    );
    let acknowledged = runtime
        .acknowledge_retention_pin(&first.task_id.to_string(), &pin)
        .await
        .unwrap();
    assert!(acknowledged.retention_pins.is_empty());
    assert!(
        runtime
            .acknowledge_retention_pin(&first.task_id.to_string(), &pin)
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
        .query("SELECT VALUE id FROM task_input WHERE task = $task;")
        .bind(("task", first.task_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let input_ids: Vec<surrealdb::types::RecordId> = response.take(0).unwrap();
    assert!(input_ids.is_empty());
    request.task_id = veoveo_task_runtime::TaskId::new();
    let second = runtime.create(request).await.unwrap();
    assert!(second.created);
    assert_ne!(second.snapshot.task_id, first.task_id);
}
