//! Denied Task and access-ledger reads never decode protected metadata or state.
#[path = "support/files.rs"]
#[allow(unused_imports)] // The shared fixture also exports optional Computer helpers.
mod file_support;
use file_support::command_fixture as command_support;
mod support;
use std::time::Duration;
use surrealdb::types::RecordId;
use uuid::Uuid;
use veoveo_computers::{
    ComputerActor, ComputerError, Reservation, api::*, cli_grants::CliGrantCredential,
    commands::CommandTaskAction, files::FileTaskAction,
};

fn record(table: &str, id: Uuid) -> RecordId {
    RecordId::new(table, surrealdb::types::Uuid::from(id))
}
async fn corrupt_authority(db: &support::TestDb, row: RecordId) {
    db.a.client()
        .query("UPDATE ONLY $row SET authority.request_context.access_token.expires_at = 42;")
        .bind(("row", row))
        .await
        .unwrap()
        .check()
        .unwrap();
}
async fn another_computer(
    store: &veoveo_computers::ComputersStore,
    actor: &ComputerActor,
) -> ComputerId {
    store
        .reserve(
            actor,
            &Reservation {
                request_id: Uuid::now_v7(),
                template_id: "development".into(),
                template_fingerprint: support::FINGERPRINT.into(),
            },
        )
        .await
        .unwrap()
        .computer_id
}

#[tokio::test]
async fn operation_policy_and_participant_checks_precede_private_state_decoding() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let (store, _, owner, agent, computer) = support::automation::setup(&db).await;
        let mut input = support::automation::input(computer);
        input.permissions = [AutomationPermission::Stop].into();
        input.execution_limits = None;
        let grant = store.issue_automation_grant(&owner, &input).await.unwrap();
        let authority = store.authorize_automation_grant(&agent, computer, grant.grant_id, AutomationPermission::Stop).await.unwrap();
        let operation = store.queue_automation_operation(&agent, authority, Uuid::now_v7(), Action::Stop).await.unwrap();
        store.authorize_operation_task(&owner, operation.operation_id, false).await.unwrap();
        store.authorize_operation_task(&agent, operation.operation_id, false).await.unwrap();
        db.a.client().query("UPDATE ONLY $row SET execution_authority.request_context.access_token.expires_at = 42;")
            .bind(("row", record("computer_operation", operation.operation_id))).await.unwrap().check().unwrap();
        let other = support::authenticated(&support::owner("bob"));
        assert!(matches!(store.operation(other.owner(), operation.operation_id).await, Err(ComputerError::NotFound)));
        assert!(matches!(store.automation_operation(&other, operation.operation_id).await, Err(ComputerError::NotFound)));
        assert!(matches!(store.operation(owner.owner(), operation.operation_id).await, Err(ComputerError::Unavailable)));
        store.revoke_automation_grant(&owner, &RevokeAutomationGrantInput { computer_id: computer, grant_id: grant.grant_id }).await.unwrap();
        assert!(matches!(store.automation_operation(&agent, operation.operation_id).await, Err(ComputerError::Forbidden)));
        assert!(matches!(store.authorize_operation_task(&agent, operation.operation_id, false).await, Err(ComputerError::Forbidden)));
    }).await.expect("operation SQL admission exceeded 90 seconds");
}

#[tokio::test]
async fn command_metadata_requires_current_execute_or_owner_read_before_decoding() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let (store, _, owner, agent, computer) = support::automation::setup(&db).await;
        let grant = store
            .issue_automation_grant(&owner, &support::automation::input(computer))
            .await
            .unwrap();
        let claim =
            command_support::queue_claim(&db, &store, &agent, computer, grant.grant_id).await;
        let id = ExecutionId::try_from(claim.snapshot.task_id.as_uuid()).unwrap();
        corrupt_authority(&db, record("computer_execution", id.into_uuid())).await;
        let other = support::authenticated(&support::owner("bob"));
        assert!(matches!(
            store
                .authorize_command_task(&other, id, CommandTaskAction::Observe)
                .await,
            Err(ComputerError::NotFound)
        ));
        assert!(matches!(
            store
                .authorize_command_task(&owner, id, CommandTaskAction::Observe)
                .await,
            Err(ComputerError::Unavailable)
        ));
        store
            .revoke_automation_grant(
                &owner,
                &RevokeAutomationGrantInput {
                    computer_id: computer,
                    grant_id: grant.grant_id,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            store
                .authorize_command_task(&agent, id, CommandTaskAction::Observe)
                .await,
            Err(ComputerError::Forbidden)
        ));
        db.a.client()
            .query("UPDATE ONLY $row SET binding.required_output_labels = ['private'];")
            .bind(("row", record("computer_execution", id.into_uuid())))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(matches!(
            store
                .authorize_command_task(&owner, id, CommandTaskAction::Observe)
                .await,
            Err(ComputerError::NotFound)
        ));
    })
    .await
    .expect("command SQL admission exceeded 90 seconds");
}

