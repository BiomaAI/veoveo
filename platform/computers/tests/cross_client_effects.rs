//! Isolated persisted effects keep their original actor and encryption bindings.
#[path = "support/commands.rs"]
mod command_support;
mod support;
use uuid::Uuid;
use veoveo_computers::{
    api::*, commands::CommandTaskAction, files::FileTaskAction, maintenance::MaintenanceTarget,
    secrets::FileTransferPayload,
};
use veoveo_mcp_contract::{GatewayProfileId, LocalToolName};
use veoveo_task_runtime::TaskRuntime;

async fn install(db: &support::TestDb) {
    let mut control = support::automation::control();
    for name in ["transfer_file", "update_template", "resume_update"] {
        let name = LocalToolName::new(name).unwrap();
        control.servers[0].tools.push(name.clone());
        control.policies[0].rules[0].tools.insert(name);
    }
    let mut control = support::clients::control(control);
    control
        .oauth_clients
        .iter_mut()
        .find(|client| client.id.as_str() == "service")
        .unwrap()
        .allowed_resources
        .insert(
            veoveo_mcp_contract::ProtectedResourceId::new("https://computers.test/mcp/workspace")
                .unwrap(),
        );
    support::policy::install(&db.a, control).await;
}

#[tokio::test]
async fn lifecycle_and_maintenance_keep_the_actual_client_actor() {
    for maintenance in [false, true] {
        let db = support::TestDb::new().await;
        let (store, replica, console, _, computer) = support::automation::setup(&db).await;
        install(&db).await;
        let workspace = support::clients::workspace(&db, "alice").await;
        let workspace_owner = workspace.owner().clone();
        let before = store.get(console.owner(), computer).await.unwrap();
        let task_id = if maintenance {
            let operation = store
                .queue_maintenance(
                    &workspace,
                    computer,
                    Uuid::now_v7(),
                    &MaintenanceTarget {
                        template_id: "development-next".into(),
                        template_fingerprint: "b".repeat(64),
                    },
                )
                .await
                .unwrap();
            assert_eq!(operation.actor, *workspace.owner());
            assert_eq!(
                replica
                    .maintenance(console.owner(), operation.operation_id)
                    .await
                    .unwrap()
                    .actor,
                operation.actor
            );
            operation.task_id()
        } else {
            let operation = store
                .queue_operation(workspace, computer, Uuid::now_v7(), Action::Stop)
                .await
                .unwrap();
            assert_eq!(operation.owner, before.owner);
            assert_eq!(operation.actor, workspace_owner);
            replica
                .ensure_operation_task(&workspace_owner, operation.operation_id)
                .await
                .unwrap();
            assert_eq!(
                replica
                    .operation(console.owner(), operation.operation_id)
                    .await
                    .unwrap()
                    .actor,
                operation.actor
            );
            operation.task_id()
        };
        let task = TaskRuntime::new(db.b.clone(), "computers", "observer")
            .get(&task_id.to_string())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task.owner, workspace_owner);
        assert_eq!(
            store.get(console.owner(), computer).await.unwrap().owner,
            before.owner
        );
    }
}

