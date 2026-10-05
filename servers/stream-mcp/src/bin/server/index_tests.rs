use std::{collections::BTreeSet, time::Duration};
use veoveo_platform_store::task_record_id;
use veoveo_types::TaskTypeDefinition;

use serde_json::json;
use veoveo_task_runtime::{CreateTask, PrincipalKind, RecoveryClass};
use veoveo_types::TaskId;
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, TenantId, WorkContextId,
};
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy};

use crate::store_fixture as fixture;

use super::*;

fn owner() -> TaskOwner {
    let principal = PrincipalId::parse("stream-index-test").unwrap();
    TaskOwner {
        principal_key: principal.to_string(),
        principal_kind: PrincipalKind::User,
        issuer: "https://issuer.example".into(),
        subject: "stream-index-test".into(),
        profile: "operator".into(),
        tenant_key: Some("stream-index-test".into()),
        data_labels: BTreeSet::new(),
        authority: InvocationAuthority {
            work_context: WorkContextId::parse("stream-index-test").unwrap(),
            tenant: TenantId::parse("stream-index-test").unwrap(),
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

fn request() -> serde_json::Value {
    let capability = json!({
        "capability_id": uuid::Uuid::now_v7(),
        "secret": "fixture-capability-never-sent-to-an-artifact-service",
        "task_id": uuid::Uuid::now_v7(),
        "expires_at": chrono::Utc::now(),
    });
    let request = json!({
        "input": {
            "operation": "run_recording",
            "pipeline_id": "fixture",
            "video": {
                "recording_uri": format!("recording://recordings/{}", uuid::Uuid::now_v7()),
                "entity_path": "camera", "timeline": "log_time", "range": {"start": 0, "end": 1}
            }
        },
        "artifact_write_capability": capability,
        "artifact_read_capability": capability,
    });
    serde_json::from_value::<super::super::tasks::DurableStreamRequest>(request.clone()).unwrap();
    request
}

#[tokio::test]
async fn native_completion_filters_before_limits_and_deduplicates_artifacts() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let tasks = TaskRuntime::new(db.a.clone(), "stream", "index-test");
        let mut expected_tasks = Vec::new();
        let mut expected_artifacts = Vec::new();
        for index in 0..103 {
            let mut task_owner = owner();
            if index == 0 {
                task_owner.data_labels.insert("restricted".into());
            }
            let task = tasks
                .create(CreateTask {
                    task_id: TaskId::new(),
                    owner: task_owner,
                    server: "stream".into(),
                    task_type: if index == 1 {
                        const { veoveo_types::TaskTypeName::from_static("unrelated") }
                    } else {
                        veoveo_stream_mcp::contract::StreamTaskKind::RunRecording.name()
                    },
                    request: if index < 2 {
                        json!({"malformed": true})
                    } else {
                        request()
                    },
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
                serde_json::from_str(include_str!("../../../testdata/run-output.json")).unwrap();
            let run = veoveo_stream_mcp::contract::RunId::try_from(task.task_id).unwrap();
            output["run_uri"] =
                serde_json::to_value(veoveo_stream_mcp::contract::RunUri::new(run)).unwrap();
            output["result_uri"] =
                serde_json::to_value(veoveo_stream_mcp::contract::RunResultsUri::new(run)).unwrap();
            output["pipeline_uri"] = "stream://pipeline/fixture".into();
            for field in ["results_artifact", "annotations_artifact"] {
                output[field]["artifact_id"] = artifact.clone().into();
                output[field]["artifact_uri"] = serde_json::to_value(
                    veoveo_stream_mcp::uris::artifact_uri(artifact.parse().unwrap()),
                )
                .unwrap();
            }
            output["source_clip_artifact"] = output["results_artifact"].clone();
            let result = super::super::task_results::recording_result(
                serde_json::from_value(output).unwrap(),
            )
            .unwrap();
            let result =
                veoveo_platform_store::TaskResultRecord::new(serde_json::to_value(result).unwrap());
            db.b.client()
                .query(include_str!("../../../queries/bin/server/index_tests/native_completion_filters_before_limits_and_deduplicates_artifacts.surql"))
                .bind(("id", task_record_id(task.task_id)))
                .bind(("result", result))
                .await
                .unwrap()
                .check()
                .unwrap();
            if index <= 2 {
                let id = RunId::try_from(task.task_id).unwrap();
                let selected = super::super::resources::run_snapshot(&tasks, &owner(), id).await;
                assert_eq!(selected.is_ok(), index == 2);
                if index == 2 {
                    for field in ["principal", "profile", "tenant"] {
                        let mut reader = owner();
                        match field {
                            "principal" => reader.principal_key = "stranger".into(),
                            "profile" => reader.profile = "stranger".into(),
                            "tenant" => reader.tenant_key = Some("stranger".into()),
                            _ => unreachable!(),
                        }
                        assert!(
                            super::super::resources::run_snapshot(&tasks, &reader, id)
                                .await
                                .is_err()
                        );
                    }
                }
            }
            if index >= 2 {
                expected_tasks.push(task.task_id.to_string());
                expected_artifacts.push(artifact);
            }
        }
        let page = runs_page(&tasks, &owner(), None).await.unwrap();
        assert_eq!(page.limit, 100);
        assert_eq!(page.runs.len(), 100);
        let cursor = RunCursor::parse(page.next_cursor.unwrap().as_str()).unwrap();
        let tail = runs_page(&tasks, &owner(), Some(&cursor)).await.unwrap();
        assert_eq!(tail.runs.len(), 1);
        assert!(tail.next_cursor.is_none());
        assert_eq!(
            page.runs
                .into_iter()
                .chain(tail.runs)
                .map(|r| r.task_id().to_string())
                .collect::<Vec<_>>(),
            expected_tasks
        );
        let mut stranger = owner();
        stranger.principal_key = "another-caller".into();
        assert!(
            runs_page(&tasks, &stranger, Some(&cursor))
                .await
                .unwrap()
                .runs
                .is_empty()
        );
        for (domain, mut expected) in [
            (CompletionDomain::Runs, expected_tasks),
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
    .expect("Stream collection qualification exceeded 90 seconds");
}
