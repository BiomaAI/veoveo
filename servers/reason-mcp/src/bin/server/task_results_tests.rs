use std::{collections::BTreeSet, time::Duration};
use surrealdb::types::SurrealValue;

use futures::StreamExt;
use rmcp::model::{CallToolResult, ContentBlock, DetailedTask, GetTaskParams};
use serde_json::{Value, json};
use veoveo_reason_mcp::contract::AnalyzeRecordingOutput;
use veoveo_task_runtime::{CreateTask, PrincipalKind, RecoveryClass};
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, TaskId, TenantId,
    WorkContextId,
};
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy};

use crate::store_fixture as fixture;

use super::*;

fn owner() -> TaskOwner {
    let principal = PrincipalId::parse("reason-result-test").unwrap();
    TaskOwner {
        principal_key: principal.to_string(),
        principal_kind: PrincipalKind::User,
        issuer: "https://issuer.example".into(),
        subject: "reason-result-test".into(),
        profile: "operator".into(),
        tenant_key: Some("reason-result-test".into()),
        data_labels: BTreeSet::new(),
        authority: InvocationAuthority {
            work_context: WorkContextId::parse("reason-result-test").unwrap(),
            tenant: TenantId::parse("reason-result-test").unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::parse("test-v1").unwrap(),
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
        serde_json::from_str(include_str!("../../../testdata/analysis-output-v1.json")).unwrap();
    value["analysis_uri"] = format!("reason://analysis/{id}").into();
    value["result_uri"] = format!("reason://analysis/{id}/results").into();
    value["results_artifact"]["metadata"]["provenance"]["analysis_id"] = id.to_string().into();
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
            task_type: const { veoveo_types::TaskTypeName::from_static("analyze_recording") },
            request,
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap();
    runtime.claim(id, Duration::from_secs(30)).await.unwrap();
}

async fn finish(runtime: &TaskRuntime, id: TaskId, stored: Value) {
    runtime
        .transition(
            id,
            veoveo_task_runtime::mcp_task_completion(
                ANALYSIS_COMPLETED,
                serde_json::from_value(stored).unwrap(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
}
async fn corrupt_result(runtime: &TaskRuntime, id: TaskId, stored: Value) {
    let canonical =
        analysis_tool_result(serde_json::from_value(current_output(id)).unwrap()).unwrap();
    finish(runtime, id, serde_json::to_value(canonical).unwrap()).await;
    runtime
        .platform_store()
        .client()
        .query(include_str!(
            "../../../queries/bin/server/test_support/corrupt_result.surql"
        ))
        .bind(("task", veoveo_platform_store::task_record_id(id)))
        .bind((
            "result",
            veoveo_platform_store::TaskResultRecord::new(stored),
        ))
        .await
        .unwrap()
        .check()
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
async fn current_results_survive_cross_replica_reads_and_listener_reconnects() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        fixture::module_lanes::install(
            &db.a,
            vec![
                veoveo_reason_mcp::schema::module_setup(
                    fixture::module_lanes::execution("reason").unwrap(),
                )
                .unwrap(),
            ],
        )
        .await
        .unwrap();
        let writer = veoveo_reason_mcp::task_lookup::bind(TaskRuntime::new(
            db.a.clone(),
            "reason",
            "result-writer",
        ))
        .unwrap();
        let reader = veoveo_reason_mcp::task_lookup::bind(TaskRuntime::new(
            db.b.clone(),
            "reason",
            "result-reader",
        ))
        .unwrap();
        let id = TaskId::new();
        create(&writer, owner(), id).await;
        let canonical: AnalyzeRecordingOutput = serde_json::from_value(current_output(id)).unwrap();
        let expected = serde_json::to_value(&canonical).unwrap();
        let stored = serde_json::to_value(analysis_tool_result(canonical).unwrap()).unwrap();
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
        let retained = reader.for_owner(&owner()).get(id).await.unwrap().unwrap();
        assert_eq!(
            retained.result_uri.as_ref().map(|uri| uri.as_str()),
            expected["result_uri"].as_str()
        );
        assert_eq!(retained.result, Some(stored));
        assert_eq!(
            serde_json::to_value(analysis_view(&retained).unwrap().output().unwrap()).unwrap(),
            expected
        );
    })
    .await
    .expect("Reason current result qualification exceeded 90 seconds");
}

#[tokio::test]
async fn task_delivery_rejects_corruption_and_denies_access_before_domain_decode() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        fixture::module_lanes::install(
            &db.a,
            vec![
                veoveo_reason_mcp::schema::module_setup(
                    fixture::module_lanes::execution("reason").unwrap(),
                )
                .unwrap(),
            ],
        )
        .await
        .unwrap();
        let writer = veoveo_reason_mcp::task_lookup::bind(TaskRuntime::new(
            db.a.clone(),
            "reason",
            "invalid-writer",
        ))
        .unwrap();
        let reader = veoveo_reason_mcp::task_lookup::bind(TaskRuntime::new(
            db.b.clone(),
            "reason",
            "invalid-reader",
        ))
        .unwrap();
        for corruption in [
            "unknown-schema",
            "unversioned",
            "obsolete-result-field",
            "wrong-pipeline",
            "wrong-task",
            "missing-product",
        ] {
            let id = TaskId::new();
            create(&writer, owner(), id).await;
            let mut value = current_output(id);
            match corruption {
                "unknown-schema" => value["schema"] = "unknown/v2".into(),
                "unversioned" => {
                    value.as_object_mut().unwrap().remove("schema");
                }
                "obsolete-result-field" => {
                    value["results_uri"] =
                        value.as_object_mut().unwrap().remove("result_uri").unwrap();
                }
                "wrong-pipeline" => value["pipeline_uri"] = "reason://pipeline/other".into(),
                "wrong-task" => value = current_output(TaskId::new()),
                "missing-product" => value = Value::Null,
                _ => unreachable!(),
            }
            corrupt_result(
                &writer,
                id,
                json!({"content": [], "structuredContent": value}),
            )
            .await;
            let error = get_task(&reader, &owner(), GetTaskParams::new(id.to_string()))
                .await
                .unwrap_err();
            assert!(
                error
                    .message
                    .contains("stored Reason output does not satisfy the current contract"),
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
        fixture::module_lanes::install(
            &db.a,
            vec![
                veoveo_reason_mcp::schema::module_setup(
                    fixture::module_lanes::execution("reason").unwrap(),
                )
                .unwrap(),
            ],
        )
        .await
        .unwrap();
        let tasks = veoveo_reason_mcp::task_lookup::bind(TaskRuntime::new(
            db.a.clone(),
            "reason",
            "tool-error-reader",
        ))
        .unwrap();
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
                surrealdb::types::RecordId::new("reason_analysis", id.to_string()),
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
        let actor = owner();
        let identity = tasks
            .platform_store()
            .ensure_identity(
                actor.tenant_key(),
                &actor.principal_key,
                &actor.issuer,
                &actor.subject,
                actor.principal_kind,
            )
            .await
            .unwrap();
        let scope = veoveo_platform_store::ArtifactReadScope::new(
            &identity,
            [],
            std::collections::BTreeSet::new(),
            None,
        )
        .unwrap();
        assert!(
            veoveo_reason_mcp::knowledge::readable_findings(
                tasks.platform_store(),
                &scope,
                veoveo_reason_mcp::knowledge::FindingSelection::Member(
                    veoveo_reason_mcp::contract::AnalysisId::try_from(id).unwrap()
                )
            )
            .await
            .unwrap()
            .is_empty()
        );
    })
    .await
    .expect("Reason tool error qualification exceeded 60 seconds");
}

#[tokio::test]
async fn unrelated_malformed_operation_is_excluded_before_task_and_resource_decode() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        fixture::module_lanes::install(&db.a, vec![veoveo_reason_mcp::schema::module_setup(fixture::module_lanes::execution("reason").unwrap()).unwrap()]).await.unwrap();
        let writer = veoveo_reason_mcp::task_lookup::bind(TaskRuntime::new(db.a.clone(), "reason", "writer")).unwrap();
        let reader = veoveo_reason_mcp::task_lookup::bind(TaskRuntime::new(db.b.clone(), "reason", "reader")).unwrap();
        let id = TaskId::new();
        create(&writer, owner(), id).await;
        db.b.client().query(include_str!("../../../queries/bin/server/task_results_tests/unrelated_malformed_operation_is_excluded_before_task_and_resource_decode.surql"))
            .bind(("task", veoveo_platform_store::task_record_id(id))).await.unwrap().check().unwrap();
        assert!(reader.for_owner(&owner()).get(id).await.is_err());
        let error = get_task(&reader, &owner(), GetTaskParams::new(id.to_string())).await.unwrap_err();
        assert_eq!(error.message, "unknown task id");
        assert!(subscribe_tasks(&reader, owner(), vec![id.to_string()]).await.unwrap().accepted_task_ids.is_empty());
        let error = super::super::resources::analysis_snapshot(&reader, &owner(), veoveo_reason_mcp::contract::AnalysisId::try_from(id).unwrap()).await.unwrap_err();
        assert_eq!(error.code, rmcp::model::ErrorCode::RESOURCE_NOT_FOUND);
    }).await.expect("operation isolation exceeded 60 seconds");
}
