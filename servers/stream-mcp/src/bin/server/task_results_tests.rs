use std::time::Duration;
use surrealdb::types::SurrealValue;

use futures::StreamExt;
use rmcp::model::{DetailedTask, GetTaskParams};
use serde_json::Value;
use veoveo_types::TaskId;

use crate::store_fixture as fixture;
use crate::test_support::{create, current_output, finish, owner};

use super::*;

#[test]
fn live_tool_handoffs_link_to_the_session_product() {
    use veoveo_stream_mcp::contract::*;
    let id: SessionId = "01983da0-0000-7000-8000-000000000001".parse().unwrap();
    let start = StartLiveSessionOutput::new(
        id,
        "traffic".parse().unwrap(),
        LiveStartDetails {
            ingress: LiveIngressView {
                transport: LiveTransport::RtpH264Udp,
                host: "stream-mcp".into(),
                port: 9001,
                payload_type: 97,
                clock_rate: 90_000,
                caps: "application/x-rtp".into(),
            },
            video: LiveVideoView {
                codec: "avc1.42e01f".into(),
                width: 640,
                height: 480,
                frame_rate: 30,
                expected_bitrate_bps: 4_000_000,
            },
            recording_output: None,
            started_at: "2026-09-28T00:00:00Z".into(),
        },
    );
    let stop = StopLiveSessionOutput {
        result_uri: start.result_uri(),
        lifecycle: LiveSessionLifecycle::Stopped,
        received_video_frames: 10,
        processed_frames: 10,
        recording_output: None,
        stopped_at: "2026-09-28T00:01:00Z".into(),
    };
    for (result, status) in [
        (live_started_result(start).unwrap(), "Live session started."),
        (live_stopped_result(stop).unwrap(), "Live session stopped."),
    ] {
        let [ContentBlock::Text(text), ContentBlock::ResourceLink(link)] =
            result.content.as_slice()
        else {
            panic!("one status and one product link required");
        };
        assert_eq!(text.text, status);
        assert_eq!(link.uri, SessionUri::new(id).to_string());
        assert_eq!(link.mime_type.as_deref(), Some("application/json"));
        assert_eq!(result.structured_content.unwrap()["resultUri"], link.uri);
    }
}

fn assert_handoff(task: DetailedTask, expected: &Value) {
    assert_eq!(task.task.status_message.as_deref(), Some(RUN_COMPLETED));
    let TaskPayload::Completed { result } = task.payload else {
        panic!("expected completed Task")
    };
    let result: CallToolResult = serde_json::from_value(Value::Object(result)).unwrap();
    assert_eq!(result.structured_content.as_ref(), Some(expected));
    let [ContentBlock::Text(status), ContentBlock::ResourceLink(link)] = result.content.as_slice()
    else {
        panic!("completion needs one status and one result link")
    };
    assert_eq!(status.text, RUN_COMPLETED);
    assert_eq!(link.uri, expected["resultUri"].as_str().unwrap());
    assert_eq!(
        link.mime_type.as_deref(),
        Some("application/vnd.veoveo.stream-results+json")
    );
}

#[tokio::test]
async fn current_results_survive_cross_replica_reads_and_listener_reconnects() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        fixture::module_lanes::install(
            &db.a,
            vec![
                veoveo_stream_mcp::schema::module_setup(
                    fixture::module_lanes::execution("stream").unwrap(),
                )
                .unwrap(),
            ],
        )
        .await
        .unwrap();
        let writer = veoveo_stream_mcp::task_lookup::bind(TaskRuntime::new(
            db.a.clone(),
            "stream",
            "result-writer",
        ))
        .unwrap();
        let reader = veoveo_stream_mcp::task_lookup::bind(TaskRuntime::new(
            db.b.clone(),
            "stream",
            "result-reader",
        ))
        .unwrap();
        let id = TaskId::new();
        create(&writer, owner(), id).await;
        let canonical: RunRecordingOutput = serde_json::from_value(current_output(id)).unwrap();
        let expected = serde_json::to_value(&canonical).unwrap();
        let stored = serde_json::to_value(recording_result(canonical).unwrap()).unwrap();
        let mut subscription = subscribe_tasks(&reader, owner(), vec![id.to_string()])
            .await
            .unwrap();
        assert_eq!(subscription.accepted_task_ids, vec![id.to_string()]);
        assert!(matches!(
            subscription.updates.next().await.unwrap().unwrap().payload,
            TaskPayload::Working
        ));
        finish(&writer, id, stored.clone()).await;
        let completed = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let task = subscription.updates.next().await.unwrap().unwrap();
                if matches!(task.payload, TaskPayload::Completed { .. }) {
                    break task;
                }
            }
        })
        .await
        .unwrap();
        assert_handoff(completed, &expected);
        let fetched = get_task(&reader, &owner(), GetTaskParams::new(id.to_string()))
            .await
            .unwrap();
        assert_handoff(fetched.task, &expected);
        // A new listener receives the stored current result after reconnect.
        let mut reconnected = subscribe_tasks(&reader, owner(), vec![id.to_string()])
            .await
            .unwrap();
        assert_handoff(
            reconnected.updates.next().await.unwrap().unwrap(),
            &expected,
        );
        let blocking = completed_payload(&reader, &owner(), RunId::try_from(id).unwrap())
            .await
            .unwrap();
        assert_eq!(blocking.structured_content.as_ref(), Some(&expected));
        let retained = reader.for_owner(&owner()).get(id).await.unwrap().unwrap();
        assert_eq!(
            retained.result_uri.as_ref().map(|uri| uri.as_str()),
            expected["resultUri"].as_str()
        );
        assert_eq!(retained.result, Some(stored));
        assert_eq!(
            serde_json::to_value(run_view(&retained).unwrap().output().unwrap()).unwrap(),
            expected
        );
    })
    .await
    .expect("Stream current result qualification exceeded 90 seconds");
}

