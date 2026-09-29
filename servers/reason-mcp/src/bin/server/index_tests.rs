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
                        const { veoveo_types::TaskTypeName::from_static("unrelated") }
                    } else {
                        veoveo_reason_mcp::contract::ReasonTaskKind::AnalyzeRecording.name()
                    },
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
            let result = veoveo_platform_store::TaskResultRecord::new(json!({
                "structuredContent": {
                    "results_artifact": {"artifact_id": artifact},
                    "annotations_artifact": {"artifact_id": artifact},
                    "source_clip_artifact": {"artifact_id": artifact},
                }
            }));
            db.b.client()
                .query("UPDATE ONLY $id SET result = $result;")
                .bind(("id", task_record_id(task.task_id)))
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

#[tokio::test]
async fn resource_reads_and_subscription_admission_filter_before_decoding() {
    use super::super::resources::{analysis_snapshot, subscribable_analysis_id};
    use veoveo_reason_mcp::{contract::AnalysisId, uris};

    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let reader = TaskRuntime::new(db.a.clone(), "reason", "resource-reader");
        let writer = TaskRuntime::new(db.b.clone(), "reason", "resource-writer");
        let draft = || CreateTask {
            task_id: TaskId::new(),
            owner: owner(),
            server: "reason".into(),
            task_type: const { veoveo_types::TaskTypeName::from_static("analyze_recording") },
            request: json!({}),
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
        for mutation in [
            "request.owner.data_labels = ['restricted']",
            "request.owner.principal_key = 'inconsistent'",
            "request.owner.profile = 'inconsistent'",
            "request.owner.tenant_key = 'inconsistent'",
            "owner = principal:other",
            "profile = profile:other",
            "tenant = tenant:other",
        ] {
            let task = writer.create(draft()).await.unwrap().snapshot;
            db.b.client()
                .query(format!(
                    "UPDATE ONLY $task SET {mutation}, request.input = NONE RETURN NONE;"
                ))
                .bind(("task", task_record_id(task.task_id)))
                .await
                .unwrap()
                .check()
                .unwrap();
            // The old unrestricted lookup cannot decode this persisted envelope.
            assert!(reader.get(&task.task_id.to_string()).await.is_err());
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
