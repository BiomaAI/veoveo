mod support;
use chrono::{TimeDelta, Utc};
use uuid::Uuid;
use veoveo_computers::{
    ComputerActor, ComputerError, ComputersStore, api::*, automation_grants::AutomationGrantPolicy,
};
use veoveo_mcp_contract::{GatewayControlPlane, InvocationProvenance, LocalToolName, PolicyEffect};
use veoveo_platform_store::{PrincipalKind, RecordId, deterministic_principal_id};

const POLICY: AutomationGrantPolicy = AutomationGrantPolicy {
    max_grants: 2,
    maximum_lifetime_seconds: 3600,
    maximum_execution_seconds: 60,
    maximum_output_bytes: 65536,
};

fn control() -> GatewayControlPlane {
    let mut control = support::interactive::control();
    for tool in ["execute", "grant_automation", "revoke_automation"] {
        let tool = LocalToolName::new(tool).unwrap();
        control.servers[0].tools.push(tool.clone());
        control.policies[0].rules[0].tools.insert(tool);
    }
    control
}
async fn setup(
    db: &support::TestDb,
) -> (
    ComputersStore,
    ComputersStore,
    ComputerActor,
    ComputerActor,
    Uuid,
) {
    let owner =
        ComputerActor::from_verified(&support::browser::identity(db, "alice").await).unwrap();
    let (a, b, computer) = support::interactive::ready(db, &owner).await;
    support::policy::install(&db.a, control()).await;
    a.install_automation_grant_policy(None, POLICY)
        .await
        .unwrap();
    let mut service = support::owner("service");
    service.principal_kind = PrincipalKind::Service;
    service.authority.provenance = InvocationProvenance::Automated;
    db.a.ensure_identity(
        service.tenant_key(),
        &service.principal_key,
        &service.issuer,
        &service.subject,
        service.principal_kind,
    )
    .await
    .unwrap();
    let agent = support::authenticated(&service);
    (a, b, owner, agent, computer)
}
fn input(computer: Uuid) -> IssueAutomationGrantInput {
    IssueAutomationGrantInput {
        computer_id: computer,
        request_id: Uuid::now_v7(),
        principal_id: "https://computers.test#service".into(),
        oauth_client_id: "service".into(),
        name: "Build agent".into(),
        permissions: [AutomationPermission::Read, AutomationPermission::Execute].into(),
        execution_limits: Some(AutomationExecutionLimits {
            maximum_seconds: 30,
            maximum_output_bytes: 1024,
        }),
        expires_at: Utc::now() + TimeDelta::minutes(30),
    }
}

