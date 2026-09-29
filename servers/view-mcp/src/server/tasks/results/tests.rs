use super::super::test_support::{
    capture, completed, connection_switch, create, finish, fixture, identity,
};
use super::*;
use rmcp::model::TaskPayload;
use serde_json::{Value, json};
use std::time::Duration;
use veoveo_platform_store::task_record_id;
use veoveo_types::{TaskId, WorkContextId};

fn assert_completed(task: DetailedTask, expected: &Value) {
    let TaskPayload::Completed { result } = task.payload else {
        panic!("completed Task required");
    };
    assert_eq!(&Value::Object(result), expected);
}

#[tokio::test]
async fn checked_completion_survives_cross_replica_reads_and_eventless_reconnect() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let endpoint = db.a.config().endpoint();
        let switch = connection_switch::ConnectionSwitch::start(
            endpoint.host_str().unwrap().into(),
            endpoint.port_or_known_default().unwrap(),
        )
        .await;
        let writer = TaskRuntime::new(db.b.clone(), "view", "writer");
        let reader = TaskRuntime::new(db.connect_via(&switch.endpoint).await, "view", "reader");
        let caller = identity();
        let request = capture(&caller);
        let id = create(&writer, &caller, &request).await;
        let expected = completed(&request);
        let mut subscription = subscribe_tasks(&reader, &caller, vec![id.to_string()])
            .await
            .unwrap();
        assert_eq!(subscription.accepted_task_ids, vec![id.to_string()]);
        assert!(matches!(
            subscription.updates.next().await.unwrap().unwrap().payload,
            TaskPayload::Working
        ));
        switch.set_enabled(false).await;
        finish(&writer, id, expected.clone()).await;
        db.b.client()
            .query("DELETE outbox_event WHERE aggregate_type = 'task' RETURN NONE;")
            .await
            .unwrap()
            .check()
            .unwrap();
        switch.set_enabled(true).await;
        loop {
            let task = subscription.updates.next().await.unwrap().unwrap();
            if matches!(task.payload, TaskPayload::Completed { .. }) {
                assert_completed(task, &expected);
                break;
            }
        }
        assert_completed(
            get_task(&reader, &caller, GetTaskParams::new(id.to_string()))
                .await
                .unwrap()
                .task,
            &expected,
        );
        let mut fresh = subscribe_tasks(&reader, &caller, vec![id.to_string()])
            .await
            .unwrap();
        assert_completed(fresh.updates.next().await.unwrap().unwrap(), &expected);

        let mut corrupt = expected;
        corrupt["content"][1]["data"] = json!(BASE64_STANDARD.encode(b"different capture bytes"));
        db.b.client()
            .query(
                "UPDATE ONLY $task SET result = { payload: $result } RETURN NONE;
                 CREATE outbox_event SET aggregate_type = 'task', aggregate_id = $id,
                 event_type = 'task.fixture', schema_version = 3,
                 payload = { snapshot: { server: 'view' } } RETURN NONE;",
            )
            .bind(("task", task_record_id(id)))
            .bind(("result", corrupt))
            .bind(("id", id.to_string()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            subscription
                .updates
                .next()
                .await
                .unwrap()
                .unwrap_err()
                .message
                .starts_with("invalid stored View capture result:")
        );
        assert!(
            get_task(&reader, &caller, GetTaskParams::new(id.to_string()))
                .await
                .is_err()
        );
    })
    .await
    .expect("View completion recovery exceeded 90 seconds");
}

