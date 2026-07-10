use std::collections::BTreeSet;
use std::time::Duration;

use secrecy::SecretString;
use serde_json::json;
use uuid::Uuid;
use veoveo_agent_runtime::{
    AgentInstanceId, AgentRuntime, AgentSpec, DEFAULT_CLAIM_LEASE, EpisodeCompletion, NewWake,
    json_object,
};
use veoveo_platform_store::{
    AgentEpisodeState, AgentTaskRecord, OpenObject, PlatformStore, PrincipalKind, StoreConfig,
    StoreCredentials, WakeKind,
};
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskRuntime, TaskTransition};

struct Fixture {
    root: PlatformStore,
    first: AgentRuntime,
    second: AgentRuntime,
    tasks: TaskRuntime,
}

async fn fixture() -> Option<Fixture> {
    if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
        return None;
    }
    let endpoint = std::env::var("VEOVEO_SURREAL_ENDPOINT")
        .or_else(|_| std::env::var("VEOVEO_SURREAL_URL"))
        .unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
    let root_user = std::env::var("VEOVEO_SURREAL_USERNAME")
        .or_else(|_| std::env::var("VEOVEO_SURREAL_USER"))
        .unwrap_or_else(|_| "root".to_owned());
    let root_password =
        std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
    let namespace = "veoveo_agent_integration";
    let database = format!("agent_runtime_{}", Uuid::now_v7().simple());
    let root = PlatformStore::connect(
        StoreConfig::builder(
            &endpoint,
            namespace,
            &database,
            StoreCredentials::root(root_user, SecretString::from(root_password)),
        )
        .migrate_on_connect(true)
        .build()
        .unwrap(),
    )
    .await
    .unwrap();
    let runtime_password = SecretString::from("agent-runtime-integration-password");
    root.replace_database_editor("agent_runtime", &runtime_password)
        .await
        .unwrap();
    let runtime_store = || async {
        PlatformStore::connect(
            StoreConfig::builder(
                &endpoint,
                namespace,
                &database,
                StoreCredentials::database("agent_runtime", runtime_password.clone()),
            )
            .build()
            .unwrap(),
        )
        .await
        .unwrap()
    };
    let spec = || AgentSpec {
        tenant_key: "integration".to_owned(),
        agent_key: "durability-agent".to_owned(),
        display_name: "Durability agent".to_owned(),
        profile: "integration".to_owned(),
        manifest: OpenObject::default(),
        memory_database: "memory.duckdb".to_owned(),
    };
    let first = AgentRuntime::register(runtime_store().await, spec(), AgentInstanceId::new())
        .await
        .unwrap();
    let second = AgentRuntime::register(runtime_store().await, spec(), AgentInstanceId::new())
        .await
        .unwrap();
    let tasks = TaskRuntime::new(root.clone(), "integration-server", "integration-worker");
    Some(Fixture {
        root,
        first,
        second,
        tasks,
    })
}

#[tokio::test]
async fn two_replicas_fence_claims_and_recover_expired_work() {
    let Some(fixture) = fixture().await else {
        return;
    };
    fixture
        .first
        .acquire_lease(Duration::from_millis(150))
        .await
        .unwrap()
        .expect("first lease");
    assert!(
        fixture
            .second
            .acquire_lease(Duration::from_secs(30))
            .await
            .unwrap()
            .is_none()
    );

    let wake = NewWake::now(
        WakeKind::Timer,
        Some("heartbeat".to_owned()),
        OpenObject::default(),
    );
    let wake_id = wake.wake_id;
    assert_eq!(wake_id.as_uuid().get_version_num(), 7);
    fixture.first.enqueue_wake(wake).await.unwrap();
    let claimed = fixture
        .first
        .claim_wakes(10, Duration::from_millis(50))
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);

    tokio::time::sleep(Duration::from_millis(200)).await;
    fixture
        .second
        .acquire_lease(Duration::from_secs(30))
        .await
        .unwrap()
        .expect("recovered lease");
    let reclaimed = fixture
        .second
        .claim_wakes(10, DEFAULT_CLAIM_LEASE)
        .await
        .unwrap();
    assert_eq!(reclaimed.len(), 1);
    assert_eq!(reclaimed[0].wake_id, wake_id);
    assert!(reclaimed[0].attempts >= 2);
    let outbox = fixture.root.read_outbox(0, 100).await.unwrap();
    assert!(
        outbox
            .events
            .iter()
            .any(|event| event.event_type == "wake.claim_recovered")
    );
}

