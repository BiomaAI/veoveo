use std::{collections::BTreeSet, time::Duration};

use futures::StreamExt;
use rmcp::model::{DetailedTask, GetTaskParams};
use serde_json::{Value, json};
use veoveo_mcp_contract::{
    InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy,
};
use veoveo_task_runtime::{CreateTask, PrincipalKind, RecoveryClass, TaskTransition};
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, TaskId, TenantId,
    WorkContextId,
};

use crate::store_fixture as fixture;

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
        assert_eq!(result.structured_content.unwrap()["result_uri"], link.uri);
    }
}

fn owner() -> TaskOwner {
    let principal = PrincipalId::new("stream-result-test").unwrap();
    TaskOwner {
        principal_key: principal.to_string(),
        principal_kind: PrincipalKind::User,
        issuer: "https://issuer.example".into(),
        subject: "stream-result-test".into(),
        profile: "operator".into(),
        tenant_key: Some("stream-result-test".into()),
        data_labels: BTreeSet::new(),
        authority: InvocationAuthority {
            work_context: WorkContextId::new("stream-result-test").unwrap(),
            tenant: TenantId::new("stream-result-test").unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::new("test-v1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(principal.clone()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: principal,
            },
        },
    }
}

fn current_output(id: TaskId) -> Value {
    let mut value: Value =
        serde_json::from_str(include_str!("../../../testdata/run-output.json")).unwrap();
    let id = RunId::try_from(id).unwrap();
    value["run_uri"] = serde_json::to_value(veoveo_stream_mcp::contract::RunUri::new(id)).unwrap();
    value["result_uri"] =
        serde_json::to_value(veoveo_stream_mcp::contract::RunResultsUri::new(id)).unwrap();
    value
}

async fn create(runtime: &TaskRuntime, owner: TaskOwner, id: TaskId) {
    // These capabilities are inert fixture values. No Artifact or GPU service
    // participates in control-plane result-delivery qualification.
    let capability = json!({
        "capability_id": "01983da0-0000-7000-8000-000000000004",
        "secret": "inert_fixture_capability_not_issued_0000",
        "task_id": id.to_string(),
        "expires_at": "2030-01-01T00:00:00Z"
    });
    let request = json!({
        "input": {
            "operation": "run_recording",
            "video": {
                "recording_uri": "recording://recordings/01983da0-0000-7000-8000-000000000001",
                "entity_path": "/camera/front", "timeline": "sensor_time",
                "range": {"start": 10, "end": 20}
            },
            "pipeline_id": "traffic"
        },
        "artifact_write_capability": capability,
        "artifact_read_capability": capability
    });
    serde_json::from_value::<super::super::tasks::DurableStreamRequest>(request.clone()).unwrap();
    runtime
        .create(CreateTask {
            task_id: id,
            owner,
            server: "stream".into(),
            task_type: "run_recording".into(),
            request,
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap();
    runtime
        .claim(&id.to_string(), Duration::from_secs(30))
        .await
        .unwrap();
}

async fn finish(runtime: &TaskRuntime, id: TaskId, stored: Value) {
    runtime
        .transition(
            &id.to_string(),
            TaskTransition::Succeeded {
                message: RUN_COMPLETED.into(),
                result: stored,
            },
        )
        .await
        .unwrap();
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
    assert_eq!(link.uri, expected["result_uri"].as_str().unwrap());
    assert_eq!(
        link.mime_type.as_deref(),
        Some("application/vnd.veoveo.stream-results+json")
    );
}

#[tokio::test]
async fn current_results_survive_cross_replica_reads_and_listener_reconnects() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "stream", "result-writer");
        let reader = TaskRuntime::new(db.b.clone(), "stream", "result-reader");
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
        let retained = reader.get_for_owner(&owner(), id).await.unwrap().unwrap();
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
        let writer = TaskRuntime::new(db.a.clone(), "stream", "invalid-writer");
        let reader = TaskRuntime::new(db.b.clone(), "stream", "invalid-reader");
        for corruption in [
            "wrong-pipeline",
            "wrong-task",
            "missing-product",
            "missing-result-uri",
            "wrong-link",
            "extra-link",
        ] {
            let id = TaskId::new();
            create(&writer, owner(), id).await;
            let mut value = current_output(id);
            let mut result =
                recording_result(serde_json::from_value(value.clone()).unwrap()).unwrap();
            match corruption {
                "wrong-pipeline" => value["pipeline_uri"] = "stream://pipeline/other".into(),
                "wrong-task" => {
                    value = current_output(TaskId::new());
                    result =
                        recording_result(serde_json::from_value(value.clone()).unwrap()).unwrap();
                }
                "missing-product" => value = Value::Null,
                "missing-result-uri" => {
                    value.as_object_mut().unwrap().remove("result_uri");
                }
                "wrong-link" => {
                    let ContentBlock::ResourceLink(link) = &mut result.content[1] else {
                        unreachable!()
                    };
                    link.uri = "stream://pipeline/other".into();
                }
                "extra-link" => result.content.push(result.content[1].clone()),
                _ => unreachable!(),
            }
            result.structured_content = Some(value);
            finish(&writer, id, serde_json::to_value(result).unwrap()).await;
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
        let tasks = TaskRuntime::new(db.a.clone(), "stream", "tool-error-reader");
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
    })
    .await
    .expect("Stream tool error qualification exceeded 60 seconds");
}
