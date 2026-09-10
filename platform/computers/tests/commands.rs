mod support;
use std::collections::BTreeMap;
use uuid::Uuid;
use veoveo_computer_execution::ExecutionRequest;
use veoveo_computers::{
    ComputerActor, ComputerError, ComputersStore,
    api::*,
    automation_grants::AutomationAuthority,
    command_secrets::{CommandKeyRing, CommandPayload, CommandSealingKey},
};
use veoveo_platform_store::RecordId;
use veoveo_task_runtime::TaskRuntime;
use zeroize::Zeroizing;

fn keys() -> CommandKeyRing {
    CommandKeyRing::new(
        Uuid::from_u128(1),
        vec![CommandSealingKey::new(Uuid::from_u128(1), Zeroizing::new([19; 32])).unwrap()],
    )
    .unwrap()
}
fn payload(value: &str, seconds: u32) -> CommandPayload {
    CommandPayload::new(
        ExecutionRequest::new(
            vec!["/bin/printf".into(), value.into()],
            ".".into(),
            BTreeMap::from([("TOKEN".into(), "private-command-environment-fixture".into())]),
            b"private-command-stdin-fixture".to_vec(),
        )
        .unwrap(),
        AutomationExecutionLimits {
            maximum_seconds: seconds,
            maximum_output_bytes: 1024,
        },
    )
    .unwrap()
}
async fn permit(
    store: &ComputersStore,
    actor: &ComputerActor,
    computer: Uuid,
    grant: Uuid,
) -> AutomationAuthority {
    store
        .authorize_automation_grant(actor, computer, grant, AutomationPermission::Execute)
        .await
        .unwrap()
}
fn computer_record(id: Uuid) -> RecordId {
    RecordId::new("computer", surrealdb::types::Uuid::from(id))
}

#[tokio::test]
async fn racing_command_retry_has_one_private_slot_event_and_recoverable_task() {
    let db = support::TestDb::new().await;
    let (a, b, owner, agent, computer) = support::automation::setup(&db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let request = Uuid::now_v7();
    let keyring = keys();
    let first_payload = payload("private-command-argument-fixture", 30);
    let pa = permit(&a, &agent, computer, grant.grant_id).await;
    let pb = permit(&b, &agent, computer, grant.grant_id).await;
    let (left, right) = futures::join!(
        a.queue_command(&agent, pa, request, &first_payload, &keyring),
        b.queue_command(&agent, pb, request, &first_payload, &keyring),
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(left.execution_id(), right.execution_id());
    assert_eq!(left.actor(), *agent.owner());
    assert_ne!(left.actor(), *owner.owner());
    let recovered = b.pending_commands(None, 1).await.unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].execution_id(), left.execution_id());
    assert!(
        b.pending_commands(Some(left.execution_id()), 1)
            .await
            .unwrap()
            .is_empty()
    );
    let (l, r) = futures::join!(a.ensure_command_task(&left), b.ensure_command_task(&right));
    l.unwrap();
    r.unwrap();
    let task = TaskRuntime::new(db.a.clone(), "computers", "fixture")
        .get(&left.task_id().to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        task.request,
        serde_json::json!({"computerId":computer,"executionId":left.execution_id()})
    );
    assert_eq!(task.owner, *agent.owner());
    let mut rows = db.a.client().query("SELECT * FROM computer_execution; SELECT * FROM computer_execution_slot; SELECT * FROM outbox_event WHERE event_type = 'computer.execution_queued';").await.unwrap().check().unwrap();
    for i in 0..3 {
        let rows: Vec<surrealdb::types::Value> = rows.take(i).unwrap();
        assert_eq!(rows.len(), 1);
        let text = serde_json::to_string(&rows[0]).unwrap();
        for marker in [
            "private-command-argument-fixture",
            "private-command-environment-fixture",
            "private-command-stdin-fixture",
        ] {
            assert!(!text.contains(marker));
        }
    }
    let after = a.get(owner.owner(), computer).await.unwrap();
    assert!(after.active_operation.is_none());
    assert_eq!(after.phase, ComputerPhase::Ready);
    a.issue_browser_grant(&owner, computer).await.unwrap();
    for changed in [
        payload("changed", 30),
        payload("private-command-argument-fixture", 29),
    ] {
        let permit = permit(&a, &agent, computer, grant.grant_id).await;
        assert!(matches!(
            a.queue_command(&agent, permit, request, &changed, &keyring)
                .await,
            Err(ComputerError::RequestConflict)
        ));
    }
    let permit = permit(&a, &agent, computer, grant.grant_id).await;
    assert!(matches!(
        a.queue_command(&agent, permit, Uuid::now_v7(), &first_payload, &keyring)
            .await,
        Err(ComputerError::OperationBusy)
    ));
}