#[tokio::test]
async fn task_delivery_rejects_corruption_and_denies_access_before_domain_decode() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        fixture::module_lanes::install(
            &db.a,
            vec![
                veoveo_stream_mcp::schema::module_setup(
                    fixture::module_lanes::execution("stream").unwrap(),
                )
                .unwrap(),
            ],
        )
        .await
        .unwrap();
        let writer = veoveo_stream_mcp::task_lookup::bind(TaskRuntime::new(
            db.a.clone(),
            "stream",
            "invalid-writer",
        ))
        .unwrap();
        let reader = veoveo_stream_mcp::task_lookup::bind(TaskRuntime::new(
            db.b.clone(),
            "stream",
            "invalid-reader",
        ))
        .unwrap();
        for corruption in [
            "wrong-pipeline",
            "wrong-task",
            "missing-product",
            "missing-result-uri",
            "wrong-link",
            "extra-link",
            "retired-product",
            "mixed-product",
            "conflicting-provenance",
        ] {
            let id = TaskId::new();
            create(&writer, owner(), id).await;
            let mut value = current_output(id);
            let mut result =
                recording_result(serde_json::from_value(value.clone()).unwrap()).unwrap();
            match corruption {
                "wrong-pipeline" => value["pipelineUri"] = "stream://pipeline/other".into(),
                "wrong-task" => {
                    value = current_output(TaskId::new());
                    result =
                        recording_result(serde_json::from_value(value.clone()).unwrap()).unwrap();
                }
                "missing-product" => value = Value::Null,
                "missing-result-uri" => {
                    value.as_object_mut().unwrap().remove("resultUri");
                }
                "wrong-link" => {
                    let ContentBlock::ResourceLink(link) = &mut result.content[1] else {
                        unreachable!()
                    };
                    link.uri = "stream://pipeline/other".into();
                }
                "extra-link" => result.content.push(result.content[1].clone()),
                "retired-product" => {
                    let old = value
                        .as_object_mut()
                        .unwrap()
                        .remove("resultsArtifact")
                        .unwrap();
                    value["results_artifact"] = old;
                }
                "mixed-product" => value["results_artifact"] = value["resultsArtifact"].clone(),
                "conflicting-provenance" => {
                    value["annotationsArtifact"]["metadata"]["provenance"]["run_id"] =
                        TaskId::new().to_string().into()
                }
                _ => unreachable!(),
            }
            result.structured_content = Some(value);
            crate::test_support::corrupt_result(&writer, id, serde_json::to_value(result).unwrap())
                .await;
            let error = get_task(&reader, &owner(), GetTaskParams::new(id.to_string()))
                .await
                .unwrap_err();
            assert!(
                error
                    .message
                    .contains("stored Stream output does not satisfy the current contract"),
                "{corruption}"
            );
            assert!(
                completed_payload(&reader, &owner(), RunId::try_from(id).unwrap())
                    .await
                    .is_err()
            );
            let mut subscription = subscribe_tasks(&reader, owner(), vec![id.to_string()])
                .await
                .unwrap();
            assert!(subscription.updates.next().await.unwrap().is_err());
            let mut denied = owner();
            denied.profile = "other-profile".into();
            let error = get_task(&reader, &denied, GetTaskParams::new(id.to_string()))
                .await
                .unwrap_err();
            assert_eq!(error.message, "unknown task id");
            assert!(
                subscribe_tasks(&reader, denied, vec![id.to_string()])
                    .await
                    .unwrap()
                    .accepted_task_ids
                    .is_empty()
            );
        }
    })
    .await
    .expect("Stream denied/corrupt result qualification exceeded 90 seconds");
}

