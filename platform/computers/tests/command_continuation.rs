mod support;
use std::time::{Duration, Instant};
#[path = "support/commands.rs"]
mod command_support;
use command_support::*;
use veoveo_computers::{api::*, commands::*};
use veoveo_task_runtime::TaskRuntime;

#[tokio::test]
async fn live_authority_is_short_and_rechecks_policy_under_the_current_task_lease() {
    let db = support::TestDb::new().await;
    let (a, _, owner, agent, computer) = support::automation::setup(&db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let mut claim = queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
    let dispatch = a.begin_command_dispatch(&claim, &keys()).await.unwrap();
    assert!(
        dispatch
            .authority_deadline()
            .saturating_duration_since(Instant::now())
            <= Duration::from_secs(5)
    );
    let CommandContinuation::Authorized(first) = a
        .command_continuation(&claim, dispatch.operation())
        .await
        .unwrap()
    else {
        panic!("active grant should authorize continuation");
    };
    assert!(first.valid_until > Instant::now());
    assert!(first.valid_until <= Instant::now() + Duration::from_secs(5));
    assert_eq!(first.maximum_output_bytes, 1024);
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
    let CommandContinuation::Authorized(reduced) = a
        .command_continuation(&claim, dispatch.operation())
        .await
        .unwrap()
    else {
        panic!("reduced live grant");
    };
    assert_eq!(reduced.maximum_output_bytes, 512);
    assert!(reduced.execution_deadline < first.execution_deadline);
    assert!(reduced.execution_deadline <= Instant::now() + Duration::from_secs(7));
    assert_eq!(reduced.deadline_reason, CommandInterruption::AuthorityLost);
    let tasks = TaskRuntime::new(db.a.clone(), "computers", "command-worker");
    let renewed = tasks
        .renew_lease(&claim.snapshot.task_id.to_string(), Duration::from_secs(60))
        .await
        .unwrap();
    assert!(
        a.command_continuation(&claim, dispatch.operation())
            .await
            .is_err()
    );
    claim.lease_expires_at = renewed.lease_expires_at.unwrap();
    claim.snapshot = renewed;
    // The active worker already holds the authenticated request. Refresh compares
    // immutable metadata without downloading or decrypting command ciphertext.
    db.a.client()
        .query("UPDATE computer_execution SET sealed.ciphertext='not-read-by-active-refresh';")
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        a.command_continuation(&claim, dispatch.operation())
            .await
            .unwrap(),
        CommandContinuation::Authorized(_)
    ));
    tasks
        .cancel(&claim.snapshot.task_id.to_string())
        .await
        .unwrap();
    assert!(matches!(
        a.command_continuation(&claim, dispatch.operation())
            .await
            .unwrap(),
        CommandContinuation::Interrupted(CommandInterruption::Cancelled)
    ));
}

#[tokio::test]
async fn revocation_and_native_run_replacement_interrupt_continuation_without_another_dispatch() {
    for scenario in ["revoke", "run", "lease", "journal"] {
        let db = support::TestDb::new().await;
        let (a, _, owner, agent, computer) = support::automation::setup(&db).await;
        let grant = a
            .issue_automation_grant(&owner, &support::automation::input(computer))
            .await
            .unwrap();
        let claim = queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
        let dispatch = a.begin_command_dispatch(&claim, &keys()).await.unwrap();
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
                TaskRuntime::new(db.a.clone(), "computers", "command-worker")
                    .release_observation(&claim)
                    .await
                    .unwrap();
            }
            "journal" => {
                db.a.client()
                    .query(
                        "UPDATE computer_execution SET binding.process_id='substituted-binding';",
                    )
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            }
            _ => unreachable!(),
        }
        let continuation = a.command_continuation(&claim, dispatch.operation()).await;
        match scenario {
            "revoke" => assert!(matches!(
                continuation,
                Ok(CommandContinuation::Interrupted(
                    CommandInterruption::AuthorityLost
                ))
            )),
            "run" => assert!(matches!(
                continuation,
                Ok(CommandContinuation::Interrupted(
                    CommandInterruption::ExecutionUnknown
                ))
            )),
            _ => assert!(continuation.is_err()),
        }
        let mut response = db.a.client().query("SELECT * FROM outbox_event WHERE event_type='computer.execution_dispatched'; SELECT * FROM computer_execution_slot;").await.unwrap().check().unwrap();
        let events: Vec<surrealdb::types::Value> = response.take(0).unwrap();
        let slots: Vec<surrealdb::types::Value> = response.take(1).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(slots.len(), 1);
    }
}
