use super::*;
use veoveo_task_runtime::{TaskSnapshot, TaskStatus};

#[tokio::test]
async fn result_shapes_survive_store_reads_events_and_authorized_reconnects() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let (db, writer) = runtime("writer").await;
        let reader = TaskRuntime::new(db.b.clone(), "integration-server", "reader");
        for payload in [
            json!(null),
            json!(false),
            json!(42),
            json!(-42),
            json!(2.5),
            json!(i64::MAX),
            json!(u64::MAX),
            json!({"nested":[u64::MAX]}),
            json!("value"),
            json!([]),
            json!([null, {"value":42}]),
            json!({}),
            json!({"value":42}),
            json!({"value":null}),
            json!({"payload":{"value":42}}),
        ] {
            let task = writer
                .create(draft("result-shape", RecoveryClass::Resume))
                .await
                .unwrap()
                .snapshot;
            let id = task.task_id;
            let pending = serde_json::to_value(&task).unwrap();
            assert!(pending.get("result").is_none());
            assert_eq!(
                serde_json::from_value::<TaskSnapshot>(pending)
                    .unwrap()
                    .result,
                None
            );
            let mut events = reader
                .live_updates_for(std::slice::from_ref(&id))
                .await
                .unwrap();
            assert_eq!(events.next().await.unwrap().unwrap().snapshot.result, None);
            writer.claim(id, Duration::from_secs(30)).await.unwrap();
            let completed = writer
                .transition(
                    id,
                    TaskTransition::Succeeded { result_uri: None,
                        message: "completed".into(),
                        result: payload.clone(),
                    },
                )
                .await
                .unwrap();
            assert_eq!(completed.result, Some(payload.clone()));
            assert_eq!(
                reader.get(id).await.unwrap().unwrap().result,
                Some(payload.clone())
            );
            assert_eq!(
                reader
                    .for_owner(&task.owner)
                    .get(task.task_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .result,
                Some(payload.clone())
            );
            assert_eq!(
                reader.await_payload_state(id).await.unwrap(),
                TaskPayloadState::Completed(payload.clone())
            );
            let json = serde_json::to_value(&completed).unwrap();
            assert_eq!(json.get("result"), Some(&payload));
            assert_eq!(
                serde_json::from_value::<TaskSnapshot>(json).unwrap().result,
                Some(payload.clone())
            );
            loop {
                let event = events.next().await.unwrap().unwrap();
                if event.snapshot.status == TaskStatus::Succeeded {
                    assert_eq!(event.snapshot.result, Some(payload.clone()));
                    break;
                }
            }
            let mut fresh = reader
                .for_owner(&task.owner)
                .subscribe(&[task.task_id])
                .await
                .unwrap()
                .updates;
            assert_eq!(
                fresh.next().await.unwrap().unwrap().snapshot.result,
                Some(payload.clone())
            );
            let mut response =
                db.b.client()
                    .query(include_str!("../queries/support/result_shape_cases/result_shapes_survive_store_reads_events_and_authorized_reconnects/statement_1.surql"))
                    .bind(("task", task_record_id(task.task_id)))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            let envelope: Option<veoveo_platform_store::OpenObject> = response.take(0).unwrap();
            assert_eq!(
                serde_json::to_value(envelope.unwrap()).unwrap(),
                json!({"payload":payload})
            );
        }
    })
    .await
    .expect("Task result shape qualification exceeded 90 seconds");
}

#[tokio::test]
async fn current_schema_rejects_incomplete_result_envelopes_without_mutating_tasks() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (db, runtime) = runtime("writer").await;
        let task = runtime
            .create(draft("result-shape", RecoveryClass::Resume))
            .await
            .unwrap()
            .snapshot;
        for invalid in [
            json!({}),
            json!({"value":42}),
            json!({"payload":42,"extra":true}),
        ] {
            assert!(
                db.b.client()
                    .query(include_str!("../queries/support/result_shape_cases/current_schema_rejects_incomplete_result_envelopes_without_mutating_tasks/statement_1.surql"))
                    .bind(("task", task_record_id(task.task_id)))
                    .bind(("result", invalid))
                    .await
                    .unwrap()
                    .check()
                    .is_err()
            );
            assert_eq!(
                runtime.get(task.task_id).await.unwrap().unwrap().result,
                None
            );
        }
    })
    .await
    .expect("Task result schema qualification exceeded 60 seconds");
}