#[tokio::test]
async fn file_metadata_requires_current_execute_or_owner_read_before_decoding() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let (store, _, owner, agent, computer) = file_support::setup(&db).await;
        let grant = store
            .issue_automation_grant(&owner, &support::automation::input(computer))
            .await
            .unwrap();
        let claim = file_support::queue_claim(&db, &store, &agent, computer, grant.grant_id).await;
        let id = FileTransferId::try_from(claim.snapshot.task_id.as_uuid()).unwrap();
        corrupt_authority(&db, record("computer_file_transfer", id.into_uuid())).await;
        let other = support::authenticated(&support::owner("bob"));
        assert!(matches!(
            store
                .authorize_file_task(&other, id, FileTaskAction::Observe)
                .await,
            Err(ComputerError::NotFound)
        ));
        assert!(matches!(
            store
                .authorize_file_task(&owner, id, FileTaskAction::Observe)
                .await,
            Err(ComputerError::Unavailable)
        ));
        store
            .revoke_automation_grant(
                &owner,
                &RevokeAutomationGrantInput {
                    computer_id: computer,
                    grant_id: grant.grant_id,
                },
            )
            .await
            .unwrap();
        assert!(matches!(
            store
                .authorize_file_task(&agent, id, FileTaskAction::Observe)
                .await,
            Err(ComputerError::Forbidden)
        ));
        db.a.client()
            .query("UPDATE ONLY $row SET binding.required_labels = ['private'];")
            .bind(("row", record("computer_file_transfer", id.into_uuid())))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(matches!(
            store
                .authorize_file_task(&owner, id, FileTaskAction::Observe)
                .await,
            Err(ComputerError::NotFound)
        ));
    })
    .await
    .expect("file SQL admission exceeded 90 seconds");
}

