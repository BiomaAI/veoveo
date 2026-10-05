use std::{collections::BTreeSet, time::Duration};
use veoveo_platform_store::task_record_id;
use veoveo_types::TaskTypeDefinition;

use veoveo_task_runtime::{CreateTask, PrincipalKind, RecoveryClass};
use veoveo_types::TaskId;
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, TenantId, WorkContextId,
};
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy};

use crate::store_fixture as fixture;

use super::*;

fn owner() -> TaskOwner {
    let principal = PrincipalId::parse("reason-index-test").unwrap();
    TaskOwner {
        principal_key: principal.to_string(),
        principal_kind: PrincipalKind::User,
        issuer: "https://issuer.example".into(),
        subject: "reason-index-test".into(),
        profile: "operator".into(),
        tenant_key: Some("reason-index-test".into()),
        data_labels: BTreeSet::new(),
        authority: InvocationAuthority {
            work_context: WorkContextId::parse("reason-index-test").unwrap(),
            tenant: TenantId::parse("reason-index-test").unwrap(),
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

#[tokio::test]
async fn native_completion_filters_before_limits_and_deduplicates_artifacts() {
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
    tokio::time::timeout(Duration::from_secs(90), async {
        let tasks = veoveo_reason_mcp::task_lookup::bind(TaskRuntime::new(
            db.a.clone(),
            "reason",
            "index-test",
        ))
        .unwrap();
        let mut expected_tasks = Vec::new();
        let mut expected_artifacts = Vec::new();
        for index in 0..103 {
            let mut owner = owner();
            if index == 0 {
                owner.data_labels.insert("restricted".into());
            }
            let mut request: serde_json::Value = serde_json::from_str(include_str!(
                "../../../testdata/task-request.json"
            )).unwrap();
            let variant = match index {
                2 | 3 => Some(("describe_segment", "description")),
                4 | 5 => Some(("detect_events", "events")),
                6 | 7 => Some(("answer_question", "answer")),
                _ => None,
            };
            if let Some((kind, _)) = variant {
                request["input"]["task"] = match kind {
                    "describe_segment" if index == 2 => serde_json::json!({"kind": kind}),
                    "describe_segment" | "detect_events" => serde_json::json!({"kind": kind, "prompt": "Observe traffic"}),
                    "answer_question" => serde_json::json!({"kind": kind, "question": "What happened?"}),
                    _ => unreachable!(),
                };
            }
            let task = tasks
                .create(CreateTask {
                    task_id: TaskId::new(),
                    owner,
                    server: "reason".into(),
                    task_type: if index == 1 {
                        const { veoveo_types::TaskTypeName::from_static("unrelated") }
                    } else {
                        veoveo_reason_mcp::contract::ReasonTaskKind::AnalyzeRecording.name()
                    },
                    request: request.clone(),
                    recovery_class: RecoveryClass::Resume,
                    idempotency_key: None,
                    ttl_ms: None,
                    poll_interval_ms: None,
                    retention_pins: BTreeSet::new(),
                })
                .await
                .unwrap()
                .snapshot;
            let artifact = uuid::Uuid::now_v7().to_string();
            let mut output: serde_json::Value =
                serde_json::from_str(include_str!("../../../testdata/analysis-output-v1.json"))
                    .unwrap();
            if let Some((kind, answer)) = variant {
                output["finding"]["task"] = request["input"]["task"].clone();
                if answer != "events" {
                    output["finding"]["answer"] = serde_json::json!({
                        "kind": answer,
                        "excerpt": {"text": "A vehicle entered.", "truncated": false}
                    });
                    output["summary"]["event_count"] = 0.into();
                }
                output["finding"]["decode"] = if index % 2 == 0 {
                    serde_json::json!({"mode": "greedy"})
                } else {
                    serde_json::json!({"mode": "sampled", "temperature": 0.5, "top_p": 0.75, "seed": 7})
                };
                output["results_artifact"]["metadata"]["provenance"]["task_kind"] = kind.into();
                if index == 2 {
                    output["finding"]["model_digest"] = serde_json::Value::Null;
                } else if index == 3 {
                    output["finding"].as_object_mut().unwrap().remove("model_digest");
                }
            }
            let analysis = veoveo_reason_mcp::contract::AnalysisId::try_from(task.task_id).unwrap();
            output["analysis_uri"] =
                serde_json::to_value(veoveo_reason_mcp::contract::AnalysisUri::new(analysis))
                    .unwrap();
            output["result_uri"] =
                serde_json::to_value(veoveo_reason_mcp::contract::ResultsUri::new(analysis))
                    .unwrap();
            output["results_artifact"]["metadata"]["provenance"]["analysis_id"] =
                task.task_id.to_string().into();
            for field in ["results_artifact", "annotations_artifact"] {
                output[field]["artifact_id"] = artifact.clone().into();
                output[field]["artifact_uri"] = format!("reason://artifact/{artifact}").into();
            }
            output["source_clip_artifact"] = output["results_artifact"].clone();
            let output: veoveo_reason_mcp::contract::AnalyzeRecordingOutput =
                serde_json::from_value(output).unwrap();
            let result = super::super::task_results::analysis_tool_result(output).unwrap();
            tasks
                .claim(task.task_id, Duration::from_secs(30))
                .await
                .unwrap();
            tasks
                .transition(
                    task.task_id,
                    veoveo_task_runtime::mcp_task_completion("fixture", result).unwrap(),
                )
                .await
                .unwrap();
            if index >= 2 {
                expected_tasks.push(task.task_id.to_string());
                expected_artifacts.push(artifact);
            }
        }
        for (domain, mut expected) in [
            (CompletionDomain::Analyses, expected_tasks),
            (CompletionDomain::Artifacts, expected_artifacts),
        ] {
            expected.sort();
            let page = complete(&tasks, &owner(), domain, "").await.unwrap();
            assert_eq!(page.values, expected[..100]);
            assert_eq!(page.has_more, Some(true));
            assert_eq!(page.total, None);
            let needle = expected.last().unwrap();
            let filtered = complete(&tasks, &owner(), domain, needle).await.unwrap();
            assert_eq!(filtered.values.as_slice(), std::slice::from_ref(needle));
            assert_eq!(filtered.has_more, Some(false));
            assert_eq!(filtered.total, Some(1));
            let mut stranger = owner();
            stranger.principal_key = "another-caller".into();
            assert!(
                complete(&tasks, &stranger, domain, "")
                    .await
                    .unwrap()
                    .values
                    .is_empty()
            );
        }
    })
    .await
    .expect("Reason completion qualification exceeded 90 seconds");
}

#[tokio::test]
async fn resource_reads_and_subscription_admission_filter_before_decoding() {
    use super::super::resources::{analysis_snapshot, subscribable_analysis_id};
    use veoveo_reason_mcp::{contract::AnalysisId, uris};

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
    tokio::time::timeout(Duration::from_secs(90), async {
        let reader = veoveo_reason_mcp::task_lookup::bind(TaskRuntime::new(db.a.clone(), "reason", "resource-reader")).unwrap();
        let writer = veoveo_reason_mcp::task_lookup::bind(TaskRuntime::new(db.b.clone(), "reason", "resource-writer")).unwrap();
        let draft = || CreateTask {
            task_id: TaskId::new(),
            owner: owner(),
            server: "reason".into(),
            task_type: const { veoveo_types::TaskTypeName::from_static("analyze_recording") },
            request: serde_json::from_str(include_str!("../../../testdata/task-request.json")).unwrap(),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        };
        let task = writer.create(draft()).await.unwrap().snapshot;
        let id = AnalysisId::try_from(task.task_id).unwrap();
        for uri in [
            uris::analysis_uri(id).to_string(),
            uris::results_uri(id).to_string(),
        ] {
            let selected = subscribable_analysis_id(&uri).unwrap();
            assert_eq!(
                analysis_snapshot(&reader, &owner(), selected)
                    .await
                    .unwrap()
                    .task_id,
                task.task_id
            );
        }
        for (mutation, statement) in [
("owner_context.data_labels = ['restricted']", include_str!("../../../queries/bin/server/index_tests/resource_reads_and_subscription_admission_filter_before_decoding_variant_1.surql")),
("owner_context.principal_key = 'inconsistent'", include_str!("../../../queries/bin/server/index_tests/resource_reads_and_subscription_admission_filter_before_decoding_variant_2.surql")),
("owner_context.profile = 'inconsistent'", include_str!("../../../queries/bin/server/index_tests/resource_reads_and_subscription_admission_filter_before_decoding_variant_3.surql")),
("owner_context.tenant_key = 'inconsistent'", include_str!("../../../queries/bin/server/index_tests/resource_reads_and_subscription_admission_filter_before_decoding_variant_4.surql")),
("owner = principal:other", include_str!("../../../queries/bin/server/index_tests/resource_reads_and_subscription_admission_filter_before_decoding_variant_5.surql")),
("profile = profile:other", include_str!("../../../queries/bin/server/index_tests/resource_reads_and_subscription_admission_filter_before_decoding_variant_6.surql")),
("tenant = tenant:other", include_str!("../../../queries/bin/server/index_tests/resource_reads_and_subscription_admission_filter_before_decoding_variant_7.surql"))
] {
            let task = writer.create(draft()).await.unwrap().snapshot;
            db.b.client()
                .query(statement)
                .bind(("task", task_record_id(task.task_id)))
                .await
                .unwrap()
                .check()
                .unwrap();
            // The old unrestricted lookup cannot decode this persisted envelope.
            assert!(reader.get(task.task_id).await.is_err());
            let id = AnalysisId::try_from(task.task_id).unwrap();
            for uri in [
                uris::analysis_uri(id).to_string(),
                uris::results_uri(id).to_string(),
            ] {
                let selected = subscribable_analysis_id(&uri).unwrap();
                let error = analysis_snapshot(&reader, &owner(), selected)
                    .await
                    .unwrap_err();
                assert_eq!(error.message.as_ref(), "analysis not found", "{mutation}");
            }
        }
        let mut unrelated = draft();
        unrelated.task_type = const { veoveo_types::TaskTypeName::from_static("unrelated") };
        let other = writer.create(unrelated).await.unwrap().snapshot;
        assert!(
            analysis_snapshot(
                &reader,
                &owner(),
                AnalysisId::try_from(other.task_id).unwrap()
            )
            .await
            .is_err()
        );
        for uri in [
            uris::ANALYSES_URI,
            uris::PIPELINES_URI,
            "reason://pipeline/traffic",
            "reason://analysis/task-1",
        ] {
            assert!(subscribable_analysis_id(uri).is_err());
        }
    })
    .await
    .expect("Reason resource qualification exceeded 90 seconds");
}
