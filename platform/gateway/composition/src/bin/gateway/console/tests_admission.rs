//! Actual catalog, assertion signature and durable policy audit; source auth is a fixture.
use axum::extract::{Extension, State};
use axum::http::{StatusCode, header};
use chrono::{TimeDelta, Utc};
use std::sync::Arc;
use veoveo_gateway_contract::{GatewayAction, ProtectedResourceId};
use veoveo_mcp_contract::*;
use veoveo_mcp_gateway::GatewayCatalogHandle;
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalog};
use veoveo_types::InvocationAuthority;
use veoveo_types::{InvocationMode, PrincipalId, ScopeName, TenantId};

fn control() -> GatewayControlPlane {
    let mut control: GatewayControlPlane = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../computers/tests/support/gateway.json"
    )))
    .unwrap();
    let mut rule = control.policies[0].rules[0].clone();
    rule.id = PolicyRuleId::new("computer-access").unwrap();
    let registry = crate::bindings::catalog_admission().unwrap();
    let attach = registry
        .registry()
        .unwrap()
        .action_key::<veoveo_computers_contract::ComputerAction>()
        .unwrap()
        .action(veoveo_computers_contract::ComputerAction::Attach)
        .unwrap();
    rule.actions = [GatewayAction::ResourcesRead.into(), attach.name().clone()]
        .into_iter()
        .collect();
    rule.tools.clear();
    control.policies[0].rules.push(rule);
    control
}
fn subject() -> AuthenticatedSubject {
    let principal = Principal {
        id: PrincipalId::new("https://computers.test#alice").unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::new("https://computers.test").unwrap(),
        subject: TokenSubject::new("alice").unwrap(),
        tenant: Some(TenantId::new("test").unwrap()),
        groups: Default::default(),
        group_roles: Default::default(),
        roles: Default::default(),
        scopes: [ScopeName::new("operator:use").unwrap()]
            .into_iter()
            .collect(),
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: None,
    };
    let authority: InvocationAuthority = serde_json::from_value(serde_json::json!({
        "work_context":"computers-test", "tenant":"test", "membership":"contributor", "policy_revision":"test-1",
        "output_policy":{"owner":{"kind":"principal","id":principal.id}}, "provenance":{"mode":"direct","initiator":principal.id}
    })).unwrap();
    let now = Utc::now();
    let access_token = AccessTokenSubject {
        managed_execution: None,
        issuer: principal.issuer.clone(),
        subject: principal.subject.clone(),
        oauth_client_id: OAuthClientId::new("console").unwrap(),
        session_family: Some(
            GatewayRefreshFamilyId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
        ),
        audience: ProtectedResourceId::new("https://computers.test/mcp/operator").unwrap(),
        work_context: authority.work_context.clone(),
        invocation_mode: InvocationMode::Direct,
        initiator: Some(principal.id.clone()),
        delegation_id: None,
        scopes: principal.scopes.clone(),
        jwt_id: Some(JwtId::new(uuid::Uuid::new_v4().to_string()).unwrap()),
        issued_at: now,
        not_before: None,
        expires_at: now + TimeDelta::seconds(25),
    };
    AuthenticatedSubject {
        extensions: Default::default(),
        audit: veoveo_mcp_contract::audit::AuditRequest::background(),
        access_token,
        principal: principal.clone(),
        actor: principal,
        principal_display_name: None,
        authority,
    }
}

#[tokio::test]
async fn session_bootstrap_is_available_without_inventory_authority_and_tracks_current_policy() {
    let state = crate::console::ConsoleState {
        catalog: GatewayCatalogHandle::new(Arc::new(
            GatewayCatalog::from_control_plane(control(), crate::catalog_admission().unwrap())
                .unwrap(),
        )),
        offline_mode: true,
    };
    let profile = GatewayProfileId::new("operator").unwrap();
    for admin in [false, true, false] {
        let mut next = control();
        if admin {
            let rule = &mut next.policies[0].rules[0];
            rule.actions = [GatewayAction::AdminRead.into()].into_iter().collect();
            rule.servers.clear();
            rule.tools.clear();
        }
        state
            .catalog
            .replace(Arc::new(
                GatewayCatalog::from_control_plane(next, state.catalog.current().admission())
                    .unwrap(),
            ))
            .unwrap();
        let response = crate::console::bootstrap(
            State(state.clone()),
            axum::extract::Path(profile.clone()),
            Extension(subject()),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
            .await
            .unwrap();
        let bootstrap: ConsoleBootstrap = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(bootstrap.can_read_installation, admin);
        assert_eq!(bootstrap.session.actor_id, subject().actor.id);
        assert_eq!(
            bootstrap.session.work_context,
            subject().authority.work_context
        );
        assert!(bootstrap.installation.offline_mode);
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(value.get("principals").is_none());
        assert!(value.get("servers").is_none());
    }
    assert_eq!(
        veoveo_mcp_gateway::http::profile_from_route(
            "/console-api/{profile}/session",
            "/console-api/operator/session"
        ),
        Some(profile)
    );
}