#[tokio::test]
async fn explicit_tool_error_keeps_its_no_product_envelope() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        fixture::module_lanes::install(
            &db.a,
            vec![
                veoveo_stream_mcp::schema::module_setup(
                    fixture::module_lanes::execution("stream").unwrap(),
                )
                .unwrap(),
            ],
        )
        .await
        .unwrap();
        let tasks = veoveo_stream_mcp::task_lookup::bind(TaskRuntime::new(
            db.a.clone(),
            "stream",
            "tool-error-reader",
        ))
        .unwrap();
        let id = TaskId::new();
        create(&tasks, owner(), id).await;
        let stored = serde_json::to_value(CallToolResult::error(vec![ContentBlock::text(
            "Recording run failed.",
        )]))
        .unwrap();
        finish(&tasks, id, stored.clone()).await;
        let task = get_task(&tasks, &owner(), GetTaskParams::new(id.to_string()))
            .await
            .unwrap();
        let TaskPayload::Completed { result } = task.task.payload else {
            panic!("expected tool result")
        };
        assert_eq!(Value::Object(result), stored);
        #[derive(surrealdb::types::SurrealValue)]
        struct NoProduct {
            outcome: String,
            no_product: bool,
        }
        let mut row = tasks
            .platform_store()
            .client()
            .query(include_str!(
                "../../../queries/bin/server/task_results_tests/no_product_settlement.surql"
            ))
            .bind((
                "lookup",
                surrealdb::types::RecordId::new("stream_run", id.to_string()),
            ))
            .await
            .unwrap()
            .check()
            .unwrap();
        let row: NoProduct = row.take::<Option<NoProduct>>(0).unwrap().unwrap();
        assert_eq!(row.outcome, "tool_error");
        assert!(row.no_product);
        let artifacts = crate::index::complete(
            &tasks,
            &owner(),
            crate::index::CompletionDomain::Artifacts,
            "",
        )
        .await
        .unwrap();
        assert!(artifacts.values.is_empty());
    })
    .await
    .expect("Stream tool error qualification exceeded 60 seconds");
}

#[tokio::test]
async fn unrelated_malformed_operation_is_excluded_before_task_and_resource_decode() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        fixture::module_lanes::install(&db.a, vec![veoveo_stream_mcp::schema::module_setup(fixture::module_lanes::execution("stream").unwrap()).unwrap()]).await.unwrap();
        let writer = veoveo_stream_mcp::task_lookup::bind(TaskRuntime::new(db.a.clone(), "stream", "writer")).unwrap();
        let reader = veoveo_stream_mcp::task_lookup::bind(TaskRuntime::new(db.b.clone(), "stream", "reader")).unwrap();
        let id = TaskId::new();
        create(&writer, owner(), id).await;
        db.b.client().query(include_str!("../../../queries/bin/server/task_results_tests/unrelated_malformed_operation_is_excluded_before_task_and_resource_decode.surql"))
            .bind(("task", veoveo_platform_store::task_record_id(id))).await.unwrap().check().unwrap();
        assert!(reader.for_owner(&owner()).get(id).await.is_err());
        let error = get_task(&reader, &owner(), GetTaskParams::new(id.to_string())).await.unwrap_err();
        assert_eq!(error.message, "unknown task id");
        assert!(subscribe_tasks(&reader, owner(), vec![id.to_string()]).await.unwrap().accepted_task_ids.is_empty());
        let error = super::super::resources::run_snapshot(&reader, &owner(), veoveo_stream_mcp::contract::RunId::try_from(id).unwrap()).await.unwrap_err();
        assert_eq!(error.code, rmcp::model::ErrorCode::RESOURCE_NOT_FOUND);
    }).await.expect("operation isolation exceeded 60 seconds");
}