#[tokio::test]
async fn malformed_selected_completions_fail_before_public_projection() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "view", "writer");
        let reader = TaskRuntime::new(db.b.clone(), "view", "reader");
        let caller = identity();
        let request = capture(&caller);
        let valid = completed(&request);
        let marker = "PRIVATE_CAPTURE_PAYLOAD";
        for (path, replacement) in [
            ("/structuredContent/view_id", json!("other-view")),
            ("/structuredContent/view_revision", json!(2)),
            (
                "/structuredContent/composition_digest_sha256",
                json!(crate::contract::Sha256Digest::from_bytes(b"other")),
            ),
            ("/structuredContent/scene_layer", json!("other-layer")),
            ("/structuredContent/style_id", json!("other-style")),
            ("/structuredContent/governed_inputs", json!([])),
            ("/structuredContent/width_px", json!(128)),
            ("/structuredContent/height_px", json!(128)),
            (
                "/structuredContent/scene_time",
                json!("2026-09-28T09:00:00Z"),
            ),
            (
                "/structuredContent/resolved_camera/position/latitude_degrees",
                json!(0),
            ),
            ("/structuredContent/byte_length", json!(1)),
            (
                "/structuredContent/output_digest_sha256",
                json!(crate::contract::Sha256Digest::from_bytes(b"other")),
            ),
            ("/structuredContent/attribution/lines", json!([])),
            ("/content/0/text", json!(marker)),
            ("/content/1/data", json!(marker)),
            ("/content/1/mimeType", json!("image/png")),
            ("/isError", json!(true)),
        ] {
            let mut result = valid.clone();
            *result
                .pointer_mut(path)
                .unwrap_or_else(|| panic!("fixture lacks {path}")) = replacement;
            let id = create(&writer, &caller, &request).await;
            finish(&writer, id, result).await;
            let error = get_task(&reader, &caller, GetTaskParams::new(id.to_string()))
                .await
                .unwrap_err();
            assert!(
                error
                    .message
                    .starts_with("invalid stored View capture result:"),
                "{path}: {error}"
            );
            assert!(!error.message.contains(marker));
            let mut stream = subscribe_tasks(&reader, &caller, vec![id.to_string()])
                .await
                .unwrap();
            assert!(
                stream.updates.next().await.unwrap().is_err(),
                "accepted {path} on baseline"
            );
        }
        // A self-consistent foreign snapshot still cannot borrow this Task's owner.
        let mut foreign = caller.clone();
        foreign.authority.work_context = WorkContextId::new("other-context").unwrap();
        let foreign_request = capture(&foreign);
        let id = create(&writer, &caller, &foreign_request).await;
        finish(&writer, id, completed(&foreign_request)).await;
        assert!(
            get_task(&reader, &caller, GetTaskParams::new(id.to_string()))
                .await
                .unwrap_err()
                .message
                .contains("capture owner disagrees")
        );
    })
    .await
    .expect("View completion admission exceeded 90 seconds");
}

#[tokio::test]
async fn sql_rejects_foreign_context_and_operation_before_completion_decoding() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "view", "writer");
        let reader = TaskRuntime::new(db.b.clone(), "view", "reader");
        let caller = identity();
        let request = capture(&caller);
        let visible = create(&writer, &caller, &request).await;
        let mut excluded = Vec::new();
        for assignment in [
            "authority.context_key = 'another-context'",
            "task_type = 'another-operation'",
        ] {
            let id = create(&writer, &caller, &request).await;
            finish(&writer, id, completed(&request)).await;
            db.a.client()
                .query(format!(
                    "UPDATE ONLY $task SET {assignment}, request.input = NONE RETURN NONE;"
                ))
                .bind(("task", task_record_id(id)))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert_eq!(
                get_task(&reader, &caller, GetTaskParams::new(id.to_string()))
                    .await
                    .unwrap_err()
                    .message,
                "unknown task id"
            );
            excluded.push(id.to_string());
        }
        excluded.push(visible.to_string());
        excluded.push("invalid-handle".into());
        let mut subscription = subscribe_tasks(&reader, &caller, excluded).await.unwrap();
        assert_eq!(subscription.accepted_task_ids, vec![visible.to_string()]);
        assert!(matches!(
            subscription.updates.next().await.unwrap().unwrap().payload,
            TaskPayload::Working
        ));
        let expected = completed(&request);
        finish(&writer, visible, expected.clone()).await;
        loop {
            let task = subscription.updates.next().await.unwrap().unwrap();
            assert_eq!(task.task.task_id, visible.to_string());
            if matches!(task.payload, TaskPayload::Completed { .. }) {
                assert_completed(task, &expected);
                break;
            }
        }
        assert!(
            subscribe_tasks(&reader, &caller, vec![TaskId::new().to_string(); 257])
                .await
                .is_err()
        );
    })
    .await
    .expect("View completion SQL selection exceeded 60 seconds");
}