#[tokio::test]
async fn result_wake_consumption_atomically_releases_task_retention_pin() {
    let Some(fixture) = fixture().await else {
        return;
    };
    fixture
        .first
        .acquire_lease(Duration::from_secs(30))
        .await
        .unwrap()
        .expect("lease");
    let episode = fixture.first.start_episode("integration").await.unwrap();
    let owner = TaskOwner {
        principal_key: "agent:durability-agent".to_owned(),
        principal_kind: PrincipalKind::Service,
        issuer: "veoveo://agent-runtime".to_owned(),
        subject: "durability-agent".to_owned(),
        profile: "integration".to_owned(),
        tenant_key: Some("integration".to_owned()),
        data_labels: BTreeSet::new(),
    };
    let task = fixture
        .tasks
        .create(CreateTask {
            task_id: veoveo_task_runtime::TaskId::new(),
            owner,
            server: "integration-server".to_owned(),
            task_type: "durability".to_owned(),
            request: json!({"work": true}),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(1),
            poll_interval_ms: None,
            retention_pins: BTreeSet::from([episode.retention_pin.clone()]),
        })
        .await
        .unwrap()
        .snapshot;
    fixture
        .tasks
        .claim(&task.task_id.to_string(), Duration::from_secs(30))
        .await
        .unwrap();
    fixture
        .tasks
        .transition(
            &task.task_id.to_string(),
            TaskTransition::Succeeded {
                message: "done".to_owned(),
                result: json!({"output": "done"}),
            },
        )
        .await
        .unwrap();
    assert_eq!(fixture.first.recover_pinned_tasks().await.unwrap(), 1);
    let claimed_task = fixture
        .first
        .claim_tasks(10, DEFAULT_CLAIM_LEASE)
        .await
        .unwrap()
        .pop()
        .expect("claimed task");
    let wake_id = fixture
        .first
        .resolve_task(
            &claimed_task,
            json_object(json!({"output": "done"}), "result").unwrap(),
            false,
        )
        .await
        .unwrap();
    let wake = fixture
        .first
        .claim_wakes(10, DEFAULT_CLAIM_LEASE)
        .await
        .unwrap();
    assert_eq!(wake[0].wake_id, wake_id);
    fixture
        .first
        .complete_episode(
            episode.episode_id,
            EpisodeCompletion {
                state: AgentEpisodeState::Completed,
                final_output: "consumed".to_owned(),
                summary: None,
                input_tokens: 0,
                output_tokens: 0,
                completion_calls: 0,
                tool_calls: 0,
                error: None,
            },
            &[wake_id],
        )
        .await
        .unwrap();

    let canonical = fixture
        .tasks
        .get(&task.task_id.to_string())
        .await
        .unwrap()
        .expect("task remains pinned through delivery");
    assert!(canonical.retention_pins.is_empty());
    let mut response = fixture
        .root
        .client()
        .query("SELECT * FROM agent_task WHERE task = $task;")
        .bind(("task", task.task_id.record_id()))
        .await
        .unwrap()
        .check()
        .unwrap();
    let deliveries: Vec<AgentTaskRecord> = response.take(0).unwrap();
    assert_eq!(deliveries.len(), 1);
    assert!(!deliveries[0].retention_pin_active);
    assert_eq!(
        deliveries[0].consumed_by_episode,
        Some(episode.episode_id.record_id())
    );

    tokio::time::sleep(Duration::from_millis(250)).await;
    assert_eq!(
        fixture.tasks.prune_expired().await.unwrap(),
        vec![task.task_id]
    );
}
