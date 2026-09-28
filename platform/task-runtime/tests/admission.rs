//! Native queued-Task admission: domain policy stays outside this shared guard.
#[path = "../../../testing/fixtures/store.rs"]
mod store;
use std::{collections::BTreeSet, time::Duration};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_platform_store::task_record_id;
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskRetentionPin, TaskRuntime};
use veoveo_types::TaskId;

fn draft() -> CreateTask {
    let owner: TaskOwner = serde_json::from_value(serde_json::json!({
        "principal_key": "pilot", "principal_kind": "service", "issuer": "https://native.example",
        "subject": "pilot", "profile": "operator", "tenant_key": "test", "data_labels": [],
        "authority": {"work_context": "operations", "tenant": "test", "membership": "owner",
            "policy_revision": "native", "output_policy": {"owner": {"kind": "principal", "id": "pilot"}},
            "provenance": {"mode": "automated"}}
    })).unwrap();
    CreateTask {
        task_id: TaskId::new(),
        owner,
        server: "admission-test".into(),
        task_type: const { veoveo_types::TaskTypeName::from_static("native") },
        request: serde_json::json!({"revision": 0}),
        recovery_class: RecoveryClass::InterruptedIndeterminate,
        idempotency_key: None,
        ttl_ms: Some(60_000),
        poll_interval_ms: None,
        retention_pins: BTreeSet::from([TaskRetentionPin::new("native:admission").unwrap()]),
    }
}

#[tokio::test]
async fn queued_snapshot_guard_rejects_claim_cancel_and_changed_input_or_pins() {
    let db = store::TestDb::new().await;
    tokio::time::timeout(Duration::from_secs(90), async {
        let runtime = TaskRuntime::new(db.a.clone(), "admission-test", "worker");
        db.a.client()
            .query("DEFINE TABLE admission_probe SCHEMALESS;")
            .await
            .unwrap()
            .check()
            .unwrap();
        for changed in 0..5 {
            let task = runtime.create(draft()).await.unwrap().snapshot;
            let id = task.task_id.to_string();
            match changed {
                0 => {
                    runtime.claim(&id, Duration::from_secs(60)).await.unwrap();
                }
                1 => {
                    runtime.cancel(&id).await.unwrap();
                }
                2 => {
                    runtime
                        .acknowledge_retention_pin(
                            &id,
                            &TaskRetentionPin::new("native:admission").unwrap(),
                        )
                        .await
                        .unwrap();
                }
                3 => {
                    db.b.client()
                        .query("UPDATE ONLY $task SET request.input.revision = 1;")
                        .bind(("task", task_record_id(task.task_id)))
                        .await
                        .unwrap()
                        .check()
                        .unwrap();
                }
                4 => {
                    db.b.client()
                        .query("UPDATE ONLY $task SET request.owner.principal_key = 'different';")
                        .bind(("task", task_record_id(task.task_id)))
                        .await
                        .unwrap()
                        .check()
                        .unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                runtime
                    .commit_admission(
                        &task,
                        "CREATE ONLY $probe CONTENT {accepted: true} RETURN NONE;",
                        vec![("probe", RecordId::new("admission_probe", id).into_value()),]
                    )
                    .await
                    .is_err(),
                "changed snapshot {changed} admitted"
            );
        }
        let mut response =
            db.b.client()
                .query("SELECT VALUE id FROM admission_probe;")
                .await
                .unwrap()
                .check()
                .unwrap();
        assert!(response.take::<Vec<RecordId>>(0).unwrap().is_empty());
        let task = runtime.create(draft()).await.unwrap().snapshot;
        assert!(
            runtime
                .commit_admission(
                    &task,
                    "RETURN NONE;",
                    vec![("_admission_task", task_record_id(task.task_id).into_value())]
                )
                .await
                .is_err()
        );
        let foreign = TaskRuntime::new(db.b.clone(), "different-server", "worker");
        assert!(
            foreign
                .commit_admission(&task, "RETURN NONE;", vec![])
                .await
                .is_err()
        );
    })
    .await
    .expect("queued Task guard qualification exceeded 90 seconds");
}

#[tokio::test]
async fn domain_admission_commits_atomically_without_changing_task_status_or_profile() {
    let db = store::TestDb::with_backend(store::StoreBackend::RocksDb).await;
    tokio::time::timeout(Duration::from_secs(90), async {
        let runtime = TaskRuntime::new(db.a.clone(), "admission-test", "worker");
        db.a.client()
            .query("DEFINE TABLE admission_probe SCHEMALESS;")
            .await
            .unwrap()
            .check()
            .unwrap();
        for class in [
            RecoveryClass::Resume,
            RecoveryClass::WebhookWait,
            RecoveryClass::ProviderWait,
            RecoveryClass::InterruptedIndeterminate,
        ] {
            let mut input = draft();
            input.recovery_class = class;
            let task = runtime.create(input).await.unwrap().snapshot;
            let probe = RecordId::new("admission_probe", task.task_id.to_string());
            runtime
                .commit_admission(
                    &task,
                    "CREATE ONLY $probe CONTENT {task: $task, committed: true} RETURN NONE;",
                    vec![
                        ("probe", probe.clone().into_value()),
                        ("task", task_record_id(task.task_id).into_value()),
                    ],
                )
                .await
                .unwrap();
            assert_eq!(
                runtime
                    .get(&task.task_id.to_string())
                    .await
                    .unwrap()
                    .unwrap(),
                task
            );
            assert!(
                runtime
                    .commit_admission(
                        &task,
                        "DELETE ONLY $probe; THROW 'native domain rollback';",
                        vec![("probe", probe.clone().into_value()),]
                    )
                    .await
                    .is_err()
            );
            let mut response =
                db.b.client()
                    .query("SELECT VALUE committed FROM ONLY $probe;")
                    .bind(("probe", probe))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            assert_eq!(response.take::<Option<bool>>(0).unwrap(), Some(true));
            assert_eq!(
                runtime
                    .get(&task.task_id.to_string())
                    .await
                    .unwrap()
                    .unwrap(),
                task
            );
        }
    })
    .await
    .expect("domain admission atomicity qualification exceeded 90 seconds");
}
