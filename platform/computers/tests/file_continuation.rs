mod support;
use std::time::{Duration, Instant};
#[path = "support/files.rs"]
mod file_support;
use file_support::*;
use veoveo_computers::{api::*, files::*};
use veoveo_task_runtime::TaskRuntime;

#[tokio::test]
async fn live_authority_is_short_and_rechecks_policy_under_the_current_task_lease() {
    let db = support::TestDb::new().await;
    let (a, _, owner, agent, computer) = setup(&db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let mut claim = queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
    let dispatch = a.begin_file_dispatch(&claim, &keys()).await.unwrap();
    assert!(
        dispatch
            .authority_deadline()
            .saturating_duration_since(Instant::now())
            <= Duration::from_secs(5)
    );
    let FileContinuation::Authorized(first) = a
        .file_continuation(&claim, dispatch.operation())
        .await
        .unwrap()
    else {
        panic!("active grant should authorize continuation");
    };
    assert!(first.valid_until > Instant::now());
    assert!(first.valid_until <= Instant::now() + Duration::from_secs(5));
    assert_eq!(first.maximum_bytes, 1024);
    let policy = a.automation_grant_policy().await.unwrap();
    a.install_automation_grant_policy(
        Some(policy),
        veoveo_computers::automation_grants::AutomationGrantPolicy {
            maximum_execution_seconds: 7,
            maximum_output_bytes: 512,
            ..policy
        },
    )
    .await
    .unwrap();
    let FileContinuation::Authorized(reduced) = a
        .file_continuation(&claim, dispatch.operation())
        .await
        .unwrap()
    else {
        panic!("reduced live grant");
    };
    assert_eq!(reduced.maximum_bytes, 512);
    assert!(reduced.execution_deadline < first.execution_deadline);
    assert!(reduced.execution_deadline <= Instant::now() + Duration::from_secs(7));
    assert_eq!(reduced.deadline_reason, FileInterruption::AuthorityLost);
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "file-worker");
    let renewed = tasks
        .renew_lease(&claim.snapshot.task_id.to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    assert!(
        a.file_continuation(&claim, dispatch.operation())
            .await
            .is_err()
    );
    claim.lease_expires_at = renewed.lease_expires_at.unwrap();
    claim.snapshot = renewed;
    // The active worker already holds the authenticated request. Refresh compares
    // immutable metadata without downloading or decrypting file ciphertext.
    db.a.client()
        .query("UPDATE computer_file_transfer SET sealed.ciphertext='not-read-by-active-refresh';")
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        a.file_continuation(&claim, dispatch.operation())
            .await
            .unwrap(),
        FileContinuation::Authorized(_)
    ));
    tasks
        .cancel(&claim.snapshot.task_id.to_string())
        .await
        .unwrap();
    assert!(matches!(
        a.file_continuation(&claim, dispatch.operation())
            .await
            .unwrap(),
        FileContinuation::Interrupted(FileInterruption::Cancelled)
    ));
}

#[tokio::test]
async fn revocation_and_native_run_replacement_interrupt_continuation_without_another_dispatch() {
    for scenario in ["revoke", "run", "lease", "journal"] {
        let db = support::TestDb::new().await;
        let (a, _, owner, agent, computer) = setup(&db).await;
        let grant = a
            .issue_automation_grant(&owner, &support::automation::input(computer))
            .await
            .unwrap();
        let claim = queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
        let dispatch = a.begin_file_dispatch(&claim, &keys()).await.unwrap();
        match scenario {
            "revoke" => {
                a.revoke_automation_grant(
                    &owner,
                    &RevokeAutomationGrantInput {
                        computer_id: computer,
                        grant_id: grant.grant_id,
                    },
                )
                .await
                .unwrap();
            }
            "run" => {
                db.a.client()
                    .query("UPDATE $computer SET process_id='replacement-run';")
                    .bind(("computer", computer_record(computer)))
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            }
            "lease" => {
                TaskRuntime::new(db.a.clone(), "computers", "file-worker")
                    .release_observation(&claim)
                    .await
                    .unwrap();
            }
            "journal" => {
                db.a.client()
                    .query(
                        "UPDATE computer_file_transfer SET binding.process_id='substituted-binding';",
                    )
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let continuation = a.file_continuation(&claim, dispatch.operation()).await;
        match scenario {
            "revoke" => assert!(matches!(
                continuation,
                Ok(FileContinuation::Interrupted(
                    FileInterruption::AuthorityLost
                ))
            )),
            "run" => assert!(matches!(
                continuation,
                Ok(FileContinuation::Interrupted(
                    FileInterruption::ExecutionUnknown
                ))
            )),
            _ => assert!(continuation.is_err()),
        }
        let mut response = db.a.client().query("SELECT * FROM outbox_event WHERE event_type='computer.file_transfer_dispatched'; SELECT * FROM computer_execution_slot;").await.unwrap().check().unwrap();
        let events: Vec<surrealdb::types::Value> = response.take(0).unwrap();
        let slots: Vec<surrealdb::types::Value> = response.take(1).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(slots.len(), 1);
    }
}
