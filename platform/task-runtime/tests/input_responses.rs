//! Caller input writes preserve SQL admission and atomic rollback on real stores.
#[path = "../../../testing/fixtures/store.rs"]
mod fixture;

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use serde_json::{Value, json};
use veoveo_platform_store::{PlatformStore, TaskInputRecord, task_record_id};
use veoveo_task_runtime::{
    CreateTask, OwnerTaskQuery, PrincipalKind, RecoveryClass, TaskError, TaskInputRequest,
    TaskInputSubmission, TaskOwner, TaskRuntime, TaskSnapshot, TaskTransition, update_durable_task,
};
use veoveo_types::{
    AccessSubject, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId, TaskId,
    TaskTypeName, TenantId, WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
};

const SERVER: &str = "input-test";
const OPERATION: TaskTypeName = TaskTypeName::from_static("interactive");

fn owner() -> TaskOwner {
    let principal = PrincipalId::parse("input-principal").unwrap();
    TaskOwner {
        principal_key: principal.to_string(),
        principal_kind: PrincipalKind::User,
        issuer: "https://input.example".into(),
        subject: "input-subject".into(),
        profile: "operator".into(),
        tenant_key: Some("input-tenant".into()),
        data_labels: BTreeSet::from(["internal".into()]),
        authority: InvocationAuthority {
            work_context: WorkContextId::parse("operations").unwrap(),
            tenant: TenantId::parse("input-tenant").unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::parse("r1").unwrap(),
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

fn query(runtime: &TaskRuntime) -> OwnerTaskQuery {
    runtime
        .for_owner(&owner())
        .of_type(OPERATION)
        .in_work_context()
        .unwrap()
}

async fn waiting(runtime: &TaskRuntime) -> TaskSnapshot {
    let task = runtime
        .create(CreateTask {
            task_id: TaskId::new(),
            owner: owner(),
            server: SERVER.into(),
            task_type: OPERATION,
            request: json!({"revision": 1}),
            recovery_class: RecoveryClass::Resume,
            idempotency_key: None,
            ttl_ms: Some(60_000),
            poll_interval_ms: None,
            retention_pins: BTreeSet::new(),
        })
        .await
        .unwrap()
        .snapshot;
    runtime
        .claim(task.task_id, Duration::from_secs(60))
        .await
        .unwrap();
    runtime
        .request_input(
            task.task_id,
            "choice",
            TaskInputRequest {
                method: "elicitation/create".into(),
                params: BTreeMap::from([("message".into(), json!("Choose a value"))]),
            },
        )
        .await
        .unwrap();
    runtime.get(task.task_id).await.unwrap().unwrap()
}

fn answers(value: i32) -> BTreeMap<String, BTreeMap<String, Value>> {
    BTreeMap::from([(
        "choice".into(),
        BTreeMap::from([
            ("action".into(), json!("accept")),
            ("content".into(), json!({"value": value})),
        ]),
    )])
}

fn update(task: TaskId) -> rmcp::model::UpdateTaskParams {
    serde_json::from_value(json!({"taskId": task, "inputResponses": answers(7)})).unwrap()
}

async fn input(store: &PlatformStore, task: TaskId) -> TaskInputRecord {
    let mut response = store
        .client()
        .query(include_str!(
            "queries/input_responses/input/statement_1.surql"
        ))
        .bind(("task", task_record_id(task)))
        .await
        .unwrap()
        .check()
        .unwrap();
    let mut rows: Vec<TaskInputRecord> = response.take(0).unwrap();
    assert_eq!(rows.len(), 1);
    rows.remove(0)
}

#[tokio::test]
async fn public_answers_roll_back_when_authority_or_status_changes_during_the_write() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), SERVER, "worker");
        let query = query(&runtime);
        for (assignment, sql) in [
("server case 1", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_01.surql")),
("tenant case 2", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_02.surql")),
("owner case 3", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_03.surql")),
("profile case 4", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_04.surql")),
("owner_context.principal_key case 5", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_05.surql")),
("owner_context.profile case 6", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_06.surql")),
("owner_context.tenant_key case 7", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_07.surql")),
("owner_context.data_labels case 8", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_08.surql")),
("work_context case 9", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_09.surql")),
("authority.context_key case 10", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_10.surql")),
("owner_context.authority.work_context case 11", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_11.surql")),
("owner_context.authority.tenant case 12", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_12.surql")),
("task_type case 13", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_13.surql")),
("status case 14", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_14.surql")),
("status case 15", include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/mutation_15.surql"))
] {
            let task = waiting(&runtime).await;
            let before_input = input(&db.b, task.task_id).await;
            // The event interleaves a change after the input write and before the
            // parent guard, without scheduling assumptions or production hooks.
            db.b.client()
                .query(sql)
                .await
                .unwrap()
                .check()
                .unwrap();
            let error = update_durable_task(&query, update(task.task_id))
                .await
                .unwrap_err();
            assert!(
                error.message.contains("task cannot accept input"),
                "{assignment}: {error}"
            );
            assert_eq!(
                query.get(task.task_id).await.unwrap(),
                Some(task.clone()),
                "{assignment}"
            );
            assert_eq!(
                input(&db.b, task.task_id).await,
                before_input,
                "{assignment}"
            );
            db.b.client()
                .query(include_str!("queries/input_responses/public_answers_roll_back_when_authority_or_status_changes_during_the_write/statement_1.surql"))
                .await
                .unwrap()
                .check()
                .unwrap();
            update_durable_task(&query, update(task.task_id))
                .await
                .unwrap();
            assert!(
                runtime
                    .outstanding_inputs(task.task_id)
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
    })
    .await
    .expect("transactional input admission exceeded 90 seconds");
}

#[tokio::test]
async fn denied_malformed_tasks_are_rejected_before_decode_and_input_mutation() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), SERVER, "worker");
        let query = query(&runtime);
        for (_assignment, sql) in [
("owner_context.data_labels case 1", include_str!("queries/input_responses/denied_malformed_tasks_are_rejected_before_decode_and_input_mutation/mutation_01.surql")),
("task_type case 2", include_str!("queries/input_responses/denied_malformed_tasks_are_rejected_before_decode_and_input_mutation/mutation_02.surql")),
("authority.context_key case 3", include_str!("queries/input_responses/denied_malformed_tasks_are_rejected_before_decode_and_input_mutation/mutation_03.surql"))
] {
            let task = waiting(&runtime).await;
            let before_input = input(&db.b, task.task_id).await;
            db.b.client()
                .query(sql)
                .bind(("task", task_record_id(task.task_id)))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(runtime.get(task.task_id).await.is_err());
            assert!(matches!(
                query.submit_input_responses(task.task_id, answers(7)).await,
                Err(TaskError::NotFound(_))
            ));
            assert_eq!(
                update_durable_task(&query, update(task.task_id))
                    .await
                    .unwrap_err()
                    .message,
                "unknown task id"
            );
            assert_eq!(input(&db.b, task.task_id).await, before_input);
        }
    })
    .await
    .expect("denied input admission exceeded 60 seconds");
}

#[tokio::test]
async fn competing_owner_answers_commit_once_and_replays_preserve_the_winner() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = fixture::TestDb::with_backend(fixture::StoreBackend::RocksDb).await;
        let first = TaskRuntime::new(db.a.clone(), SERVER, "first");
        let second = TaskRuntime::new(db.b.clone(), SERVER, "second");
        let task = waiting(&first).await;
        let first_query = query(&first);
        let second_query = query(&second);
        let (left, right) = tokio::join!(
            first_query.submit_input_responses(task.task_id, answers(7)),
            second_query.submit_input_responses(task.task_id, answers(8)),
        );
        let (left, right) = (left.unwrap(), right.unwrap());
        assert_eq!(left.accepted + right.accepted, 1);
        assert_eq!(left.ignored + right.ignored, 1);
        let committed = input(&db.b, task.task_id).await;
        let winner = if left.accepted == 1 { 7 } else { 8 };
        assert_eq!(
            committed.response.as_ref().unwrap().as_map(),
            &answers(winner)["choice"]
        );
        assert!(committed.responded_at.is_some());
        let snapshot = second_query.get(task.task_id).await.unwrap().unwrap();
        let mut replay = answers(99);
        replay.insert("unknown-key".into(), BTreeMap::new());
        assert_eq!(
            second_query
                .submit_input_responses(task.task_id, replay)
                .await
                .unwrap(),
            TaskInputSubmission {
                accepted: 0,
                ignored: 2
            }
        );
        assert_eq!(
            second_query.get(task.task_id).await.unwrap(),
            Some(snapshot)
        );
        assert_eq!(input(&db.b, task.task_id).await, committed);
    })
    .await
    .expect("competing scoped input answers exceeded 90 seconds");
}

#[tokio::test]
async fn cancelled_and_completed_tasks_reject_answers_without_changes() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), SERVER, "worker");
        let query = query(&runtime);
        for terminal in [false, true] {
            let task = waiting(&runtime).await;
            if terminal {
                runtime
                    .transition(
                        task.task_id,
                        TaskTransition::Succeeded {
                            result_uri: None,
                            message: "done".into(),
                            result: json!({"value": 42}),
                        },
                    )
                    .await
                    .unwrap();
            } else {
                query.cancel(task.task_id).await.unwrap();
            }
            let before_task = query.get(task.task_id).await.unwrap();
            let before_input = input(&db.b, task.task_id).await;
            assert!(matches!(
                query.submit_input_responses(task.task_id, answers(7)).await,
                Err(TaskError::InvalidTransition { .. })
            ));
            assert_eq!(query.get(task.task_id).await.unwrap(), before_task);
            assert_eq!(input(&db.b, task.task_id).await, before_input);
        }
    })
    .await
    .expect("terminal input rejection exceeded 60 seconds");
}

#[tokio::test]
async fn input_keys_and_parent_links_must_match_before_an_answer_is_written() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let db = fixture::TestDb::new().await;
        let runtime = TaskRuntime::new(db.a.clone(), SERVER, "worker");
        let query = query(&runtime);
        for (_assignment, sql) in [
("request_key case 1", include_str!("queries/input_responses/input_keys_and_parent_links_must_match_before_an_answer_is_written/mutation_01.surql")),
("task case 2", include_str!("queries/input_responses/input_keys_and_parent_links_must_match_before_an_answer_is_written/mutation_02.surql"))
] {
            let task = waiting(&runtime).await;
            let original = input(&db.b, task.task_id).await;
            let mut response =
                db.b.client()
                    .query(sql)
                    .bind(("input", original.id.clone()))
                    .bind(("foreign", task_record_id(TaskId::new())))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            let before: TaskInputRecord = response
                .take::<Option<TaskInputRecord>>(0)
                .unwrap()
                .unwrap();
            assert_eq!(
                query
                    .submit_input_responses(task.task_id, answers(7))
                    .await
                    .unwrap(),
                TaskInputSubmission {
                    accepted: 0,
                    ignored: 1
                }
            );
            let after: Option<TaskInputRecord> = db.b.client().select(original.id).await.unwrap();
            assert_eq!(after, Some(before));
            assert_eq!(query.get(task.task_id).await.unwrap(), Some(task));
        }
    })
    .await
    .expect("input identity admission exceeded 60 seconds");
}
