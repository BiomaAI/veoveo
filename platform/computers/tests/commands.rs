#[path = "support/commands.rs"]
mod command_support;
mod support;
use command_support::*;
use uuid::Uuid;
use veoveo_computers::{ComputerActor, ComputerError, api::*};
use veoveo_task_runtime::TaskRuntime;

#[tokio::test]
async fn replacement_command_identity_survives_replicas_and_blocks_a_changed_instance() {
    let db = support::TestDb::new().await;
    let (a, b, owner, agent, computer) = support::automation::setup(&db).await;
    let instance = Uuid::now_v7();
    db.a.client()
        .query("UPDATE $computer SET replacement_instance_id=$instance;")
        .bind(("computer", computer_record(computer)))
        .bind(("instance", instance))
        .await
        .unwrap()
        .check()
        .unwrap();
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let claim = queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
    assert_eq!(
        b.command_for_claim(&claim)
            .await
            .unwrap()
            .binding()
            .instance_id(),
        instance
    );
    db.a.client()
        .query("UPDATE $computer SET replacement_instance_id=$instance;")
        .bind(("computer", computer_record(computer)))
        .bind(("instance", Uuid::now_v7()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(b.begin_command_dispatch(&claim, &keys()).await.is_err());
    db.a.client()
        .query("UPDATE $computer SET replacement_instance_id=$instance;")
        .bind(("computer", computer_record(computer)))
        .bind(("instance", instance))
        .await
        .unwrap()
        .check()
        .unwrap();
    let ticket = b.begin_command_dispatch(&claim, &keys()).await.unwrap();
    assert_eq!(ticket.binding().instance_id(), instance);
}
#[tokio::test]
async fn one_dispatch_survives_competing_workers_and_lost_ticket_without_replay() {
    use veoveo_computers::commands::CommandStage;
    let db = support::TestDb::new().await;
    let (a, b, owner, agent, computer) = support::automation::setup(&db).await;
    let grant = a
        .issue_automation_grant(&owner, &support::automation::input(computer))
        .await
        .unwrap();
    let claim = queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
    let keys = keys();
    let (left, right) = futures::join!(
        a.begin_command_dispatch(&claim, &keys),
        b.begin_command_dispatch(&claim, &keys)
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    let ticket = match (left, right) {
        (Ok(ticket), _) | (_, Ok(ticket)) => ticket,
        _ => panic!("neither dispatch committed"),
    };
    assert_eq!(ticket.operation().stage(), CommandStage::Dispatched);
    assert_eq!(ticket.operation().actor(), *agent.owner());
    assert!(ticket.authority_deadline() > std::time::Instant::now());
    assert!(ticket.execution_deadline() > std::time::Instant::now());
    assert_eq!(
        ticket.limits().on_interruption,
        AutomationInterruption::StopComputer
    );
    drop(ticket); // A lost commit receipt or worker crash cannot reconstruct a dispatch ticket.
    let runtime = TaskRuntime::new(db.a.clone(), "computers", "command-worker");
    runtime.release_observation(&claim).await.unwrap();
    let successor = TaskRuntime::new(db.b.clone(), "computers", "successor")
        .claim_observation(
            &claim.snapshot.task_id.to_string(),
            std::time::Duration::from_secs(60),
        )
        .await
        .unwrap();
    assert!(matches!(
        b.begin_command_dispatch(&successor, &keys).await,
        Err(ComputerError::StateConflict)
    ));
    assert_eq!(
        b.command_for_claim(&successor).await.unwrap().stage(),
        CommandStage::Dispatched
    );
    let mut response = db.a.client().query("SELECT * FROM outbox_event WHERE event_type = 'computer.execution_dispatched'; SELECT * FROM computer_execution_slot;").await.unwrap().check().unwrap();
    let events: Vec<surrealdb::types::Value> = response.take(0).unwrap();
    assert_eq!(events.len(), 1);
    let text = serde_json::to_string(&events[0]).unwrap();
    for private in [
        "private-dispatch-argument",
        "private-command-environment-fixture",
        "private-command-stdin-fixture",
        "ciphertext",
        "fingerprint",
    ] {
        assert!(!text.contains(private));
    }
    let slots: Vec<surrealdb::types::Value> = response.take(1).unwrap();
    assert_eq!(slots.len(), 1);
}

#[tokio::test]
async fn cancellation_revocation_and_changed_run_prevent_command_dispatch() {
    let db = support::TestDb::new().await;
    for scenario in ["cancel", "revoke", "run", "lease", "account", "corrupt"] {
        // Independent provider IDs and owners are provided by separate database fixtures.
        let db = if scenario == "cancel" {
            &db
        } else {
            &support::TestDb::new().await
        };
        let (a, _, owner, agent, computer) = support::automation::setup(db).await;
        let grant = a
            .issue_automation_grant(&owner, &support::automation::input(computer))
            .await
            .unwrap();
        let claim = queue_claim(db, &a, &agent, computer, grant.grant_id).await;
        match scenario {
            "cancel" => {
                TaskRuntime::new(db.a.clone(), "computers", "client")
                    .cancel(&claim.snapshot.task_id.to_string())
                    .await
                    .unwrap();
            }
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
                    .query("UPDATE $computer SET process_id='different-native-run';")
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
            "account" => {
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
            }
            "corrupt" => {
                db.a.client()
                    .query("UPDATE computer_execution SET sealed.ciphertext='invalid-ciphertext';")
                    .await
                    .unwrap()
                    .check()
                    .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            a.begin_command_dispatch(&claim, &keys()).await.is_err(),
            "{scenario}"
        );
        let mut response = db.a.client().query("SELECT VALUE stage FROM computer_execution; SELECT * FROM computer_execution_slot; SELECT * FROM outbox_event WHERE event_type = 'computer.execution_dispatched';").await.unwrap().check().unwrap();
        let stages: Vec<String> = response.take(0).unwrap();
        assert_eq!(stages, ["queued"], "{scenario}");
        let slots: Vec<surrealdb::types::Value> = response.take(1).unwrap();
        assert_eq!(slots.len(), 1);
        let events: Vec<surrealdb::types::Value> = response.take(2).unwrap();
        assert!(events.is_empty());
    }
}

#[tokio::test]
async fn accepted_command_uses_current_grant_limits_after_admission_family_revocation() {
    let db = support::TestDb::new().await;
    let (a, _, owner, _, computer) = support::automation::setup(&db).await;
    let identity = support::browser::identity(&db, "bob").await;
    let bob = ComputerActor::from_verified(&identity).unwrap();
    let mut input = support::automation::input(computer);
    input.principal_id = bob.owner().principal_key.clone();
    input.oauth_client_id = "console".into();
    let grant = a.issue_automation_grant(&owner, &input).await.unwrap();
    let claim = queue_claim(&db, &a, &bob, computer, grant.grant_id).await;
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
        a.authorize_automation_grant(
            &bob,
            computer,
            grant.grant_id,
            AutomationPermission::Execute
        )
        .await
        .is_err()
    );
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
    let ticket = a.begin_command_dispatch(&claim, &keys()).await.unwrap();
    assert_eq!(ticket.limits().maximum_seconds, 7);
    assert_eq!(ticket.limits().maximum_output_bytes, 512);
    assert!(
        ticket.execution_deadline() - std::time::Instant::now()
            <= std::time::Duration::from_secs(7)
    );
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
