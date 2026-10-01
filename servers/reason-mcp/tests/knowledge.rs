//! Native SQL admission. Inert completed outputs do not qualify GPU inference.
#![cfg(feature = "mcp")]

#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use serde_json::json;
use std::{collections::BTreeSet, time::Duration};
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_platform_store::{
    ArtifactGrantDraft, ArtifactGrantSubjectKind, ArtifactOccurrenceDraft, ArtifactReadScope,
    GrantPermission, InvocationAuthorityRecord, PlatformIdentity, PlatformStore, PrincipalKind,
    TaskResultRecord, task_record_id,
};
use veoveo_reason_mcp::{
    contract::{AnalysisId, AnalysisUri, AnalyzeRecordingOutput, ReasonTaskKind},
    knowledge::{FindingSelection, readable_findings},
};
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskRuntime};
use veoveo_types::{
    AccessSubject, InvocationAuthority, InvocationProvenance, TaskId, TaskTypeDefinition,
    WorkContextMembershipLevel, WorkContextOutputPolicy,
};

fn owner(name: &str, tenant: &str) -> TaskOwner {
    let principal = name.parse().unwrap();
    TaskOwner {
        principal_key: name.into(),
        principal_kind: PrincipalKind::User,
        issuer: "https://findings.example".into(),
        subject: name.into(),
        profile: "operator".into(),
        tenant_key: Some(tenant.into()),
        data_labels: BTreeSet::new(),
        authority: InvocationAuthority {
            work_context: "operations".parse().unwrap(),
            tenant: tenant.parse().unwrap(),
            membership: WorkContextMembershipLevel::Contributor,
            policy_revision: "findings-v1".parse().unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(principal),
                initial_grants: vec![],
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Automated,
        },
    }
}

async fn identity(store: &PlatformStore, name: &str, tenant: &str) -> PlatformIdentity {
    store
        .ensure_identity(
            tenant,
            name,
            "https://findings.example",
            name,
            PrincipalKind::User,
        )
        .await
        .unwrap()
}

fn scope(identity: &PlatformIdentity, context: Option<&str>) -> ArtifactReadScope {
    ArtifactReadScope::new(
        identity,
        [],
        BTreeSet::new(),
        context.map(|s| s.parse().unwrap()),
    )
    .unwrap()
}

