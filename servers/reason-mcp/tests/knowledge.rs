//! Native SQL admission. Inert completed outputs do not qualify GPU inference.
#![cfg(feature = "mcp")]

#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use std::{collections::BTreeSet, time::Duration};
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_platform_store::{
    ArtifactGrantDraft, ArtifactGrantSubjectKind, ArtifactOccurrenceDraft, ArtifactReadScope,
    GrantPermission, InvocationAuthorityRecord, PlatformIdentity, PlatformStore, PrincipalKind,
    task_record_id,
};
use veoveo_reason_mcp::{
    contract::{AnalysisId, AnalysisUri, AnalyzeRecordingOutput, ReasonTaskKind},
    knowledge::{FindingSelection, observe, readable_findings},
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
            request: serde_json::from_str(include_str!("../testdata/task-request.json")).unwrap(),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: None,
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap();
    let mut output: AnalyzeRecordingOutput =
        serde_json::from_str(include_str!("../testdata/analysis-output-v1.json")).unwrap();
    output.analysis_uri = AnalysisUri::new(id);
    let mut metadata = output.results_artifact.metadata.clone();
    metadata["provenance"]["analysis_id"] = id.to_string().into();
    let artifact = ArtifactId::new();
    let receipt = store
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
            metadata: serde_json::from_value(metadata.clone()).unwrap(),
        })
        .await
        .unwrap();
    output.results_artifact.artifact_uri = ArtifactUri::plane(artifact);
    output.results_artifact.created_at = receipt.occurrence.created_at;
    output.results_artifact.metadata = serde_json::to_value(&receipt.occurrence.metadata).unwrap();
    tasks
        .claim(id.task_id(), Duration::from_secs(30))
        .await
        .unwrap();
    tasks
        .transition(
            id.task_id(),
            veoveo_task_runtime::mcp_task_completion(
                "fixture finding",
                veoveo_reason_mcp::task_product::analysis_tool_result(output).unwrap(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    (id, artifact)
}

#[tokio::test]
async fn findings_apply_artifact_access_and_success_before_decode_and_pagination() {
    tokio::time::timeout(Duration::from_secs(240), async {
        let db = fixture::TestDb::new().await;
        fixture::module_lanes::install(&db.a, vec![veoveo_reason_mcp::schema::module_setup(fixture::module_lanes::execution("reason").unwrap()).unwrap()]).await.unwrap();
        let tasks = veoveo_reason_mcp::task_lookup::bind(TaskRuntime::new(db.a.clone(), "reason", "findings-writer")).unwrap();
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
                0 => include_str!("queries/knowledge/findings_apply_artifact_access_and_success_before_decode_and_pagination.surql"),
                2 => include_str!("queries/knowledge/findings_apply_artifact_access_and_success_before_decode_and_pagination_2.surql"),
                _ => include_str!("queries/knowledge/findings_apply_artifact_access_and_success_before_decode_and_pagination_3.surql"),
            };
            db.b.client().query(mutation)
                .bind(("task",task_record_id(id.task_id())))
                .bind(("artifact",veoveo_platform_store::ArtifactId::from_uuid(artifact.as_uuid()).record_id()))
                .await.unwrap().check().unwrap();
            assert!(readable_findings(&db.a, &reader, FindingSelection::Member(id)).await.unwrap().is_empty());
        }
        // Wrong source provenance, unsuccessful Tasks and expired Tasks are also excluded.
        for mutation in [
            include_str!("queries/knowledge/findings_apply_artifact_access_and_success_before_decode_and_pagination_4.surql"),
            include_str!("queries/knowledge/findings_apply_artifact_access_and_success_before_decode_and_pagination_5.surql"),
            include_str!("queries/knowledge/findings_apply_artifact_access_and_success_before_decode_and_pagination_6.surql"),
            include_str!("queries/knowledge/findings_apply_artifact_access_and_success_before_decode_and_pagination_7.surql"),
        ] {
            let (id, artifact) = finding(&db.a, &tasks, owner("alice", "findings")).await;
            db.b.client().query(mutation)
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

        let baseline = observe::snapshot(&db.a, &reader, None).await.unwrap();
        assert!(baseline.present);
        assert_eq!(baseline.fingerprint, observe::snapshot(&db.b, &reader, None).await.unwrap().fingerprint);
        let (id, artifact) = visible[0];
        assert!(!first[..100].iter().any(|r| r.position.analysis == id), "mutation exercises a member beyond the first page");
        let completed = readable_findings(&db.a, &reader, FindingSelection::Complete(&id.to_string())).await.unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].position.analysis, id);
        assert!(readable_findings(&db.a, &reader, FindingSelection::Complete("not-an-id")).await.unwrap().is_empty());
        let private_reader = scope(&bob, None);
        assert!(readable_findings(&db.a, &private_reader, FindingSelection::Member(id)).await.unwrap().is_empty());
        let grant = ArtifactGrantDraft {
            artifact_id: veoveo_platform_store::ArtifactId::from_uuid(artifact.as_uuid()),
            subject: bob.principal_id.record_id(), subject_kind: ArtifactGrantSubjectKind::Principal,
            subject_key: bob.principal_key.clone(), permission: GrantPermission::Read, labels: vec![],
            expires_at: None, created_by: alice.principal_id,
        };
        let changes = observe::FindingChanges::new(db.a.clone());
        let mut updates = changes.subscribe();
        tokio::time::timeout(Duration::from_secs(15), async {
            while updates.borrow_and_update().is_none() { updates.changed().await.unwrap(); }
        }).await.unwrap();
        db.b.upsert_artifact_grant(grant.clone()).await.unwrap();
        tokio::time::timeout(Duration::from_secs(15), updates.changed()).await.unwrap().unwrap();
        let shared = observe::snapshot(&db.a, &reader, None).await.unwrap();
        assert_ne!(baseline.fingerprint, shared.fingerprint, "grant changes invalidate the entire collection");
        assert!(observe::snapshot(&db.a, &private_reader, Some(id)).await.unwrap().present);
        assert_eq!(readable_findings(&db.a, &private_reader, FindingSelection::Member(id)).await.unwrap()[0].results, artifact);
        assert!(tasks.for_owner(&owner("bob", "findings")).get(id.task_id()).await.unwrap().is_none(), "Artifact access must not grant Task control");
        db.b.remove_artifact_grant(veoveo_platform_store::ArtifactId::from_uuid(artifact.as_uuid()), ArtifactGrantSubjectKind::Principal, "bob").await.unwrap();
        assert!(readable_findings(&db.a, &private_reader, FindingSelection::Member(id)).await.unwrap().is_empty());
        db.b.upsert_artifact_grant(ArtifactGrantDraft { expires_at: Some(chrono::Utc::now() - chrono::TimeDelta::seconds(1)), ..grant }).await.unwrap();
        assert!(readable_findings(&db.a, &private_reader, FindingSelection::Member(id)).await.unwrap().is_empty());
        db.b.remove_artifact_grant(veoveo_platform_store::ArtifactId::from_uuid(artifact.as_uuid()), ArtifactGrantSubjectKind::Principal, "bob").await.unwrap();

        assert!(!observe::snapshot(&db.a, &private_reader, Some(id)).await.unwrap().present);
        let deadline = chrono::Utc::now() + chrono::TimeDelta::hours(1);
        db.b.client().query(include_str!("queries/knowledge/findings_apply_artifact_access_and_success_before_decode_and_pagination_8.surql"))
            .bind(("task", task_record_id(id.task_id()))).bind(("deadline", deadline)).await.unwrap().check().unwrap();
        assert_eq!(observe::snapshot(&db.a, &reader, Some(id)).await.unwrap().deadline, Some(deadline));
        let hidden_before = observe::snapshot(&db.a, &private_reader, None).await.unwrap();
        db.b.client().query(include_str!("queries/knowledge/findings_apply_artifact_access_and_success_before_decode_and_pagination_9.surql"))
            .bind(("task", task_record_id(id.task_id()))).await.unwrap().check().unwrap();
        assert_eq!(hidden_before.fingerprint, observe::snapshot(&db.a, &private_reader, None).await.unwrap().fingerprint);
        drop(changes);

        // Visible corruption is an error, never silently post-filtered from a page.
        db.b.client().query(include_str!("queries/knowledge/findings_apply_artifact_access_and_success_before_decode_and_pagination_10.surql"))
            .bind(("task",task_record_id(id.task_id()))).await.unwrap().check().unwrap();
        assert!(readable_findings(&db.a, &reader, FindingSelection::Member(id)).await.is_err());
    }).await.expect("Reason findings SQL qualification exceeded 240 seconds");
}

