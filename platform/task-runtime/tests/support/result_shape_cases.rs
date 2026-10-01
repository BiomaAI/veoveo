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
            let id = task.task_id.to_string();
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
            writer.claim(&id, Duration::from_secs(30)).await.unwrap();
            let completed = writer
                .transition(
                    &id,
                    TaskTransition::Succeeded {
                        message: "completed".into(),
                        result: payload.clone(),
                    },
                )
                .await
                .unwrap();
            assert_eq!(completed.result, Some(payload.clone()));
            assert_eq!(
                reader.get(&id).await.unwrap().unwrap().result,
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
                reader.await_payload_state(&id).await.unwrap(),
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
                    .query("SELECT VALUE result FROM ONLY $task;")
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
async fn schema_rejects_incomplete_envelopes_and_format_installation_over_tasks() {
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
                    .query("UPDATE ONLY $task SET result = $result;")
                    .bind(("task", task_record_id(task.task_id)))
                    .bind(("result", invalid))
                    .await
                    .unwrap()
                    .check()
                    .is_err()
            );
            assert_eq!(
                runtime
                    .get(&task.task_id.to_string())
                    .await
                    .unwrap()
                    .unwrap()
                    .result,
                None
            );
        }
        let migration = include_str!("../../../store/migrations/0098_task_result_envelope.surql");
        let error =
            db.b.client()
                .query(format!(
                    "BEGIN TRANSACTION; {migration} COMMIT TRANSACTION;"
                ))
                .await
                .unwrap()
                .check()
                .unwrap_err();
        assert!(error.to_string().contains("task_result_reset_required"));
        assert!(
            runtime
                .get(&task.task_id.to_string())
                .await
                .unwrap()
                .is_some()
        );
        db.b.client()
            .query("DELETE ONLY $task;")
            .bind(("task", task_record_id(task.task_id)))
            .await
            .unwrap()
            .check()
            .unwrap();
        db.b.client()
            .query(format!(
                "BEGIN TRANSACTION; {migration} COMMIT TRANSACTION;"
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
    })
    .await
    .expect("Task result schema qualification exceeded 60 seconds");
}