#[tokio::test]
async fn browser_grants_filter_ticket_parent_provider_and_connection_before_decoding() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let actor =
            ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
        let (store, _, computer) = support::interactive::ready(&db, &actor).await;
        let other =
            ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
        let ticket = store.issue_browser_grant(&actor, computer).await.unwrap();
        let handle = store
            .redeem_browser_grant(&actor, &ticket.token)
            .await
            .unwrap();
        let pending = store.issue_browser_grant(&actor, computer).await.unwrap();
        let id: Uuid = pending
            .token
            .expose_secret()
            .split_once('.')
            .unwrap()
            .0
            .parse()
            .unwrap();
        corrupt_authority(&db, record("computer_session_grant", id)).await;
        let wrong = TerminalToken::new(format!("{id}.{}", "f".repeat(64)));
        assert!(matches!(
            store.redeem_browser_grant(&actor, &wrong).await,
            Err(ComputerError::Forbidden)
        ));
        assert!(matches!(
            store.redeem_browser_grant(&other, &pending.token).await,
            Err(ComputerError::Forbidden)
        ));
        assert!(matches!(
            store.redeem_browser_grant(&actor, &pending.token).await,
            Err(ComputerError::Unavailable)
        ));
        let foreign = another_computer(&store, &actor).await;
        assert!(matches!(
            store.revoke_browser_grant(&actor, foreign, id).await,
            Err(ComputerError::NotFound)
        ));
        db.a.client()
            .query("UPDATE ONLY $row SET provider_instance_id = $provider;")
            .bind(("row", record("computer_session_grant", id)))
            .bind(("provider", Uuid::now_v7()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert_eq!(
            store
                .access_grants(&actor, computer)
                .await
                .unwrap()
                .grants
                .len(),
            1
        );
        corrupt_authority(&db, record("computer_session_grant", handle.grant_id())).await;
        db.a.client()
            .query("UPDATE ONLY $row SET connection_id = $connection;")
            .bind(("row", record("computer_session_grant", handle.grant_id())))
            .bind(("connection", Uuid::now_v7()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(matches!(
            store.renew_browser_grant(&handle, false).await,
            Err(ComputerError::Forbidden)
        ));
        assert!(matches!(
            store.close_browser_grant(&handle).await,
            Err(ComputerError::Forbidden)
        ));
    })
    .await
    .expect("browser grant SQL admission exceeded 90 seconds");
}

#[tokio::test]
async fn cli_grants_filter_credentials_parent_provider_and_connection_before_decoding() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let actor =
            ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
        let (store, _, computer) = support::interactive::ready(&db, &actor).await;
        let pairing = store
            .begin_cli_pairing(
                &actor,
                computer,
                &CliPairingInput {
                    name: "SQL fixture".into(),
                    code: "ABC-2345".into(),
                    callback_port: 49152,
                },
            )
            .await
            .unwrap();
        let grant = store
            .confirm_cli_pairing(&actor, computer, pairing.pairing_id)
            .await
            .unwrap();
        let profile = veoveo_mcp_contract::GatewayProfileId::new("operator").unwrap();
        let handle = store
            .open_cli_connection(Some(computer), profile.clone(), &grant.credential)
            .await
            .unwrap();
        corrupt_authority(&db, record("computer_cli_grant", grant.grant_id)).await;
        let wrong = CliGrantCredential::new(format!("vcli1.{}.{}", grant.grant_id, "f".repeat(64)));
        assert!(matches!(
            store
                .open_cli_connection(Some(computer), profile.clone(), &wrong)
                .await,
            Err(ComputerError::Forbidden)
        ));
        let foreign = another_computer(&store, &actor).await;
        assert!(matches!(
            store
                .open_cli_connection(Some(foreign), profile.clone(), &grant.credential)
                .await,
            Err(ComputerError::Forbidden)
        ));
        assert!(matches!(
            store
                .open_cli_connection(Some(computer), profile, &grant.credential)
                .await,
            Err(ComputerError::Unavailable)
        ));
        assert!(matches!(
            store
                .revoke_cli_grant(&actor, foreign, grant.grant_id)
                .await,
            Err(ComputerError::NotFound)
        ));
        db.a.client()
            .query(
                "UPDATE computer_cli_connection SET grant_id = $foreign WHERE grant_id = $grant;",
            )
            .bind(("grant", grant.grant_id))
            .bind(("foreign", Uuid::now_v7()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(matches!(
            store.renew_cli_grant(&handle, false).await,
            Err(ComputerError::Forbidden)
        ));
        db.a.client()
            .query("UPDATE ONLY $row SET provider_instance_id = $provider;")
            .bind(("row", record("computer_cli_grant", grant.grant_id)))
            .bind(("provider", Uuid::now_v7()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(
            store
                .cli_access_grants(&actor, computer)
                .await
                .unwrap()
                .is_empty()
        );
    })
    .await
    .expect("CLI grant SQL admission exceeded 90 seconds");
}

#[tokio::test]
async fn cli_pairing_matches_parent_and_session_before_decoding_callback() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = support::TestDb::new().await;
        let actor =
            ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
        let (store, _, computer) = support::interactive::ready(&db, &actor).await;
        let other =
            ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
        let pairing = store
            .begin_cli_pairing(
                &actor,
                computer,
                &CliPairingInput {
                    name: "SQL fixture".into(),
                    code: "ABC-2345".into(),
                    callback_port: 49152,
                },
            )
            .await
            .unwrap();
        // Relax this disposable fixture's range assertion to exercise admission
        // before Rust decoding. The production schema enforces valid ports too.
        db.a.client()
            .query("DEFINE FIELD OVERWRITE callback_port ON computer_cli_pairing TYPE int; UPDATE ONLY $row SET callback_port = 999999;")
            .bind(("row", record("computer_cli_pairing", pairing.pairing_id)))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(matches!(
            store
                .confirm_cli_pairing(&other, computer, pairing.pairing_id)
                .await,
            Err(ComputerError::NotFound)
        ));
        assert!(matches!(
            store
                .confirm_cli_pairing(&actor, computer, pairing.pairing_id)
                .await,
            Err(ComputerError::Unavailable)
        ));
    })
    .await
    .expect("CLI pairing SQL admission exceeded 90 seconds");
}
