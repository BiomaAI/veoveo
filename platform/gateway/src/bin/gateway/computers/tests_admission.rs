//! Actual catalog, assertion signature and durable policy audit; source auth is a fixture.
#[path = "../../../../../../testing/fixtures/store.rs"]
mod store;
use super::*;
use axum::extract::{Extension, State};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{TimeDelta, Utc};
use routes::{Operation, Route};
use veoveo_mcp_contract::*;
use veoveo_mcp_gateway::{AuthenticatedSubject, GatewayCatalog};

fn control() -> GatewayControlPlane {
    let mut control: GatewayControlPlane = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../computers/tests/support/gateway.json"
    )))
    .unwrap();
    let mut rule = control.policies[0].rules[0].clone();
    rule.id = PolicyRuleId::new("computer-access").unwrap();
    rule.actions = [GatewayAction::ResourcesRead, GatewayAction::ComputerAttach]
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
            GatewayCatalog::from_control_plane(control()).unwrap(),
        )),
        offline_mode: true,
    };
    let profile = GatewayProfileId::new("operator").unwrap();
    for admin in [false, true, false] {
        let mut next = control();
        if admin {
            let rule = &mut next.policies[0].rules[0];
            rule.actions = [GatewayAction::AdminRead].into_iter().collect();
            rule.servers.clear();
            rule.tools.clear();
        }
        state
            .catalog
            .replace(Arc::new(GatewayCatalog::from_control_plane(next).unwrap()));
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
        crate::runtime::profile_id_from_gateway_path("/console-api/operator/session"),
        Some(profile)
    );
}

#[tokio::test]
async fn admission_preserves_signed_source_context_without_admin_permission_and_audits_denials() {
    let db = store::TestDb::new().await;
    let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
    let issuer = TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap();
    let trust = GatewayInternalTrustBundle::from_json(&serde_json::json!({"keys":[{"kty":"OKP","crv":"Ed25519","x":URL_SAFE_NO_PAD.encode(key.public_key_raw()),"alg":"EdDSA","use":"sig","kid":"fixture"}]}).to_string()).unwrap();
    let verifier = GatewayInternalTokenVerifier::new(
        issuer.clone(),
        ServerSlug::new("computers").unwrap(),
        trust,
    );
    let state = ComputersState {
        catalog: GatewayCatalogHandle::new(Arc::new(
            GatewayCatalog::from_control_plane(control()).unwrap(),
        )),
        gateway_state: GatewayState::new(db.a.clone()),
        issuer: GatewayInternalTokenIssuer::new(
            issuer,
            GatewayInternalSigningKey::new("fixture", key.serialize_der()).unwrap(),
        ),
        upstream: GatewayUpstreamHttpClientPool::new(),
        origin: HeaderValue::from_static("https://computers.test"),
        slots: Arc::new(Semaphore::new(128)),
        stop: CancellationToken::new(),
    };
    let caller = subject();
    let id = uuid::Uuid::new_v4();
    let route = Route {
        operation_id: None,
        profile: GatewayProfileId::new("operator").unwrap(),
        id: Some(id),
    };
    let admitted = authority::authorize(&state, &route, Operation::Terminal(id), caller.clone())
        .await
        .unwrap();
    assert_eq!(
        admitted.url.as_str(),
        format!("http://127.0.0.1:18802/computers/admin/computers/{id}/terminal")
    );
    assert!(admitted.authorization.is_sensitive());
    let identity = verifier
        .verify(
            admitted
                .authorization
                .to_str()
                .unwrap()
                .strip_prefix("Bearer ")
                .unwrap(),
        )
        .unwrap();
    assert_eq!(identity.actor, caller.actor);
    assert_eq!(identity.authority, caller.authority);
    assert_eq!(identity.request_context, Some(caller.request_context()));
    assert!(identity.expires_at <= caller.access_token.expires_at);
    assert!(
        authority::authorize(&state, &route, Operation::Start(id), caller.clone())
            .await
            .is_ok()
    );

    let mut viewer = caller.clone();
    viewer.authority.membership = WorkContextMembershipLevel::Viewer;
    assert!(
        authority::authorize(&state, &route, Operation::Read(id), viewer.clone())
            .await
            .is_ok()
    );
    for operation in [Operation::Start(id), Operation::Terminal(id)] {
        assert_eq!(
            authority::authorize(&state, &route, operation, viewer.clone())
                .await
                .err()
                .unwrap()
                .0,
            StatusCode::FORBIDDEN
        );
    }
    let mut unbound = caller.clone();
    unbound.access_token.session_family = None;
    assert_eq!(
        authority::authorize(&state, &route, Operation::Terminal(id), unbound)
            .await
            .err()
            .unwrap()
            .0,
        StatusCode::FORBIDDEN
    );
    let mut next = control();
    next.policies[0].rules[1].effect = PolicyEffect::Deny;
    state
        .catalog
        .replace(Arc::new(GatewayCatalog::from_control_plane(next).unwrap()));
    assert_eq!(
        authority::authorize(&state, &route, Operation::Terminal(id), caller)
            .await
            .err()
            .unwrap()
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        state
            .gateway_state
            .audit_counts()
            .await
            .unwrap()
            .policy_events,
        8
    );
    // The second real store client sees the same evidence; no token or terminal payload enters it.
    assert_eq!(
        GatewayState::new(db.b.clone())
            .audit_counts()
            .await
            .unwrap()
            .policy_events,
        8
    );
}
