use chrono::{TimeDelta, Utc};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use veoveo_mcp_contract::*;
use veoveo_policy::session::{SessionFamilyAuthority, SessionRequest};

struct Fixture {
    family: Value,
    principal: Principal,
    token: AccessTokenSubject,
    profile: GatewayProfileId,
    server: AuthorizationServerId,
}
impl Fixture {
    fn new() -> Self {
        let now = Utc::now();
        let principal = Principal {
            id: PrincipalId::new("https://identity.test#one").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://gateway.test").unwrap(),
            subject: TokenSubject::new("one").unwrap(),
            tenant: Some(TenantId::new("tenant-a").unwrap()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::from([ScopeName::new("operator:use").unwrap()]),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: None,
        };
        let token = AccessTokenSubject {
            issuer: principal.issuer.clone(),
            subject: principal.subject.clone(),
            oauth_client_id: OAuthClientId::new("console").unwrap(),
            session_family: Some(
                GatewayRefreshFamilyId::new("019939b1-6770-7000-8000-000000000001").unwrap(),
            ),
            audience: ProtectedResourceId::new("https://gateway.test/mcp/operator").unwrap(),
            work_context: WorkContextId::new("mission").unwrap(),
            invocation_mode: InvocationMode::Direct,
            initiator: Some(principal.id.clone()),
            delegation_id: None,
            scopes: principal.scopes.clone(),
            jwt_id: None,
            issued_at: now,
            not_before: None,
            expires_at: now + TimeDelta::minutes(15),
        };
        let profile = GatewayProfileId::new("operator").unwrap();
        let server = AuthorizationServerId::new("veoveo").unwrap();
        let mut retained = principal.clone();
        retained.issuer = TokenIssuer::new("https://identity.test").unwrap();
        let family = json!({
            "authorization_server":server, "profile":profile,
            "oauth_client_id":token.oauth_client_id, "work_context":token.work_context,
            "principal_id":principal.id, "tenant":principal.tenant,
            "scopes":principal.scopes, "principal":{"principal":retained},
            "current_generation":0, "expires_at":now + TimeDelta::hours(1), "revoked_at":null,
        });
        Self {
            family,
            principal,
            token,
            profile,
            server,
        }
    }
    fn allowed(&self) -> bool {
        SessionFamilyAuthority::from_stored(&self.family).is_ok_and(|family| {
            family.allows(SessionRequest {
                profile: &self.profile,
                authorization_server: &self.server,
                principal: &self.principal,
                token: &self.token,
                now: Utc::now(),
            })
        })
    }
}

#[test]
fn rotation_and_display_metadata_do_not_revoke_a_bound_identity() {
    let mut fixture = Fixture::new();
    assert!(fixture.allowed());
    fixture.family["current_generation"] = json!(42);
    fixture.family["principal"]["principal_display_name"] = json!({"irrelevant":"shape"});
    fixture.family["id"] = json!("caller-checks-exact-record-id");
    assert!(fixture.allowed());
    fixture.token.scopes.clear();
    assert!(fixture.allowed());
}

#[test]
fn every_family_authority_binding_is_required() {
    for (pointer, value) in [
        ("/authorization_server", json!("other")),
        ("/profile", json!("other")),
        ("/oauth_client_id", json!("other")),
        ("/work_context", json!("other")),
        ("/principal_id", json!("other")),
        ("/tenant", json!("other")),
        ("/principal/principal/id", json!("other")),
        ("/principal/principal/subject", json!("other")),
        ("/principal/principal/kind", json!("service")),
        ("/principal/principal/tenant", json!("other")),
        ("/scopes", json!([])),
        ("/revoked_at", json!(Utc::now())),
        ("/expires_at", json!(Utc::now() - TimeDelta::seconds(1))),
        ("/current_generation", json!(-1)),
    ] {
        let mut fixture = Fixture::new();
        *fixture.family.pointer_mut(pointer).unwrap() = value;
        assert!(!fixture.allowed(), "accepted mismatch at {pointer}");
    }
    let mut fixture = Fixture::new();
    fixture.token.session_family = None;
    assert!(!fixture.allowed());
    let mut fixture = Fixture::new();
    fixture.token.issuer = TokenIssuer::new("https://other.test").unwrap();
    assert!(!fixture.allowed());
    let mut fixture = Fixture::new();
    fixture.token.subject = TokenSubject::new("other").unwrap();
    assert!(!fixture.allowed());
}

#[test]
fn corrupted_family_decoder_omits_sensitive_record_values() {
    let mut fixture = Fixture::new();
    fixture.family["expires_at"] = json!("do-not-echo-this-value");
    let error = SessionFamilyAuthority::from_stored(&fixture.family)
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "session family authority is invalid");
}