#[tokio::test]
async fn resumable_cancellation_settlement_preserves_owner_policy_and_product_links() {
    tokio::time::timeout(Duration::from_secs(60), async {
        use tokio_util::sync::CancellationToken;
        use veoveo_task_runtime::{TaskError, TaskFailure, TaskStatus, TaskTransition};
        let db = fixture::TestDb::new().await;
        fixture::module_lanes::install(
            &db.a,
            vec![
                veoveo_stream_mcp::schema::module_setup(
                    fixture::module_lanes::execution("stream").unwrap(),
                )
                .unwrap(),
            ],
        )
        .await
        .unwrap();
        let writer = veoveo_stream_mcp::task_lookup::bind(TaskRuntime::new(
            db.a.clone(),
            "stream",
            "settlement-writer",
        ))
        .unwrap();
        let reader = veoveo_stream_mcp::task_lookup::bind(TaskRuntime::new(
            db.b.clone(),
            "stream",
            "settlement-reader",
        ))
        .unwrap();
        let policy = crate::app_state::CANCELLATION_POLICY;
        #[derive(surrealdb::types::SurrealValue)]
        struct Publication {
            outcome: String,
            no_product: bool,
        }
        for kind in 0..4 {
            let id = TaskId::new();
            create(&writer, owner(), id).await;
            let before = writer.get(id).await.unwrap().unwrap();
            let canonical =
                recording_result(serde_json::from_value(current_output(id)).unwrap()).unwrap();
            let transition = if kind == 1 {
                TaskTransition::Failed(TaskFailure::new(
                    "fixture_failure",
                    "known execution failure",
                ))
            } else if kind == 2 {
                veoveo_task_runtime::mcp_task_completion(
                    "tool error",
                    CallToolResult::error(vec![ContentBlock::text("known tool error")]),
                )
                .unwrap()
            } else {
                veoveo_task_runtime::mcp_task_completion("complete", canonical.clone()).unwrap()
            };
            if kind < 3 {
                reader.cancel(id).await.unwrap();
                // Raw CAS demonstrates the original race without a workload delay.
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
            if kind < 3 {
                assert_eq!(after.status, TaskStatus::Cancelled);
                assert!(after.result.is_none() && after.result_uri.is_none());
            } else {
                assert_eq!(after.status, TaskStatus::Succeeded);
                assert_eq!(reader.cancel(id).await.unwrap(), after);
                assert_eq!(after.result, Some(serde_json::to_value(canonical).unwrap()));
            }
            let mut query =
                db.b.client()
                    .query(include_str!(
                        "../../../queries/bin/server/task_results_tests/no_product_settlement.surql"
                    ))
                    .bind((
                        "lookup",
                        surrealdb::types::RecordId::new("stream_run", id.to_string()),
                    ))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            let published: Publication = query.take::<Option<Publication>>(0).unwrap().unwrap();
            assert_eq!(published.no_product, kind < 3);
            if kind < 3 {
                assert_eq!(published.outcome, "cancelled");
            }
        }
        let id = TaskId::new();
        create(&writer, owner(), id).await;
        let before = writer.get(id).await.unwrap().unwrap();
        let stop = CancellationToken::new();
        stop.cancel();
        let canonical =
            recording_result(serde_json::from_value(current_output(id)).unwrap()).unwrap();
        let stopped = writer
            .transition_resumable_if_current(
                &before,
                veoveo_task_runtime::mcp_task_completion("complete", canonical).unwrap(),
                policy,
                Some(&stop),
            )
            .await
            .unwrap();
        assert_eq!(stopped, before);
        assert!(reader.get(id).await.unwrap().unwrap().result.is_none());
    })
    .await
    .expect("stream resumable settlement controls exceeded 60 seconds");
}

#[tokio::test]
async fn cancelled_progress_stops_before_the_next_effect() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        fixture::module_lanes::install(
            &db.a,
            vec![
                veoveo_stream_mcp::schema::module_setup(
                    fixture::module_lanes::execution("stream").unwrap(),
                )
                .unwrap(),
            ],
        )
        .await
        .unwrap();
        let writer = veoveo_stream_mcp::task_lookup::bind(TaskRuntime::new(
            db.a.clone(),
            "stream",
            "progress-writer",
        ))
        .unwrap();
        let canceller = veoveo_stream_mcp::task_lookup::bind(TaskRuntime::new(
            db.b.clone(),
            "stream",
            "progress-canceller",
        ))
        .unwrap();
        let id = TaskId::new();
        create(&writer, owner(), id).await;
        canceller.cancel(id).await.unwrap();
        let effects = std::sync::atomic::AtomicUsize::new(0);
        let work = async {
            crate::tasks::progress_checkpoint(
                &writer,
                veoveo_stream_mcp::contract::RunId::try_from(id)?,
                0.1,
                "before next effect",
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
    .expect("owner progress control exceeded 60 seconds");
}
