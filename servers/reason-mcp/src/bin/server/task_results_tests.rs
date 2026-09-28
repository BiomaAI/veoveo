use std::{collections::BTreeSet, time::Duration};

use futures::StreamExt;
use rmcp::model::{DetailedTask, GetTaskParams};
use serde_json::{Value, json};
use veoveo_mcp_contract::{
    InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy,
};
use veoveo_reason_mcp::contract::{AnalysisOutputProfile, RetainedAnalysisOutput};
use veoveo_task_runtime::{CreateTask, PrincipalKind, RecoveryClass, TaskTransition};
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, TaskId, TenantId,
    WorkContextId,
};

use crate::store_fixture as fixture;

use super::*;

fn owner() -> TaskOwner {
    let principal = PrincipalId::new("reason-result-test").unwrap();
    TaskOwner {
        principal_key: principal.to_string(),
        principal_kind: PrincipalKind::User,
        issuer: "https://issuer.example".into(),
        subject: "reason-result-test".into(),
        profile: "operator".into(),
        tenant_key: Some("reason-result-test".into()),
        data_labels: BTreeSet::new(),
        authority: InvocationAuthority {
            work_context: WorkContextId::new("reason-result-test").unwrap(),
            tenant: TenantId::new("reason-result-test").unwrap(),
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

fn legacy_output(id: TaskId) -> Value {
    let mut value: Value =
        serde_json::from_str(include_str!("../../../testdata/analysis-output-v0.json")).unwrap();
    value["analysis_uri"] = format!("reason://analysis/{id}").into();
    value["results_uri"] = format!("reason://analysis/{id}/results").into();
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
            "operation": "analyze",
            "video": {
                "recording_uri": "recording://recordings/01983da0-0000-7000-8000-000000000001",
                "entity_path": "/camera/front", "timeline": "sensor_time",
                "range": {"start": 10, "end": 20}
            },
            "pipeline_id": "traffic-events",
            "task": {"kind": "detect_events", "prompt": "Observe traffic"}
        },
        "artifact_write_capability": capability,
        "artifact_read_capability": capability
    });
    serde_json::from_value::<super::super::tasks::DurableReasonRequest>(request.clone()).unwrap();
    runtime
        .create(CreateTask {
            task_id: id,
            owner,
            server: "reason".into(),
            task_type: "analyze_recording".into(),
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
                message: format!("old completion for {id}"),
                result: stored,
            },
        )
        .await
        .unwrap();
}

fn assert_handoff(task: DetailedTask, expected: &Value) {
    assert_eq!(
        task.task.status_message.as_deref(),
        Some(ANALYSIS_COMPLETED)
    );
    let TaskPayload::Completed { result } = task.payload else {
        panic!("expected completed Task")
    };
    let result: CallToolResult = serde_json::from_value(Value::Object(result)).unwrap();
    assert_eq!(result.structured_content.as_ref(), Some(expected));
    let [ContentBlock::Text(status), ContentBlock::ResourceLink(link)] = result.content.as_slice()
    else {
        panic!("completion needs one status and one result link")
    };
    assert_eq!(status.text, ANALYSIS_COMPLETED);
    assert_eq!(link.uri, expected["result_uri"].as_str().unwrap());
    assert_eq!(
        link.mime_type.as_deref(),
        Some("application/vnd.veoveo.reason-results+json")
    );
}

#[tokio::test]
async fn both_retained_profiles_project_identically_without_writing_the_store() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "reason", "result-writer");
        let reader = TaskRuntime::new(db.b.clone(), "reason", "result-reader");
        for profile in [
            AnalysisOutputProfile::UnversionedV0,
            AnalysisOutputProfile::V1,
        ] {
            let id = TaskId::new();
            create(&writer, owner(), id).await;
            let canonical = RetainedAnalysisOutput::decode(legacy_output(id))
                .unwrap()
                .into_output();
            let expected = serde_json::to_value(&canonical).unwrap();
            let stored = match profile {
                AnalysisOutputProfile::UnversionedV0 => json!({
                    "content": [{"type":"text", "text": format!("legacy analysis {id}")}],
                    "structuredContent": legacy_output(id)
                }),
                AnalysisOutputProfile::V1 => {
                    serde_json::to_value(analysis_tool_result(canonical).unwrap()).unwrap()
                }
            };
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
            // A new listener's baseline uses the same conversion after reconnect.
            let mut reconnected = subscribe_tasks(&reader, owner(), vec![id.to_string()])
                .await
                .unwrap();
            assert_handoff(
                reconnected.updates.next().await.unwrap().unwrap(),
                &expected,
            );
            let retained = reader.get_for_owner(&owner(), id).await.unwrap().unwrap();
            assert_eq!(retained.result, Some(stored));
            assert_eq!(
                serde_json::to_value(analysis_view(&retained).unwrap().output().unwrap()).unwrap(),
                expected
            );
        }
    })
    .await
    .expect("Reason retained result qualification exceeded 90 seconds");
}

#[tokio::test]
async fn task_delivery_rejects_corruption_and_denies_access_before_domain_decode() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let writer = TaskRuntime::new(db.a.clone(), "reason", "invalid-writer");
        let reader = TaskRuntime::new(db.b.clone(), "reason", "invalid-reader");
        for corruption in [
            "unknown-schema",
            "wrong-pipeline",
            "wrong-task",
            "missing-product",
        ] {
            let id = TaskId::new();
            create(&writer, owner(), id).await;
            let mut value = legacy_output(id);
            match corruption {
                "unknown-schema" => value["schema"] = "unknown/v2".into(),
                "wrong-pipeline" => value["pipeline_uri"] = "reason://pipeline/other".into(),
                "wrong-task" => value = legacy_output(TaskId::new()),
                "missing-product" => value = Value::Null,
                _ => unreachable!(),
            }
            finish(
                &writer,
                id,
                json!({"content": [], "structuredContent": value}),
            )
            .await;
            let error = get_task(&reader, &owner(), GetTaskParams::new(id.to_string()))
                .await
                .unwrap_err();
            assert!(
                error.message.contains("retained Reason output is invalid"),
                "{corruption}"
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
    .expect("Reason denied/corrupt result qualification exceeded 90 seconds");
}

#[tokio::test]
async fn explicit_tool_error_keeps_its_no_product_envelope() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let tasks = TaskRuntime::new(db.a.clone(), "reason", "tool-error-reader");
        let id = TaskId::new();
        create(&tasks, owner(), id).await;
        let stored = serde_json::to_value(CallToolResult::error(vec![ContentBlock::text(
            "Analysis failed.",
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
    .expect("Reason tool error qualification exceeded 60 seconds");
}