async fn finding(
    store: &PlatformStore,
    tasks: &TaskRuntime,
    owner: TaskOwner,
) -> (AnalysisId, ArtifactId) {
    let id = AnalysisId::try_from(TaskId::new()).unwrap();
    let actor = identity(
        store,
        &owner.principal_key,
        owner.tenant_key.as_deref().unwrap(),
    )
    .await;
    tasks
        .create(CreateTask {
            task_id: id.task_id(),
            owner: owner.clone(),
            server: "reason".into(),
            task_type: ReasonTaskKind::AnalyzeRecording.name(),
            request: json!({}),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap();
    let artifact = ArtifactId::new();
    store
        .create_artifact_occurrence(ArtifactOccurrenceDraft {
            artifact_id: veoveo_platform_store::ArtifactId::from_uuid(artifact.as_uuid()),
            identity: actor.clone(),
            authority: InvocationAuthorityRecord {
                context_key: owner.authority.work_context.to_string(),
                membership: veoveo_platform_store::WorkContextMembershipLevel::Contributor,
                policy_revision: "findings-v1".into(),
                owner_kind: ArtifactGrantSubjectKind::Principal,
                owner_key: actor.principal_key.clone(),
                initial_grants: vec![],
                classification: None,
                data_labels: vec![],
                invocation_mode: veoveo_platform_store::InvocationMode::Automated,
                initiator_key: None,
                delegation_id: None,
            },
            owner: actor.principal_id.record_id(),
            initial_grants: vec![],
            sha256: "a".repeat(64),
            byte_len: 12,
            object_key: format!("fixture/{}/results", actor.tenant_key),
            media_type: "application/vnd.veoveo.reason-results+json".into(),
            filename: None,
            classification: String::new(),
            labels: vec![],
            retention_expires_at: None,
            metadata: [(
                "provenance".into(),
                json!({"kind":"reason_results", "analysis_id":id}),
            )]
            .into(),
        })
        .await
        .unwrap();
    let mut output: AnalyzeRecordingOutput =
        serde_json::from_str(include_str!("../testdata/analysis-output-v1.json")).unwrap();
    output.analysis_uri = AnalysisUri::new(id);
    output.results_artifact.artifact_uri = ArtifactUri::plane(artifact);
    let result = TaskResultRecord::new(json!({"structuredContent":output, "isError":false}));
    store
        .client()
        .query("UPDATE ONLY $task SET status = 'succeeded', result = $result RETURN NONE;")
        .bind(("task", task_record_id(id.task_id())))
        .bind(("result", result))
        .await
        .unwrap()
        .check()
        .unwrap();
    (id, artifact)
}

#[tokio::test]
async fn findings_apply_artifact_access_and_success_before_decode_and_pagination() {
    tokio::time::timeout(Duration::from_secs(240), async {
        let db = fixture::TestDb::new().await;
        let tasks = TaskRuntime::new(db.a.clone(), "reason", "findings-writer");
        let bob = identity(&db.a, "bob", "findings").await;
        let alice = identity(&db.a, "alice", "findings").await;
        let reader = scope(&bob, Some("operations"));
        let mut visible = Vec::new();
        for _ in 0..105 {
            visible.push(finding(&db.a, &tasks, owner("alice", "findings")).await);
        }
        for index in 0..120 {
            let mut author = owner("alice", if index % 4 == 3 { "foreign" } else { "findings" });
            if index % 4 == 1 { author.authority.work_context = "private".parse().unwrap(); }
            let (id, artifact) = finding(&db.a, &tasks, author).await;
            // Malformed output behind newer denied candidates must never decode.
            let mutation = match index % 4 {
                0 => "UPDATE $artifact SET classification = 'restricted' RETURN NONE;",
                2 => "UPDATE $artifact SET retention_expires_at = time::now() - 1s RETURN NONE;",
                _ => "RETURN NONE;",
            };
            db.b.client().query(format!("{mutation} UPDATE $task SET result.payload.structuredContent.model_uri = 17 RETURN NONE;"))
                .bind(("task",task_record_id(id.task_id())))
                .bind(("artifact",veoveo_platform_store::ArtifactId::from_uuid(artifact.as_uuid()).record_id()))
                .await.unwrap().check().unwrap();
            assert!(readable_findings(&db.a, &reader, FindingSelection::Member(id)).await.unwrap().is_empty());
        }
        // Wrong source provenance, unsuccessful Tasks and expired Tasks are also excluded.
        for mutation in [
            "UPDATE $artifact SET metadata.provenance.analysis_id = 'wrong-parent' RETURN NONE;",
            "UPDATE $artifact SET metadata.provenance.kind = 'reason_annotation_layer' RETURN NONE;",
            "UPDATE $task SET status = 'failed' RETURN NONE;",
            "UPDATE $task SET retention_expires_at = time::now() - 1s RETURN NONE;",
        ] {
            let (id, artifact) = finding(&db.a, &tasks, owner("alice", "findings")).await;
            db.b.client().query(format!("{mutation} UPDATE $task SET result.payload.structuredContent.model_uri = 17 RETURN NONE;"))
                .bind(("task",task_record_id(id.task_id())))
                .bind(("artifact",veoveo_platform_store::ArtifactId::from_uuid(artifact.as_uuid()).record_id()))
                .await.unwrap().check().unwrap();
        }
        let first = readable_findings(&db.a, &reader, FindingSelection::Page(None)).await.unwrap();
        assert_eq!(first.len(), 101, "one lookahead after SQL admission");
        let second = readable_findings(&db.b, &reader, FindingSelection::Page(Some(&first[99].position))).await.unwrap();
        assert_eq!(second.len(), 5);
        let actual: BTreeSet<_> = first[..100].iter().chain(&second).map(|r| r.position.analysis).collect();
        assert_eq!(actual, visible.iter().map(|(id,_)| *id).collect());
        assert_eq!(first[100].position, second[0].position);

        let (id, artifact) = visible[0];
        let private_reader = scope(&bob, None);
        assert!(readable_findings(&db.a, &private_reader, FindingSelection::Member(id)).await.unwrap().is_empty());
        let grant = ArtifactGrantDraft {
            artifact_id: veoveo_platform_store::ArtifactId::from_uuid(artifact.as_uuid()),
            subject: bob.principal_id.record_id(), subject_kind: ArtifactGrantSubjectKind::Principal,
            subject_key: bob.principal_key.clone(), permission: GrantPermission::Read, labels: vec![],
            expires_at: None, created_by: alice.principal_id,
        };
        db.b.upsert_artifact_grant(grant.clone()).await.unwrap();
        assert_eq!(readable_findings(&db.a, &private_reader, FindingSelection::Member(id)).await.unwrap()[0].results, artifact);
        assert!(tasks.for_owner(&owner("bob", "findings")).get(id.task_id()).await.unwrap().is_none(), "Artifact access must not grant Task control");
        db.b.remove_artifact_grant(veoveo_platform_store::ArtifactId::from_uuid(artifact.as_uuid()), ArtifactGrantSubjectKind::Principal, "bob").await.unwrap();
        assert!(readable_findings(&db.a, &private_reader, FindingSelection::Member(id)).await.unwrap().is_empty());
        db.b.upsert_artifact_grant(ArtifactGrantDraft { expires_at: Some(chrono::Utc::now() - chrono::TimeDelta::seconds(1)), ..grant }).await.unwrap();
        assert!(readable_findings(&db.a, &private_reader, FindingSelection::Member(id)).await.unwrap().is_empty());
        db.b.remove_artifact_grant(veoveo_platform_store::ArtifactId::from_uuid(artifact.as_uuid()), ArtifactGrantSubjectKind::Principal, "bob").await.unwrap();

        // Visible corruption is an error, never silently post-filtered from a page.
        db.b.client().query("UPDATE $task SET result.payload.structuredContent.model_uri = 17 RETURN NONE;")
            .bind(("task",task_record_id(id.task_id()))).await.unwrap().check().unwrap();
        assert!(readable_findings(&db.a, &reader, FindingSelection::Member(id)).await.is_err());
    }).await.expect("Reason findings SQL qualification exceeded 240 seconds");
}