#[tokio::test]
async fn lookup_admission_rejects_closed_corruption_and_observes_owner_changes() {
    tokio::time::timeout(Duration::from_secs(180), async {
        use surrealdb::types::{RecordId, SurrealValue, Value};
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
            "lookup-writer",
        ))
        .unwrap();
        let actor = identity(&db.a, "alice", "findings").await;
        let reader = scope(&actor, Some("operations"));
        let (id, _) = finding(&db.a, &tasks, owner("alice", "findings")).await;
        let lookup = RecordId::new("reason_analysis", id.task_id().to_string());
        let before: Value =
            db.a.client()
                .query(include_str!("queries/knowledge/lookup_controls/read.surql"))
                .bind(("lookup", lookup.clone()))
                .await
                .unwrap()
                .check()
                .unwrap()
                .take(0)
                .unwrap();
        for case in 0..12 {
            let Value::Object(mut row) = before.clone() else {
                panic!("lookup missing");
            };
            let Value::Object(identity) = row.get_mut("identity").unwrap() else {
                panic!("identity missing");
            };
            match case {
                0 => {
                    identity.insert("unexpected", true.into_value());
                }
                1 => {
                    identity.remove("tenant");
                }
                2..=7 => {
                    let Value::Object(settlement) = row.get_mut("settlement").unwrap() else {
                        panic!("settlement missing");
                    };
                    match case {
                        2 => {
                            settlement.remove("completed_at");
                        }
                        3 => {
                            let Value::Object(finding) = settlement.get_mut("finding").unwrap()
                            else {
                                panic!("finding missing");
                            };
                            finding.insert("unexpected", true.into_value());
                        }
                        4 => {
                            settlement.insert("status", Value::None);
                        }
                        5 => {
                            settlement.insert("completed_at", Value::None);
                        }
                        6 => {
                            settlement.insert("status", "failed".into_value());
                        }
                        7 => {
                            settlement.insert("status", "cancelled".into_value());
                        }
                        _ => unreachable!(),
                    }
                }
                8..=11 => {
                    let Value::Object(settlement) = row.get_mut("settlement").unwrap() else {
                        panic!("settlement missing");
                    };
                    if case == 8 {
                        let Value::Object(metadata) =
                            settlement.get_mut("expected_metadata").unwrap()
                        else {
                            panic!("metadata missing");
                        };
                        let Value::Object(provenance) = metadata.get_mut("provenance").unwrap()
                        else {
                            panic!("provenance missing");
                        };
                        provenance.remove("pipeline_id");
                    } else {
                        let Value::Object(finding) = settlement.get_mut("finding").unwrap() else {
                            panic!("finding missing");
                        };
                        let Value::Object(answer) = finding.get_mut("answer").unwrap() else {
                            panic!("answer missing");
                        };
                        if case == 11 {
                            answer.remove("events");
                        } else {
                            let Value::Array(events) = answer.get_mut("events").unwrap() else {
                                panic!("events missing");
                            };
                            let Value::Object(event) = events.first_mut().unwrap() else {
                                panic!("event missing");
                            };
                            let Value::Object(nested) = event
                                .get_mut(if case == 9 { "label" } else { "range" })
                                .unwrap()
                            else {
                                panic!("event detail missing");
                            };
                            nested.remove(if case == 9 { "text" } else { "start" });
                        }
                    }
                }
                _ => unreachable!(),
            }
            assert!(
                db.b.client()
                    .query(include_str!(
                        "queries/knowledge/lookup_controls/replace.surql"
                    ))
                    .bind(("lookup", lookup.clone()))
                    .bind(("row", Value::Object(row)))
                    .await
                    .unwrap()
                    .check()
                    .is_err(),
                "malformed controlled lookup case {case} was admitted"
            );
            let after: Value =
                db.a.client()
                    .query(include_str!("queries/knowledge/lookup_controls/read.surql"))
                    .bind(("lookup", lookup.clone()))
                    .await
                    .unwrap()
                    .check()
                    .unwrap()
                    .take(0)
                    .unwrap();
            assert_eq!(before, after);
        }
        let changes = observe::FindingChanges::new(db.a.clone());
        let mut updates = changes.subscribe();
        tokio::time::timeout(Duration::from_secs(15), async {
            while updates.borrow_and_update().is_none() {
                updates.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        updates.borrow_and_update();
        db.b.client()
            .query(include_str!(
                "queries/knowledge/lookup_controls/copied_finding.surql"
            ))
            .bind(("lookup", lookup.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(15), updates.changed())
            .await
            .unwrap()
            .unwrap();
        assert!(
            readable_findings(&db.a, &reader, FindingSelection::Member(id))
                .await
                .is_err(),
            "copied finding differs from complete retained result"
        );
        db.b.client()
            .query(include_str!(
                "queries/knowledge/lookup_controls/replace.surql"
            ))
            .bind(("lookup", lookup.clone()))
            .bind(("row", before.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            readable_findings(&db.a, &reader, FindingSelection::Member(id))
                .await
                .unwrap()
                .len(),
            1
        );
        tokio::time::timeout(Duration::from_secs(15), updates.changed())
            .await
            .unwrap()
            .unwrap();
        updates.borrow_and_update();
        db.b.client()
            .query(include_str!(
                "queries/knowledge/lookup_controls/delete.surql"
            ))
            .bind(("lookup", lookup.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(15), updates.changed())
            .await
            .unwrap()
            .unwrap();
        assert!(
            readable_findings(&db.a, &reader, FindingSelection::Member(id))
                .await
                .unwrap()
                .is_empty()
        );
        drop(changes);
        let reconnected = db.connect_at(db.a.config().endpoint().as_str()).await;
        let reconnected_changes = observe::FindingChanges::new(reconnected.clone());
        let mut updates = reconnected_changes.subscribe();
        tokio::time::timeout(Duration::from_secs(15), async {
            while updates.borrow_and_update().is_none() {
                updates.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        updates.borrow_and_update();
        db.b.client()
            .query(include_str!(
                "queries/knowledge/lookup_controls/create.surql"
            ))
            .bind(("lookup", lookup))
            .bind(("row", before))
            .await
            .unwrap()
            .check()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(15), updates.changed())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            readable_findings(&reconnected, &reader, FindingSelection::Member(id))
                .await
                .unwrap()
                .len(),
            1
        );
        drop(reconnected_changes);
    })
    .await
    .expect("Reason lookup admission exceeded 180 seconds");
}