#[tokio::test]
async fn file_task_recovery_preserves_ciphertext_and_original_actor() {
    let db = support::TestDb::new().await;
    let (store, replica, console, _, computer) = support::automation::setup(&db).await;
    install(&db).await;
    let workspace = support::clients::workspace(&db, "alice").await;
    let stranger = support::clients::workspace(&db, "bob").await;
    let payload = FileTransferPayload::new(
        FileTransfer::Export {
            path: RetainedFilePath::try_from("retained-owner-fixture.txt".to_owned()).unwrap(),
            filename: "result.txt".into(),
            media_type: "text/plain".into(),
        },
        FileTransferLimits {
            maximum_seconds: 30,
            maximum_bytes: 1024,
            on_interruption: AutomationInterruption::StopComputer,
        },
    )
    .unwrap();
    let keys = command_support::keys();
    let authority = store
        .file_transfer_authority(&workspace, computer, None)
        .await
        .unwrap();
    let operation = store
        .queue_file_transfer(&workspace, authority, Uuid::now_v7(), &payload, &keys)
        .await
        .unwrap();
    replica.ensure_file_task(&operation).await.unwrap();
    let record = veoveo_platform_store::RecordId::new(
        "computer_file_transfer",
        surrealdb::types::Uuid::from(operation.transfer_id()),
    );
    let mut read =
        db.a.client()
            .query("SELECT VALUE sealed FROM ONLY $record;")
            .bind(("record", record.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
    let sealed: Option<veoveo_platform_store::OpenObject> = read.take(0).unwrap();
    for actor in [&console, &workspace] {
        for action in [FileTaskAction::Observe, FileTaskAction::Cancel] {
            let access = replica
                .authorize_file_task(actor, operation.transfer_id(), action)
                .await
                .unwrap();
            assert_eq!(access.owner().unwrap(), workspace.owner());
        }
    }
    assert!(
        replica
            .authorize_file_task(&stranger, operation.transfer_id(), FileTaskAction::Observe)
            .await
            .is_err()
    );
    let mut read =
        db.b.client()
            .query("SELECT VALUE sealed FROM ONLY $record;")
            .bind(("record", record))
            .await
            .unwrap()
            .check()
            .unwrap();
    let after: Option<veoveo_platform_store::OpenObject> = read.take(0).unwrap();
    assert_eq!(
        serde_json::to_value(&sealed).unwrap(),
        serde_json::to_value(after).unwrap()
    );
    let sealed = serde_json::from_value(serde_json::to_value(sealed.unwrap()).unwrap()).unwrap();
    assert_eq!(
        keys.open_file_transfer(operation.binding(), &sealed)
            .unwrap()
            .transfer()
            .path()
            .as_str(),
        "retained-owner-fixture.txt"
    );
    let task = TaskRuntime::new(db.b.clone(), "computers", "observer")
        .get(&operation.task_id().to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(task.owner, *workspace.owner());
}

#[tokio::test]
async fn automation_grant_parent_and_task_recovery_span_owner_clients() {
    let db = support::TestDb::new().await;
    let (store, replica, console, agent, computer) = support::automation::setup(&db).await;
    install(&db).await;
    let workspace = support::clients::workspace(&db, "alice").await;
    let mut agent_owner = agent.owner().clone();
    agent_owner.profile = "workspace".into();
    let mut agent_identity = support::identity(&agent_owner);
    agent_identity
        .request_context
        .as_mut()
        .unwrap()
        .access_token
        .audience =
        veoveo_mcp_contract::ProtectedResourceId::new("https://computers.test/mcp/workspace")
            .unwrap();
    let workspace_agent = veoveo_computers::ComputerActor::from_verified(&agent_identity).unwrap();
    let grant = store
        .issue_automation_grant(&workspace, &support::automation::input(computer))
        .await
        .unwrap();
    assert!(
        store
            .authorize_automation_grant(
                &agent,
                computer,
                grant.grant_id,
                AutomationPermission::Execute
            )
            .await
            .is_err()
    );
    assert_eq!(
        replica
            .list_automation_grants(&console, computer)
            .await
            .unwrap()
            .grants
            .len(),
        1
    );
    let operation = store
        .queue_command(
            &workspace_agent,
            command_support::permit(&store, &workspace_agent, computer, grant.grant_id).await,
            Uuid::now_v7(),
            &command_support::payload("private-cross-client-command", 30),
            &command_support::keys(),
        )
        .await
        .unwrap();
    replica.ensure_command_task(&operation).await.unwrap();
    replica
        .revoke_automation_grant(
            &console,
            &RevokeAutomationGrantInput {
                computer_id: computer,
                grant_id: grant.grant_id,
            },
        )
        .await
        .unwrap();
    assert!(
        replica
            .authorize_command_task(
                &workspace_agent,
                operation.execution_id(),
                CommandTaskAction::Observe
            )
            .await
            .is_err()
    );
    for owner in [&console, &workspace] {
        for action in [CommandTaskAction::Observe, CommandTaskAction::Cancel] {
            assert_eq!(
                replica
                    .authorize_command_task(owner, operation.execution_id(), action)
                    .await
                    .unwrap()
                    .owner()
                    .unwrap(),
                workspace_agent.owner()
            );
        }
    }
}

#[tokio::test]
async fn cli_pairing_keeps_its_profile_and_can_be_revoked_from_another_client() {
    let db = support::TestDb::new().await;
    let (store, replica, console, _, computer) = support::automation::setup(&db).await;
    install(&db).await;
    let workspace = support::clients::workspace(&db, "alice").await;
    let pairing = store
        .begin_cli_pairing(
            &workspace,
            computer,
            &CliPairingInput {
                name: "Cross-client fixture".into(),
                code: "ABC-2345".into(),
                callback_port: 49152,
            },
        )
        .await
        .unwrap();
    assert!(
        replica
            .confirm_cli_pairing(&console, computer, pairing.pairing_id)
            .await
            .is_err()
    );
    let paired = replica
        .confirm_cli_pairing(&workspace, computer, pairing.pairing_id)
        .await
        .unwrap();
    assert!(
        store
            .open_cli_connection(
                Some(computer),
                GatewayProfileId::new("operator").unwrap(),
                &paired.credential
            )
            .await
            .is_err()
    );
    let handle = store
        .open_cli_connection(
            Some(computer),
            GatewayProfileId::new("workspace").unwrap(),
            &paired.credential,
        )
        .await
        .unwrap();
    replica.renew_cli_grant(&handle, true).await.unwrap();
    let grants = replica.cli_access_grants(&console, computer).await.unwrap();
    assert_eq!(grants.len(), 1);
    assert!(!grants[0].current_session);
    replica
        .revoke_cli_grant(&console, computer, paired.grant_id)
        .await
        .unwrap();
    assert!(store.renew_cli_grant(&handle, false).await.is_err());
}
