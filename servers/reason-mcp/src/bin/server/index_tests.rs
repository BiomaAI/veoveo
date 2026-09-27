use std::{collections::BTreeSet, time::Duration};

use serde_json::json;
use veoveo_mcp_contract::{
    InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy,
};
use veoveo_task_runtime::{CreateTask, PrincipalKind, RecoveryClass, TaskId};
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, TenantId, WorkContextId,
};

#[path = "../../../../../testing/fixtures/store.rs"]
mod fixture;

use super::*;

fn owner() -> TaskOwner {
    let principal = PrincipalId::new("reason-index-test").unwrap();
    TaskOwner {
        principal_key: principal.to_string(),
        principal_kind: PrincipalKind::User,
        issuer: "https://issuer.example".into(),
        subject: "reason-index-test".into(),
        profile: "operator".into(),
        tenant_key: Some("reason-index-test".into()),
        data_labels: BTreeSet::new(),
        authority: InvocationAuthority {
            work_context: WorkContextId::new("reason-index-test").unwrap(),
            tenant: TenantId::new("reason-index-test").unwrap(),
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

#[test]
fn analysis_cursor_round_trips_and_rejects_other_versions_and_collections() {
    let position = TaskPageCursor {
        created_at: chrono::Utc::now(),
        task_id: TaskId::new(),
    };
    let encoded = encode_cursor(position.clone()).unwrap();
    let parsed = parse_collection(&format!("reason://analyses?cursor={encoded}"))
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(parsed.position, position);
    assert_eq!(parse_collection(uris::ANALYSES_URI).unwrap(), Some(None));
    assert_eq!(parse_collection("reason://models").unwrap(), None);
    for uri in [
        "reason://analyses?",
        "reason://analyses?cursor=",
        "reason://analyses?offset=1",
        "reason://analyses?cursor=bad",
        "reason://analyses?cursor=one&cursor=two",
    ] {
        assert!(parse_collection(uri).is_err(), "{uri}");
    }
    for cursor in [
        AnalysisCursor {
            version: 2,
            ..parsed.clone()
        },
        AnalysisCursor {
            collection: "stream://runs".into(),
            ..parsed
        },
    ] {
        let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor).unwrap());
        assert!(parse_collection(&format!("reason://analyses?cursor={encoded}")).is_err());
    }
}

#[tokio::test]
async fn native_completion_filters_before_limits_and_deduplicates_artifacts() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let tasks = TaskRuntime::new(db.a.clone(), "reason", "index-test");
        let mut expected_tasks = Vec::new();
        let mut expected_artifacts = Vec::new();
        for index in 0..103 {
            let mut owner = owner();
            if index == 0 {
                owner.data_labels.insert("restricted".into());
            }
            let task = tasks
                .create(CreateTask {
                    task_id: TaskId::new(),
                    owner,
                    server: "reason".into(),
                    task_type: if index == 1 {
                        "unrelated"
                    } else {
                        "analyze_recording"
                    }
                    .into(),
                    request: json!({}),
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
            let result = veoveo_platform_store::OpenObject::new(
                [(
                    "structuredContent".into(),
                    json!({
                        "results_artifact": {"artifact_id": artifact},
                        "annotations_artifact": {"artifact_id": artifact},
                        "source_clip_artifact": {"artifact_id": artifact},
                    }),
                )]
                .into(),
            );
            db.b.client()
                .query("UPDATE ONLY $id SET result = $result;")
                .bind(("id", task.task_id.record_id()))
                .bind(("result", result))
                .await
                .unwrap()
                .check()
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
