#[path = "support/commands.rs"]
mod commands;
mod support;

use std::time::Duration;
use uuid::Uuid;
use veoveo_computers::{AuthorityChanges, AuthorityInterest, ComputersStore, api::*};
use veoveo_platform_store::RecordId;
use veoveo_types::TaskId;

async fn wake(changes: &mut AuthorityChanges, interest: &AuthorityInterest) {
    tokio::time::timeout(Duration::from_secs(3), changes.next(interest))
        .await
        .expect("native authority wake deadline")
        .unwrap();
}
async fn drain(changes: &mut AuthorityChanges, interest: &AuthorityInterest) {
    for _ in 0..32 {
        match tokio::time::timeout(Duration::from_millis(50), changes.next(interest)).await {
            Err(_) => return,
            Ok(result) => result.unwrap(),
        }
    }
    panic!("authority source did not become idle");
}

#[tokio::test]
async fn native_authority_changes_cover_grants_directory_policy_tasks_and_journals() {
    let db = support::TestDb::new().await;
    let (a, b, owner, agent, computer) = support::automation::setup(&db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let claim = commands::queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
    let operation = a.command_for_claim(&claim).await.unwrap();
    let interest = AuthorityInterest::Command {
        computer,
        task: operation.task_id(),
        execution: operation.execution_id(),
    };
    let mut changes = a.authority_changes().await.unwrap();
    let clone = a.clone();
    let mut second = clone.authority_changes().await.unwrap();
    drain(&mut changes, &interest).await;
    drain(&mut second, &interest).await;

    // A journal-only deletion has no original payload and must still invalidate
    // its exact execution. The fixture owns this journal and never dispatches it.
    db.b.client()
        .query("DELETE ONLY $record;")
        .bind((
            "record",
            RecordId::new(
                "computer_execution",
                surrealdb::types::Uuid::from(operation.execution_id().into_uuid()),
            ),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    wake(&mut changes, &interest).await;
    wake(&mut second, &interest).await;

    let task_interest = AuthorityInterest::Command {
        computer: ComputerId::new(),
        task: operation.task_id(),
        execution: ExecutionId::new(),
    };
    drain(&mut changes, &task_interest).await;
    db.b.client()
        .query("UPDATE ONLY $task SET status = 'cancel_requested';")
        .bind((
            "task",
            veoveo_platform_store::task_record_id(operation.task_id()),
        ))
        .await
        .unwrap()
        .check()
        .unwrap();
    wake(&mut changes, &task_interest).await;

    drain(&mut changes, &interest).await;
    b.revoke_automation_grant(
        &owner,
        &RevokeAutomationGrantInput {
            computer_id: computer,
            grant_id: grant.grant_id,
        },
    )
    .await
    .unwrap();
    wake(&mut changes, &interest).await;
    assert!(
        a.authorize_automation_grant(
            &agent,
            computer,
            grant.grant_id,
            AutomationPermission::Execute
        )
        .await
        .is_err()
    );

    // Directory and installation grant policy changes invalidate even when no
    // Computer, grant or Task row changed.
    let unrelated = AuthorityInterest::Command {
        computer: ComputerId::new(),
        task: TaskId::new(),
        execution: ExecutionId::new(),
    };
    drain(&mut changes, &unrelated).await;
    let mut policy = support::automation::POLICY;
    policy.max_grants = 1;
    b.install_automation_grant_policy(Some(support::automation::POLICY), policy)
        .await
        .unwrap();
    wake(&mut changes, &unrelated).await;
    drain(&mut changes, &unrelated).await;
    db.b.client()
        .query("UPDATE principal SET enabled = false;")
        .await
        .unwrap()
        .check()
        .unwrap();
    wake(&mut changes, &unrelated).await;
    assert!(a.control_authority(&owner).await.is_err());
}

#[tokio::test]
async fn dropping_the_last_store_closes_its_native_source() {
    let db = support::TestDb::new().await;
    let store = ComputersStore::new(db.a.clone(), Uuid::now_v7()).unwrap();
    let clone = store.clone();
    let mut changes = store.authority_changes().await.unwrap();
    drop(store);
    changes.check().unwrap();
    drop(clone);
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        changes.next(&AuthorityInterest::All),
    )
    .await
    .unwrap();
    assert!(result.is_err());
}
