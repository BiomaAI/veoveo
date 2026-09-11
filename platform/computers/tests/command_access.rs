#[path = "support/commands.rs"]
mod command_support;
mod support;

use uuid::Uuid;
use veoveo_computers::{ComputerActor, ComputersStore, api::*, commands::CommandTaskAction};

#[tokio::test]
async fn command_tasks_keep_source_client_authority_and_owner_control_separate() {
    let db = support::TestDb::new().await;
    let (a, b, owner, agent, computer) = support::automation::setup(&db).await;
    let mut input = support::automation::input(computer);
    // Execute carries progress/cancellation for its own Task without broader Read.
    input.permissions = [AutomationPermission::Execute].into();
    let grant = a.issue_automation_grant(&owner, &input).await.unwrap();
    let claim = command_support::queue_claim(&db, &a, &agent, computer, grant.grant_id).await;
    let execution = Uuid::parse_str(&claim.snapshot.task_id.to_string()).unwrap();
    for actor in [&owner, &agent] {
        for action in [CommandTaskAction::Observe, CommandTaskAction::Cancel] {
            let access = b
                .authorize_command_task(actor, execution, action)
                .await
                .unwrap();
            assert_eq!(access.owner().unwrap(), agent.owner());
            assert_eq!(access.computer_id(), computer);
            assert_eq!(access.execution_id(), execution);
        }
    }
    let other = support::authenticated(&support::owner("bob"));
    assert!(
        a.authorize_command_task(&other, execution, CommandTaskAction::Observe)
            .await
            .is_err()
    );
    let other_provider = ComputersStore::new(db.a.clone(), Uuid::now_v7()).unwrap();
    assert!(
        other_provider
            .authorize_command_task(&agent, execution, CommandTaskAction::Observe)
            .await
            .is_err()
    );

    // Signed request consistency rejects a forged machine client or expired
    // token. Valid changed tenant/context/profile identities still have no grant.
    for field in ["client", "context", "profile", "tenant", "expired"] {
        let mut identity = support::identity(agent.owner());
        match field {
            "client" => {
                identity
                    .request_context
                    .as_mut()
                    .unwrap()
                    .access_token
                    .oauth_client_id =
                    veoveo_mcp_contract::OAuthClientId::new("different-client").unwrap()
            }
            "profile" => {
                identity.profile =
                    veoveo_mcp_contract::GatewayProfileId::new("different-profile").unwrap()
            }
            "expired" => identity.expires_at = chrono::Utc::now() - chrono::TimeDelta::seconds(1),
            // Consistently changed invocation inputs remain different identities.
            "context" => {
                identity.authority.work_context =
                    veoveo_mcp_contract::WorkContextId::new("different-context").unwrap();
                identity
                    .request_context
                    .as_mut()
                    .unwrap()
                    .access_token
                    .work_context = identity.authority.work_context.clone();
            }
            "tenant" => {
                identity.authority.tenant =
                    veoveo_mcp_contract::TenantId::new("different-tenant").unwrap();
                identity.actor.tenant = Some(identity.authority.tenant.clone());
                identity.request_context.as_mut().unwrap().principal.tenant =
                    identity.actor.tenant.clone();
            }
            _ => unreachable!(),
        }
        if matches!(field, "client" | "expired") {
            assert!(ComputerActor::from_verified(&identity).is_err(), "{field}");
        } else {
            let actor = ComputerActor::from_verified(&identity).unwrap();
            assert!(
                a.authorize_command_task(&actor, execution, CommandTaskAction::Observe)
                    .await
                    .is_err(),
                "{field}"
            );
        }
    }
    a.revoke_automation_grant(
        &owner,
        &RevokeAutomationGrantInput {
            computer_id: computer,
            grant_id: grant.grant_id,
        },
    )
    .await
    .unwrap();
    for action in [CommandTaskAction::Observe, CommandTaskAction::Cancel] {
        assert!(
            b.authorize_command_task(&agent, execution, action)
                .await
                .is_err()
        );
        assert!(
            b.authorize_command_task(&owner, execution, action)
                .await
                .is_ok()
        );
    }
    let mut control = support::automation::control();
    control.policies[0].rules[0]
        .tools
        .remove(&veoveo_mcp_contract::LocalToolName::new("stop").unwrap());
    support::policy::install(&db.a, control).await;
    assert!(
        a.authorize_command_task(&owner, execution, CommandTaskAction::Observe)
            .await
            .is_ok()
    );
    assert!(
        a.authorize_command_task(&owner, execution, CommandTaskAction::Cancel)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn a_current_human_token_from_another_client_cannot_read_a_command_task() {
    let db = support::TestDb::new().await;
    let (a, b, owner, _, computer) = support::automation::setup(&db).await;
    let human = support::owner("bob");
    db.a.ensure_identity(
        human.tenant_key(),
        &human.principal_key,
        &human.issuer,
        &human.subject,
        human.principal_kind,
    )
    .await
    .unwrap();
    let actor = support::authenticated(&human);
    let mut input = support::automation::input(computer);
    input.principal_id = human.principal_key.clone();
    input.oauth_client_id = "console".into();
    let grant = a.issue_automation_grant(&owner, &input).await.unwrap();
    let claim = command_support::queue_claim(&db, &a, &actor, computer, grant.grant_id).await;
    let execution = Uuid::parse_str(&claim.snapshot.task_id.to_string()).unwrap();
    assert!(
        b.authorize_command_task(&actor, execution, CommandTaskAction::Observe)
            .await
            .is_ok()
    );
    let mut identity = support::identity(&human);
    identity
        .request_context
        .as_mut()
        .unwrap()
        .access_token
        .oauth_client_id = veoveo_mcp_contract::OAuthClientId::new("another-human-client").unwrap();
    let another_client = ComputerActor::from_verified(&identity).unwrap();
    assert!(
        b.authorize_command_task(&another_client, execution, CommandTaskAction::Observe)
            .await
            .is_err()
    );
}
