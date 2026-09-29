//! Shared identities for native DuckDB adapter checks.
use chrono::{TimeDelta, Utc};
use std::collections::BTreeSet;
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalIdentity, GatewayProfileId, JwtId, Principal,
    PrincipalAssurance, PrincipalKind, ServerSlug, TokenIssuer, TokenSubject,
};
use veoveo_types::{
    AccessSubject, DataLabelId, GroupId, InvocationAuthority, InvocationProvenance, PolicyVersion,
    PrincipalId, RoleId, ScopeName, TenantId, WorkContextId, WorkContextMembershipLevel,
    WorkContextOutputPolicy,
};
pub(super) fn identity(profile: &str, subject: &str) -> GatewayInternalIdentity {
    let now = Utc::now();
    let issuer = TokenIssuer::new("https://idp.example.test").unwrap();
    let actor = Principal {
        id: PrincipalId::new(format!("principal-{subject}")).unwrap(),
        kind: PrincipalKind::User,
        issuer,
        subject: TokenSubject::new(subject).unwrap(),
        tenant: Some(TenantId::new("tenant-a").unwrap()),
        groups: BTreeSet::<GroupId>::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::<RoleId>::new(),
        scopes: BTreeSet::<ScopeName>::new(),
        data_labels: BTreeSet::<DataLabelId>::new(),
        assurances: BTreeSet::<PrincipalAssurance>::new(),
        authenticated_at: Some(now),
    };
    GatewayInternalIdentity {
        issuer: TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(),
        profile: GatewayProfileId::new(profile).unwrap(),
        server: ServerSlug::new("duckdb").unwrap(),
        actor: actor.clone(),
        authority: InvocationAuthority {
            work_context: WorkContextId::new("mission").unwrap(),
            tenant: TenantId::new("tenant-a").unwrap(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::new("r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(actor.id.clone()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: actor.id,
            },
        },
        request_context: None,
        jwt_id: JwtId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
        issued_at: now,
        not_before: now,
        expires_at: now + TimeDelta::minutes(5),
    }
}
