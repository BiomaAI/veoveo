use chrono::{TimeDelta, Utc};
use std::collections::BTreeSet;
use veoveo_mcp_contract::{
    AccessSubject, DataLabelId, GatewayInternalIdentity, GatewayProfileId, InvocationAuthority,
    InvocationProvenance, JwtId, PolicyVersion, Principal, PrincipalId, PrincipalKind, ServerSlug,
    TenantId, TokenIssuer, TokenSubject, WorkContextId, WorkContextMembershipLevel,
    WorkContextOutputPolicy,
};
pub(super) fn identity(
    tenant: &str,
    context: &str,
    name: &str,
    labels: &[&str],
) -> GatewayInternalIdentity {
    let principal = PrincipalId::new(name).unwrap();
    let tenant = TenantId::new(tenant).unwrap();
    let now = Utc::now();
    GatewayInternalIdentity {
        issuer: TokenIssuer::new("https://gateway.example").unwrap(),
        profile: GatewayProfileId::new("uav-index-test").unwrap(),
        server: ServerSlug::new("uav-sim").unwrap(),
        actor: Principal {
            id: principal.clone(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://identity.example").unwrap(),
            subject: TokenSubject::new(name).unwrap(),
            tenant: Some(tenant.clone()),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::from([veoveo_types::ScopeName::new("uav-sim:control").unwrap()]),
            assurances: BTreeSet::new(),
            authenticated_at: None,
            data_labels: labels
                .iter()
                .map(|label| DataLabelId::new(*label).unwrap())
                .collect(),
        },
        authority: InvocationAuthority {
            work_context: WorkContextId::new(context).unwrap(),
            tenant,
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::new("r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(principal.clone()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Direct {
                initiator: principal,
            },
        },
        request_context: None,
        jwt_id: JwtId::new(uuid::Uuid::now_v7().to_string()).unwrap(),
        issued_at: now,
        not_before: now,
        expires_at: now + TimeDelta::minutes(5),
    }
}

#[path = "../../../../testing/fixtures/store.rs"]
pub(super) mod fixture;
