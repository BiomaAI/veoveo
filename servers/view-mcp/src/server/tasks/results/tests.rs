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
async fn checked_completion_survives_cross_replica_reads_and_reconnect() {
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
            .query(include_str!("../../../../queries/server/tasks/results/tests/checked_completion_survives_cross_replica_reads_and_reconnect.surql"))
            .bind(("task", task_record_id(id)))
            .bind(("result", corrupt))
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
            ("/structuredContent/viewId", json!("other-view")),
            ("/structuredContent/viewRevision", json!(2)),
            (
                "/structuredContent/compositionDigestSha256",
                json!(crate::contract::Sha256Digest::from_bytes(b"other")),
            ),
            ("/structuredContent/sceneLayer", json!("other-layer")),
            ("/structuredContent/styleId", json!("other-style")),
            ("/structuredContent/governedInputs", json!([])),
            ("/structuredContent/widthPx", json!(128)),
            ("/structuredContent/heightPx", json!(128)),
            (
                "/structuredContent/sceneTime",
                json!("2026-09-28T09:00:00Z"),
            ),
            (
                "/structuredContent/resolvedCamera/position/latitudeDegrees",
                json!(0),
            ),
            ("/structuredContent/byteLength", json!(1)),
            (
                "/structuredContent/outputDigestSha256",
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
        for (case, metadata) in
            crate::contract::test_support::retired_field_cases(&valid["structuredContent"])
        {
            let mut result = valid.clone();
            result["structuredContent"] = metadata;
            let id = create(&writer, &caller, &request).await;
            finish(&writer, id, result).await;
            let error = get_task(&reader, &caller, GetTaskParams::new(id.to_string()))
                .await
                .unwrap_err();
            assert!(
                error
                    .message
                    .starts_with("invalid stored View capture result:"),
                "{case}: {error}"
            );
            assert!(!error.message.contains(marker));
            let mut stream = subscribe_tasks(&reader, &caller, vec![id.to_string()])
                .await
                .unwrap();
            assert!(
                stream.updates.next().await.unwrap().is_err(),
                "baseline admitted {case}"
            );
        }
        // A self-consistent foreign snapshot still cannot borrow this Task's owner.
        let mut foreign = caller.clone();
        foreign.authority.work_context = WorkContextId::parse("other-context").unwrap();
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
        for (_assignment, statement) in [
("authority.context_key = 'another-context'", include_str!("../../../../queries/server/tasks/results/tests/sql_rejects_foreign_context_and_operation_before_completion_decoding_variant_1.surql")),
("task_type = 'another-operation'", include_str!("../../../../queries/server/tasks/results/tests/sql_rejects_foreign_context_and_operation_before_completion_decoding_variant_2.surql"))
] {
            let id = create(&writer, &caller, &request).await;
            finish(&writer, id, completed(&request)).await;
            db.a.client()
                .query(statement)
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

#[tokio::test]
async fn resumable_capture_settlement_preserves_failure_and_completed_frame() {
    tokio::time::timeout(Duration::from_secs(60), async {
        use tokio_util::sync::CancellationToken;
        use veoveo_task_runtime::{TaskError, TaskFailure, TaskStatus, TaskTransition};
        let db = fixture::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "view", "capture-writer");
        let reader = TaskRuntime::new(db.b.clone(), "view", "capture-canceller");
        let identity = identity();
        let request = capture(&identity);
        let policy = super::super::CANCELLATION_POLICY;
        for kind in 0..3 {
            let id = create(&writer, &identity, &request).await;
            let before = writer.get(id).await.unwrap().unwrap();
            let failure = TaskFailure::new("view_capture_failed", "known capture failure");
            let transition = if kind == 1 {
                TaskTransition::Failed(failure.clone())
            } else {
                veoveo_task_runtime::mcp_task_completion(
                    "Frame captured",
                    serde_json::from_value(completed(&request)).unwrap(),
                )
                .unwrap()
            };
            if kind < 2 {
                reader.cancel(id).await.unwrap();
                assert!(matches!(
                    writer
                        .transition_if_current(&before, transition.clone())
                        .await,
                    Err(TaskError::Conflict(_))
                ));
            }
            let after = writer
                .transition_resumable_if_current(&before, transition, policy, None)
                .await
                .unwrap();
            assert_eq!(after.owner, before.owner);
            assert_eq!(after.request, before.request);
            if kind == 0 {
                assert_eq!(after.status, TaskStatus::Cancelled);
                assert!(after.result.is_none() && after.result_uri.is_none());
            } else if kind == 1 {
                assert_eq!(after.status, TaskStatus::Failed);
                assert_eq!(after.error, Some(failure));
                assert!(after.result.is_none() && after.result_uri.is_none());
            } else {
                assert_eq!(after.status, TaskStatus::Succeeded);
                assert_eq!(after.result, Some(completed(&request)));
                assert_eq!(reader.cancel(id).await.unwrap(), after);
            }
        }
        let id = create(&writer, &identity, &request).await;
        let before = writer.get(id).await.unwrap().unwrap();
        let stop = CancellationToken::new();
        stop.cancel();
        let transition = veoveo_task_runtime::mcp_task_completion(
            "Frame captured",
            serde_json::from_value(completed(&request)).unwrap(),
        )
        .unwrap();
        assert_eq!(
            writer
                .transition_resumable_if_current(&before, transition, policy, Some(&stop))
                .await
                .unwrap(),
            before
        );
        assert!(reader.get(id).await.unwrap().unwrap().result.is_none());
    })
    .await
    .expect("View resumable settlement controls exceeded 60 seconds");
}

#[tokio::test]
async fn cancelled_initial_progress_stops_before_the_next_effect() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "view", "progress-writer");
        let canceller = TaskRuntime::new(db.b.clone(), "view", "progress-canceller");
        let identity = identity();
        let id = create(&writer, &identity, &capture(&identity)).await;
        canceller.cancel(id).await.unwrap();
        let effects = std::sync::atomic::AtomicUsize::new(0);
        let work = async {
            super::super::running_checkpoint(
                &writer,
                id,
                &tokio_util::sync::CancellationToken::new(),
            )
            .await?;
            effects.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok::<_, anyhow::Error>(())
        };
        assert!(work.await.is_err());
        assert_eq!(effects.load(std::sync::atomic::Ordering::SeqCst), 0);
        let current = writer.get(id).await.unwrap().unwrap();
        assert_eq!(current.status, veoveo_task_runtime::TaskStatus::Cancelled);
        assert!(current.result.is_none() && current.result_uri.is_none());
    })
    .await
    .expect("View progress control exceeded 60 seconds");
}
