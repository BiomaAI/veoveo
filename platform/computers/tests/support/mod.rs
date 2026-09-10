use veoveo_task_runtime::TaskOwner;
#[path = "../../../../testing/fixtures/store.rs"]
mod store;
pub use store::TestDb;
#[allow(dead_code)] // Only dispatch scenarios consume current policy.
pub mod policy;

pub const FINGERPRINT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

pub fn owner(subject: &str) -> TaskOwner {
    let principal = format!("https://computers.test#{subject}");
    serde_json::from_value(serde_json::json!({
        "principal_key": principal, "principal_kind": "user", "issuer": "https://computers.test",
        "subject": subject, "profile": "operator", "tenant_key": "test", "data_labels": [],
        "authority": {"work_context": "computers-test", "tenant": "test", "membership": "contributor",
            "policy_revision": "test-1", "output_policy": {"owner": {"kind": "principal", "id": principal}},
            "provenance": {"mode": "direct", "initiator": principal}}
    })).unwrap()
}

/// Explicit local request fixture. This is not installed authentication or policy evidence.
#[allow(dead_code)] // Collection-only scenarios do not admit lifecycle operations.
pub fn authenticated(owner: &TaskOwner) -> veoveo_computers::ComputerActor {
    veoveo_computers::ComputerActor::from_verified(&identity(owner)).unwrap()
}

#[allow(dead_code)]
pub fn identity(owner: &TaskOwner) -> veoveo_mcp_contract::GatewayInternalIdentity {
    use veoveo_mcp_contract::*;
    let principal = Principal {
        id: PrincipalId::new(owner.principal_key.clone()).unwrap(),
        kind: match owner.principal_kind {
            veoveo_platform_store::PrincipalKind::User => PrincipalKind::User,
            veoveo_platform_store::PrincipalKind::Service => PrincipalKind::Service,
        },
        issuer: TokenIssuer::new(owner.issuer.clone()).unwrap(),
        subject: TokenSubject::new(owner.subject.clone()).unwrap(),
        tenant: Some(TenantId::new(owner.tenant_key()).unwrap()),
        groups: Default::default(),
        group_roles: Default::default(),
        roles: Default::default(),
        scopes: [ScopeName::new("operator:use").unwrap()]
            .into_iter()
            .collect(),
        data_labels: owner
            .data_labels
            .iter()
            .map(|s| DataLabelId::new(s.clone()).unwrap())
            .collect(),
        assurances: Default::default(),
        authenticated_at: None,
    };
    let now = chrono::Utc::now();
    let access_token = AccessTokenSubject {
        issuer: principal.issuer.clone(),
        subject: principal.subject.clone(),
        oauth_client_id: OAuthClientId::new(
            if owner.authority.provenance.mode() == InvocationMode::Automated {
                &owner.subject
            } else {
                "console"
            },
        )
        .unwrap(),
        session_family: None,
        audience: ProtectedResourceId::new("https://computers.test/mcp/operator").unwrap(),
        work_context: owner.authority.work_context.clone(),
        invocation_mode: owner.authority.provenance.mode(),
        initiator: owner.authority.provenance.initiator().cloned(),
        delegation_id: match &owner.authority.provenance {
            InvocationProvenance::Delegated { delegation_id, .. } => Some(delegation_id.clone()),
            _ => None,
        },
        scopes: principal.scopes.clone(),
        jwt_id: Some(JwtId::new(uuid::Uuid::now_v7().to_string()).unwrap()),
        issued_at: now,
        not_before: None,
        expires_at: now + chrono::TimeDelta::hours(1),
    };
    GatewayInternalIdentity {
        issuer: TokenIssuer::new("veoveo-internal").unwrap(),
        profile: GatewayProfileId::new(owner.profile.clone()).unwrap(),
        server: ServerSlug::new("computers").unwrap(),
        actor: principal.clone(),
        authority: owner.authority.clone(),
        request_context: Some(GatewayRequestContext {
            principal,
            access_token,
        }),
        jwt_id: JwtId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
        issued_at: now,
        not_before: now,
        expires_at: now + chrono::TimeDelta::minutes(1),
    }
}