#[tokio::test]
async fn concurrent_grants_are_idempotent_bounded_and_revocation_never_reissues_them() {
    let db = support::TestDb::new().await;
    let (a, b, owner, agent, computer) = setup(&db).await;
    let request = input(computer);
    let (left, right) = futures::join!(
        a.issue_automation_grant(&owner, &request),
        b.issue_automation_grant(&owner, &request)
    );
    let first = left.unwrap();
    assert_eq!(first, right.unwrap());
    let changed = IssueAutomationGrantInput {
        name: "Changed".into(),
        ..request.clone()
    };
    assert_eq!(
        a.issue_automation_grant(&owner, &changed).await,
        Err(ComputerError::RequestConflict)
    );
    let second = input(computer);
    let third = input(computer);
    let (left, right) = futures::join!(
        a.issue_automation_grant(&owner, &second),
        b.issue_automation_grant(&owner, &third)
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert!(matches!(
        (left, right),
        (Ok(_), Err(ComputerError::AccessLimit)) | (Err(ComputerError::AccessLimit), Ok(_))
    ));
    assert_eq!(
        a.list_automation_grants(&owner, computer)
            .await
            .unwrap()
            .grants
            .len(),
        2
    );
    assert!(matches!(
        a.get(agent.owner(), computer).await,
        Err(ComputerError::NotFound)
    ));
    let authority = b
        .authorize_automation_grant(
            &agent,
            computer,
            first.grant_id,
            AutomationPermission::Execute,
        )
        .await
        .unwrap();
    assert_eq!(
        authority.computer().unwrap().owner.principal_key,
        owner.owner().principal_key
    );
    assert_eq!(
        authority.execution_limits().unwrap(),
        request.execution_limits
    );
    let read = b
        .authorize_automation_grant(&agent, computer, first.grant_id, AutomationPermission::Read)
        .await
        .unwrap();
    assert_eq!(read.execution_limits(), Err(ComputerError::Forbidden));
    assert!(
        authority.valid_until() <= std::time::Instant::now() + std::time::Duration::from_secs(30)
    );
    let revoke = RevokeAutomationGrantInput {
        computer_id: computer,
        grant_id: first.grant_id,
    };
    let (left, right) = futures::join!(
        a.revoke_automation_grant(&owner, &revoke),
        b.revoke_automation_grant(&owner, &revoke)
    );
    let revoked = left.unwrap();
    assert_eq!(revoked, right.unwrap());
    assert!(revoked.revoked_at.is_some());
    assert!(matches!(
        b.authorize_automation_grant(
            &agent,
            computer,
            first.grant_id,
            AutomationPermission::Execute
        )
        .await,
        Err(ComputerError::Forbidden)
    ));
    a.install_automation_grant_policy(
        Some(POLICY),
        AutomationGrantPolicy {
            max_grants: 0,
            ..POLICY
        },
    )
    .await
    .unwrap();
    assert_eq!(
        a.issue_automation_grant(&owner, &request).await.unwrap(),
        revoked
    );
    let mut events = db.b.client().query("SELECT VALUE event_type FROM outbox_event WHERE aggregate_id = $id AND event_type = 'automation_revoked';").bind(("id", computer.to_string())).await.unwrap().check().unwrap();
    let events: Vec<String> = events.take(0).unwrap();
    assert_eq!(events.len(), 1);
}

#[tokio::test]
async fn automation_uses_its_own_principal_and_survives_the_grantors_browser_logout() {
    let db = support::TestDb::new().await;
    let (a, b, owner, agent, computer) = setup(&db).await;
    let granted = a
        .issue_automation_grant(&owner, &input(computer))
        .await
        .unwrap();
    db.b.client()
        .query("UPDATE gateway_refresh_family SET revoked_at = time::now();")
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(
        b.authorize_automation_grant(
            &agent,
            computer,
            granted.grant_id,
            AutomationPermission::Execute
        )
        .await
        .is_ok()
    );
    assert!(matches!(
        b.authorize_automation_grant(
            &agent,
            computer,
            granted.grant_id,
            AutomationPermission::Stop
        )
        .await,
        Err(ComputerError::Forbidden)
    ));
    assert!(matches!(
        b.authorize_automation_grant(
            &owner,
            computer,
            granted.grant_id,
            AutomationPermission::Read
        )
        .await,
        Err(ComputerError::NotFound)
    ));
    assert!(matches!(
        b.authorize_automation_grant(
            &agent,
            Uuid::now_v7(),
            granted.grant_id,
            AutomationPermission::Read
        )
        .await,
        Err(ComputerError::NotFound)
    ));
    let foreign = ComputersStore::new(db.b.clone(), Uuid::now_v7()).unwrap();
    assert!(matches!(
        foreign
            .authorize_automation_grant(
                &agent,
                computer,
                granted.grant_id,
                AutomationPermission::Read
            )
            .await,
        Err(ComputerError::NotFound)
    ));
    let fresh_owner =
        ComputerActor::from_verified(&support::browser::identity(&db, "alice").await).unwrap();
    b.revoke_automation_grant(
        &fresh_owner,
        &RevokeAutomationGrantInput {
            computer_id: computer,
            grant_id: granted.grant_id,
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        a.authorize_automation_grant(
            &agent,
            computer,
            granted.grant_id,
            AutomationPermission::Read
        )
        .await,
        Err(ComputerError::Forbidden)
    ));
}

#[tokio::test]
async fn a_user_principal_can_receive_a_grant_but_cannot_change_its_oauth_client() {
    let db = support::TestDb::new().await;
    let (a, _, owner, _, computer) = setup(&db).await;
    let bob = support::owner("bob");
    db.a.ensure_identity(
        bob.tenant_key(),
        &bob.principal_key,
        &bob.issuer,
        &bob.subject,
        bob.principal_kind,
    )
    .await
    .unwrap();
    let mut identity = support::identity(&bob);
    let actor = ComputerActor::from_verified(&identity).unwrap();
    let mut request = input(computer);
    request.principal_id = bob.principal_key;
    request.oauth_client_id = "console".into();
    let grant = a.issue_automation_grant(&owner, &request).await.unwrap();
    assert!(
        a.authorize_automation_grant(
            &actor,
            computer,
            grant.grant_id,
            AutomationPermission::Execute
        )
        .await
        .is_ok()
    );
    let mut current = control();
    let mut client = current
        .oauth_clients
        .iter()
        .find(|client| client.id.as_str() == "console")
        .unwrap()
        .clone();
    client.id = veoveo_mcp_contract::OAuthClientId::new("console-secondary").unwrap();
    current.work_contexts[0].memberships[0]
        .oauth_clients
        .insert(client.id.clone());
    identity
        .request_context
        .as_mut()
        .unwrap()
        .access_token
        .oauth_client_id = client.id.clone();
    current.oauth_clients.push(client);
    support::policy::install(&db.b, current).await;
    let actor = ComputerActor::from_verified(&identity).unwrap();
    assert!(matches!(
        a.authorize_automation_grant(
            &actor,
            computer,
            grant.grant_id,
            AutomationPermission::Execute
        )
        .await,
        Err(ComputerError::NotFound)
    ));
}

#[tokio::test]
async fn current_principals_policy_clearance_and_reduced_limits_bound_each_use() {
    let db = support::TestDb::new().await;
    let (a, b, owner, agent, computer) = setup(&db).await;
    let granted = a
        .issue_automation_grant(&owner, &input(computer))
        .await
        .unwrap();
    for name in ["alice", "service"] {
        let principal =
            deterministic_principal_id("test", &format!("https://computers.test#{name}"))
                .unwrap()
                .record_id();
        db.b.client()
            .query("UPDATE ONLY $principal SET enabled=false;")
            .bind(("principal", principal.clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        assert!(matches!(
            a.authorize_automation_grant(
                &agent,
                computer,
                granted.grant_id,
                AutomationPermission::Execute
            )
            .await,
            Err(ComputerError::Forbidden)
        ));
        db.b.client()
            .query("UPDATE ONLY $principal SET enabled=true;")
            .bind(("principal", principal))
            .await
            .unwrap()
            .check()
            .unwrap();
    }
    let mut denied = control();
    denied.policies[0].rules[0].effect = PolicyEffect::Deny;
    support::policy::install(&db.b, denied).await;
    assert!(matches!(
        a.authorize_automation_grant(
            &agent,
            computer,
            granted.grant_id,
            AutomationPermission::Execute
        )
        .await,
        Err(ComputerError::Forbidden)
    ));
    support::policy::install(&db.b, control()).await;
    let record = RecordId::new("computer", surrealdb::types::Uuid::from(computer));
    db.b.client()
        .query("UPDATE ONLY $computer SET owner_context.data_labels = ['pii'];")
        .bind(("computer", record.clone()))
        .await
        .unwrap()
        .check()
        .unwrap();
    assert!(matches!(
        a.authorize_automation_grant(
            &agent,
            computer,
            granted.grant_id,
            AutomationPermission::Read
        )
        .await,
        Err(ComputerError::Forbidden)
    ));
    db.b.client()
        .query("UPDATE ONLY $computer SET owner_context.data_labels = [];")
        .bind(("computer", record))
        .await
        .unwrap()
        .check()
        .unwrap();
    let smaller = AutomationGrantPolicy {
        maximum_execution_seconds: 5,
        maximum_output_bytes: 8,
        ..POLICY
    };
    b.install_automation_grant_policy(Some(POLICY), smaller)
        .await
        .unwrap();
    let authority = a
        .authorize_automation_grant(
            &agent,
            computer,
            granted.grant_id,
            AutomationPermission::Execute,
        )
        .await
        .unwrap();
    assert_eq!(
        authority.execution_limits().unwrap(),
        Some(AutomationExecutionLimits {
            maximum_seconds: 5,
            maximum_output_bytes: 8
        })
    );
    b.install_automation_grant_policy(
        Some(smaller),
        AutomationGrantPolicy {
            maximum_lifetime_seconds: 1,
            ..smaller
        },
    )
    .await
    .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    assert!(matches!(
        a.authorize_automation_grant(
            &agent,
            computer,
            granted.grant_id,
            AutomationPermission::Read
        )
        .await,
        Err(ComputerError::Forbidden)
    ));
}

#[tokio::test]
async fn invalid_bounds_unknown_principals_and_foreign_owners_create_no_grants() {
    let db = support::TestDb::new().await;
    let (a, _, owner, agent, computer) = setup(&db).await;
    let original = input(computer);
    let mut missing = original.clone();
    missing.principal_id = "https://computers.test#missing".into();
    assert_eq!(
        a.issue_automation_grant(&owner, &missing).await,
        Err(ComputerError::NotFound)
    );
    let mut unknown_client = original.clone();
    unknown_client.oauth_client_id = "unknown-client".into();
    assert_eq!(
        a.issue_automation_grant(&owner, &unknown_client).await,
        Err(ComputerError::InvalidInput)
    );
    assert_eq!(
        a.issue_automation_grant(&agent, &original).await,
        Err(ComputerError::NotFound)
    );
    for mode in 0..6 {
        let mut request = original.clone();
        match mode {
            0 => request.permissions.clear(),
            1 => request.execution_limits = None,
            2 => {
                request
                    .execution_limits
                    .as_mut()
                    .unwrap()
                    .maximum_output_bytes = 0
            }
            3 => request.expires_at = Utc::now() - TimeDelta::seconds(1),
            4 => request.expires_at = Utc::now() + TimeDelta::days(2),
            _ => request.execution_limits.as_mut().unwrap().maximum_seconds = 61,
        }
        assert_eq!(
            a.issue_automation_grant(&owner, &request).await,
            Err(ComputerError::InvalidInput),
            "mode {mode}"
        );
    }
    assert!(
        a.list_automation_grants(&owner, computer)
            .await
            .unwrap()
            .grants
            .is_empty()
    );
}