#[tokio::test]
async fn stale_authority_cannot_queue_after_revocation_policy_change_or_principal_disable() {
    let db = support::TestDb::new().await;
    let (a, _, owner, agent, computer) = support::automation::setup(&db).await;
    let keyring = keys();
    let payload = payload("private", 30);
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let authority = permit(&a, &agent, computer, grant.grant_id).await;
    a.revoke_automation_grant(
        &owner,
        &RevokeAutomationGrantInput {
            computer_id: computer,
            grant_id: grant.grant_id,
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        a.queue_command(&agent, authority, Uuid::now_v7(), &payload, &keyring)
            .await,
        Err(ComputerError::Forbidden)
    ));
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let authority = permit(&a, &agent, computer, grant.grant_id).await;
    let prior = a.automation_grant_policy().await.unwrap();
    let reduced = veoveo_computers::automation_grants::AutomationGrantPolicy {
        maximum_execution_seconds: 1,
        ..prior
    };
    a.install_automation_grant_policy(Some(prior), reduced)
        .await
        .unwrap();
    assert!(matches!(
        a.queue_command(&agent, authority, Uuid::now_v7(), &payload, &keyring)
            .await,
        Err(ComputerError::PolicyConflict)
    ));
    a.install_automation_grant_policy(Some(reduced), prior)
        .await
        .unwrap();
    let authority = permit(&a, &agent, computer, grant.grant_id).await;
    let principal = veoveo_platform_store::deterministic_principal_id(
        agent.owner().tenant_key(),
        &agent.owner().principal_key,
    )
    .unwrap()
    .record_id();
    db.a.client()
        .query("UPDATE $principal SET enabled=false;")
        .bind(("principal", principal))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        a.queue_command(&agent, authority, Uuid::now_v7(), &payload, &keyring)
            .await,
        Err(ComputerError::Forbidden)
    ));
    let mut response =
        db.a.client()
            .query("SELECT * FROM computer_execution_slot;")
            .await
            .unwrap()
            .check()
            .unwrap();
    let slots: Vec<surrealdb::types::Value> = response.take(0).unwrap();
    assert!(slots.is_empty());
}

#[tokio::test]
async fn command_authority_is_permission_specific_and_retained_slots_prevent_restart() {
    let db = support::TestDb::new().await;
    let (a, _, owner, agent, computer) = support::automation::setup(&db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let keyring = keys();
    let request = payload("private", 30);
    let authority = a
        .authorize_automation_grant(&agent, computer, grant.grant_id, AutomationPermission::Read)
        .await
        .unwrap();
    assert!(matches!(
        a.queue_command(&agent, authority, Uuid::now_v7(), &request, &keyring)
            .await,
        Err(ComputerError::Forbidden)
    ));
    let authority = permit(&a, &agent, computer, grant.grant_id).await;
    assert!(matches!(
        a.queue_command(&owner, authority, Uuid::now_v7(), &request, &keyring)
            .await,
        Err(ComputerError::Forbidden)
    ));
    let authority = permit(&a, &agent, computer, grant.grant_id).await;
    assert!(matches!(
        a.queue_command(
            &agent,
            authority,
            Uuid::now_v7(),
            &payload("private", 31),
            &keyring
        )
        .await,
        Err(ComputerError::InvalidInput)
    ));
    let authority = permit(&a, &agent, computer, grant.grant_id).await;
    a.queue_command(&agent, authority, Uuid::now_v7(), &request, &keyring)
        .await
        .unwrap();
    // Simulate an authoritative Stop in this isolated store; the pending command
    // remains protected until its own settlement releases the execution slot.
    db.a.client()
        .query("UPDATE $computer SET phase='stopped';")
        .bind(("computer", computer_record(computer)))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        a.queue_operation(owner, computer, Uuid::now_v7(), Action::Start)
            .await,
        Err(ComputerError::OperationBusy)
    ));
}
