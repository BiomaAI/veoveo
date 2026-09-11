#[path = "support/files.rs"]
mod file_support;
mod support;
use file_support::*;
use std::time::{Duration, Instant};
use uuid::Uuid;
use veoveo_computer_execution::{FileFailure, FileReceipt};
use veoveo_computers::{ComputerActor, api::*, files::*, secrets::FileTransferPayload};
use veoveo_task_runtime::{TaskRetentionPin, TaskRuntime, TaskTransition};

async fn slots(db: &support::TestDb) -> usize {
    let mut read =
        db.a.client()
            .query("SELECT * FROM computer_execution_slot;")
            .await
            .unwrap()
            .check()
            .unwrap();
    read.take::<Vec<surrealdb::types::Value>>(0).unwrap().len()
}

#[tokio::test]
async fn one_dispatch_survives_a_lost_ticket_and_owner_authority_is_current() {
    let db = support::TestDb::new().await;
    let (a, b, _, _, computer) = setup(&db).await;
    let identity = support::browser::identity(&db, "alice").await;
    let owner = ComputerActor::from_verified(&identity).unwrap();
    let claim = queue(
        &db,
        &a,
        &owner,
        computer,
        None,
        &payload("private-transfer-file"),
    )
    .await;
    // Accepted work is independent of the admission family, while new work is denied.
    let family = identity
        .request_context
        .as_ref()
        .unwrap()
        .access_token
        .session_family
        .as_ref()
        .unwrap();
    db.a.client()
        .query("UPDATE $family SET revoked_at=time::now();")
        .bind((
            "family",
            veoveo_platform_store::gateway_refresh_family_record_id(
                Uuid::parse_str(family.as_str()).unwrap(),
            ),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        a.file_transfer_authority(&owner, computer, None)
            .await
            .is_err()
    );
    let key = keys();
    let attempts = futures::future::join_all((0..4).map(|i| {
        let (a, b, claim, key) = (&a, &b, &claim, &key);
        async move {
            if i % 2 == 0 { a } else { b }
                .begin_file_dispatch(claim, key)
                .await
        }
    }))
    .await;
    let mut successful = attempts.into_iter().filter_map(Result::ok);
    let ticket = successful.next().expect("one original dispatch");
    assert!(successful.next().is_none());
    assert!(ticket.authority_deadline() <= Instant::now() + Duration::from_secs(5));
    assert!(matches!(
        a.file_continuation(&claim, ticket.operation())
            .await
            .unwrap(),
        FileContinuation::Authorized(_)
    ));
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "file-worker");
    tasks.release_observation(&claim).await.unwrap();
    let successor = TaskRuntime::new(db.b.clone(), "computers", "successor")
        .claim_observation(&claim.snapshot.task_id.to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    assert!(b.begin_file_dispatch(&successor, &key).await.is_err());
    assert!(
        a.file_continuation(&claim, ticket.operation())
            .await
            .is_err()
    );
    let mut denied = control();
    denied.policies[0].rules[0]
        .tools
        .remove(&veoveo_mcp_contract::LocalToolName::new("transfer_file").unwrap());
    support::policy::install(&db.a, denied).await;
    assert!(matches!(
        b.file_continuation(&successor, ticket.operation())
            .await
            .unwrap(),
        FileContinuation::Interrupted(FileInterruption::AuthorityLost)
    ));
    drop(ticket);
    let containing = b
        .begin_file_containment(&successor, FileInterruption::ExecutionUnknown)
        .await
        .unwrap();
    assert_eq!(containing.stage(), FileTransferStage::Containing);
    assert_eq!(slots(&db).await, 1);
}

fn control() -> veoveo_mcp_contract::GatewayControlPlane {
    let mut control = support::automation::control();
    for name in ["transfer_file", "update_template"] {
        let name = veoveo_mcp_contract::LocalToolName::new(name).unwrap();
        control.servers[0].tools.push(name.clone());
        control.policies[0].rules[0].tools.insert(name);
    }
    control
}

#[tokio::test]
async fn known_import_export_and_rejection_release_the_slot_and_preserve_exact_results() {
    let db = support::TestDb::new().await;
    let (a, b, owner, _, computer) = setup(&db).await;
    for scenario in ["import", "export", "reject"] {
        let artifact = Uuid::now_v7();
        let payload = if scenario == "import" {
            FileTransferPayload::new(
                FileTransfer::Import {
                    artifact_id: artifact,
                    path: RetainedFilePath::try_from("private-import-file".to_owned()).unwrap(),
                },
                payload("unused").limits(),
            )
            .unwrap()
        } else {
            payload("private-transfer-file")
        };
        let claim = queue(&db, &a, &owner, computer, None, &payload).await;
        let ticket = a.begin_file_dispatch(&claim, &keys()).await.unwrap();
        let native = if scenario == "reject" {
            Err(FileFailure::DestinationExists)
        } else {
            Ok(FileReceipt {
                bytes: 32,
                sha256: [7; 32],
            })
        };
        let exit = ticket.observe_file_result(native).unwrap();
        // A cancellation that arrives after verified completion cannot erase a known effect.
        let tasks = TaskRuntime::new(db.a.clone(), "computers", "file-worker");
        tasks
            .cancel(&claim.snapshot.task_id.to_string())
            .await
            .unwrap();
        let completed = a
            .complete_file_result(&claim, exit, (scenario != "reject").then_some(artifact))
            .await
            .unwrap();
        assert_eq!(slots(&db).await, 0);
        assert_eq!(
            a.get(owner.owner(), computer).await.unwrap().phase,
            ComputerPhase::Ready
        );
        assert_eq!(
            b.file_for_claim(&claim).await.unwrap().outcome(),
            completed.outcome()
        );
        assert!(b.begin_file_dispatch(&claim, &keys()).await.is_err());
        assert!(
            b.begin_file_containment(&claim, FileInterruption::ExecutionUnknown)
                .await
                .is_err()
        );
        assert!(a.acknowledge_file_task(&completed).await.is_err());
        if scenario == "reject" {
            assert_eq!(
                completed.outcome(),
                Some(FileOutcome::Rejected(FileFailure::DestinationExists))
            );
            tasks
                .transition(
                    &completed.task_id().to_string(),
                    TaskTransition::Failed(veoveo_task_runtime::TaskFailure {
                        code: "destination_exists".into(),
                        message: "File was rejected".into(),
                        details: None,
                    }),
                )
                .await
                .unwrap();
        } else {
            let Some(FileOutcome::Completed(result)) = completed.outcome() else {
                panic!("expected a known result")
            };
            assert_eq!(result.artifact_id, artifact);
            assert_eq!(result.bytes, 32);
            assert_eq!(result.sha256, hex::encode([7; 32]));
            assert_eq!(
                result.result_uri,
                file_transfer_uri(completed.transfer_id())
            );
            assert_eq!(result.direction, payload.transfer().direction());
            tasks.transition(&completed.task_id().to_string(), TaskTransition::Succeeded {message:"File transferred".into(),result:serde_json::json!({"content":[],"structuredContent":result,"isError":false})}).await.unwrap();
            db.a.client()
                .query("UPDATE $task SET result.structuredContent.artifactId=$wrong;")
                .bind(("task", completed.task_id().record_id()))
                .bind(("wrong", Uuid::now_v7().to_string()))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(b.acknowledge_file_task(&completed).await.is_err());
            db.a.client()
                .query("UPDATE $task SET result.structuredContent.artifactId=$correct;")
                .bind(("task", completed.task_id().record_id()))
                .bind(("correct", artifact.to_string()))
                .await
                .unwrap()
                .check()
                .unwrap();
        }
        b.acknowledge_file_task(&completed).await.unwrap();
        assert_eq!(b.pending_file_transfers(None, 100).await.unwrap().len(), 1);
        let pin = TaskRetentionPin::new(format!(
            "computer-file-transfer/{}",
            completed.transfer_id()
        ))
        .unwrap();
        tasks
            .acknowledge_retention_pin(&completed.task_id().to_string(), &pin)
            .await
            .unwrap();
        assert!(
            b.pending_file_transfers(None, 100)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(b.ensure_file_task(&completed).await.is_err());
    }
    let mut read =
        db.a.client()
            .query("SELECT * FROM outbox_event WHERE event_type='computer.file_transfer_finished';")
            .await
            .unwrap()
            .check()
            .unwrap();
    let events: Vec<surrealdb::types::Value> = read.take(0).unwrap();
    assert_eq!(events.len(), 3);
    let public = serde_json::to_string(&events).unwrap();
    assert!(!public.contains("private-transfer-file"));
    assert!(!public.contains("private-import-file"));
    assert!(!public.contains("private-file-read-capability"));
}

#[tokio::test]
async fn uncertain_effects_and_substituted_import_occurrences_cannot_be_completed() {
    for scenario in [
        "unknown",
        "oversized",
        "artifact",
        "lease",
        "run",
        "containing",
    ] {
        let db = support::TestDb::new().await;
        let (a, _, owner, _, computer) = setup(&db).await;
        let source = Uuid::now_v7();
        let input = FileTransferPayload::new(
            FileTransfer::Import {
                artifact_id: source,
                path: RetainedFilePath::try_from("retained.txt".to_owned()).unwrap(),
            },
            payload("unused").limits(),
        )
        .unwrap();
        let claim = queue(&db, &a, &owner, computer, None, &input).await;
        let ticket = a.begin_file_dispatch(&claim, &keys()).await.unwrap();
        let observed = ticket.observe_file_result(if scenario == "unknown" {
            Err(FileFailure::CommitUnknown)
        } else {
            Ok(FileReceipt {
                bytes: if scenario == "oversized" { 1025 } else { 32 },
                sha256: [7; 32],
            })
        });
        if matches!(scenario, "unknown" | "oversized") {
            assert!(observed.is_err());
        } else {
            match scenario {
                "lease" => TaskRuntime::new(db.a.clone(), "computers", "file-worker")
                    .release_observation(&claim)
                    .await
                    .unwrap(),
                "run" => {
                    db.a.client()
                        .query("UPDATE $computer SET process_id='replacement-run';")
                        .bind(("computer", computer_record(computer)))
                        .await
                        .unwrap()
                        .check()
                        .unwrap();
                }
                "containing" => {
                    a.begin_file_containment(&claim, FileInterruption::ExecutionUnknown)
                        .await
                        .unwrap();
                }
                _ => {}
            }
            let id = if scenario == "artifact" {
                Uuid::now_v7()
            } else {
                source
            };
            assert!(
                a.complete_file_result(&claim, observed.unwrap(), Some(id))
                    .await
                    .is_err()
            );
        }
        assert_eq!(slots(&db).await, 1);
        assert!(a.file_for_claim(&claim).await.unwrap().outcome().is_none());
    }
}

#[tokio::test]
async fn preparation_expiry_and_cancelled_admission_never_dispatch() {
    let db = support::TestDb::new().await;
    let (a, _, owner, _, computer) = setup(&db).await;
    for cancel in [false, true] {
        let claim = queue(&db, &a, &owner, computer, None, &payload("retained.txt")).await;
        assert!(
            a.abort_queued_file(&claim, FileRefusal::PreparationExpired)
                .await
                .is_err()
        );
        if cancel {
            TaskRuntime::new(db.a.clone(), "computers", "client")
                .cancel(&claim.snapshot.task_id.to_string())
                .await
                .unwrap();
        } else {
            db.a.client().query("UPDATE computer_file_transfer SET created_at=time::now()-301s WHERE stage='queued';").await.unwrap().check().unwrap();
        }
        assert!(a.begin_file_dispatch(&claim, &keys()).await.is_err());
        let reason = if cancel {
            FileRefusal::CancelledBeforeDispatch
        } else {
            FileRefusal::PreparationExpired
        };
        let result = a.abort_queued_file(&claim, reason).await.unwrap();
        assert_eq!(result.outcome(), Some(FileOutcome::Undispatched(reason)));
        assert_eq!(slots(&db).await, 0);
    }
}

#[tokio::test]
async fn source_preparation_keeps_queued_authority_short_and_cancellable() {
    let db = support::TestDb::new().await;
    let (a, _, owner, agent, computer) = setup(&db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let claim = queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
    let prepared = a.prepare_file_transfer(&claim, &keys()).await.unwrap();
    assert_eq!(prepared.operation.stage(), FileTransferStage::Queued);
    assert_eq!(prepared.authority.maximum_bytes, 1024);
    assert!(prepared.authority.valid_until <= Instant::now() + Duration::from_secs(5));
    assert!(prepared.authority.execution_deadline <= Instant::now() + Duration::from_secs(300));
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "file-worker");
    tasks
        .cancel(&claim.snapshot.task_id.to_string())
        .await
        .unwrap();
    assert!(matches!(
        a.file_continuation(&claim, &prepared.operation)
            .await
            .unwrap(),
        FileContinuation::Interrupted(FileInterruption::Cancelled)
    ));
    assert!(a.prepare_file_transfer(&claim, &keys()).await.is_err());
    assert_eq!(slots(&db).await, 1);
    a.abort_queued_file(&claim, FileRefusal::CancelledBeforeDispatch)
        .await
        .unwrap();
    assert_eq!(
        a.get(owner.owner(), computer).await.unwrap().phase,
        ComputerPhase::Ready
    );
}
