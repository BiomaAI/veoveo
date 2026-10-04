use chrono::{TimeDelta, Utc};
use serde_json::json;
use veoveo_mcp_contract::*;
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::{
    PlatformStore, WorkContextMembershipRuleRecord, deterministic_tenant_id,
    deterministic_work_context_id,
};
use veoveo_types::{
    GroupId, InvocationAuthority, InvocationMode, PrincipalId, ScopeName, TenantId,
};
pub(crate) fn subject(name: &str) -> AuthenticatedSubject {
    let principal = Principal {
        id: PrincipalId::new(format!("https://workspace.test#{name}")).unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::new("https://workspace.test").unwrap(),
        subject: TokenSubject::new(name).unwrap(),
        tenant: Some(TenantId::new("test").unwrap()),
        groups: [GroupId::new("collaborators").unwrap()]
            .into_iter()
            .collect(),
        roles: Default::default(),
        group_roles: Default::default(),
        scopes: [ScopeName::new("operator:use").unwrap()]
            .into_iter()
            .collect(),
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: None,
    };
    let authority: InvocationAuthority = serde_json::from_value(json!({
        "work_context":"shared", "tenant":"test", "membership":"contributor", "policy_revision":"v1",
        "output_policy":{"owner":{"kind":"principal","id":principal.id}},
        "provenance":{"mode":"direct","initiator":principal.id}
    })).unwrap();
    let now = Utc::now();
    let access_token = AccessTokenSubject {
        managed_execution: None,
        issuer: principal.issuer.clone(),
        subject: principal.subject.clone(),
        oauth_client_id: OAuthClientId::new("workspace").unwrap(),
        session_family: Some(
            GatewayRefreshFamilyId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
        ),
        audience: ProtectedResourceId::new("https://workspace.test/mcp/operator").unwrap(),
        work_context: authority.work_context.clone(),
        invocation_mode: InvocationMode::Direct,
        initiator: Some(principal.id.clone()),
        delegation_id: None,
        scopes: principal.scopes.clone(),
        jwt_id: Some(JwtId::new(uuid::Uuid::now_v7().to_string()).unwrap()),
        issued_at: now,
        not_before: None,
        expires_at: now + TimeDelta::minutes(5),
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

pub(crate) async fn setup(store: &PlatformStore) {
    for name in ["Alice", "Bob", "Eve"] {
        let actor = subject(name);
        store
            .ensure_named_identity(
                "test",
                actor.principal.id.as_str(),
                actor.principal.issuer.as_str(),
                name,
                veoveo_platform_store::PrincipalKind::User,
                name,
            )
            .await
            .unwrap();
    }
    let rule = WorkContextMembershipRuleRecord {
        level: veoveo_platform_store::WorkContextMembershipLevel::Contributor,
        principals: vec![],
        groups: vec!["collaborators".into()],
        roles: vec![],
        oauth_clients: vec![],
    };
    store.client().query("CREATE ONLY $context SET tenant = $tenant, context_key = 'shared', title = 'Shared work',
        policy_revision = 'v1', memberships = $rules,
        output_policy = {owner_kind: 'principal', owner_key: 'Alice', initial_grants: [], data_labels: []};")
        .bind(("context", deterministic_work_context_id("test", "shared").unwrap().record_id()))
        .bind(("tenant", deterministic_tenant_id("test").unwrap().record_id()))
        .bind(("rules", vec![rule])).await.unwrap().check().unwrap();
}
