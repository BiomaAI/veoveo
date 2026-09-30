//! Consistent signed request attribution for Artifact domain and HTTP fixtures.
use super::*;

pub(super) fn caller(principal: &str, tenant: &str, labels: &[&str]) -> PlaneCaller {
    let now = Utc::now();
    let actor = Principal {
        id: PrincipalId::new(principal).unwrap(),
        kind: PrincipalKind::User,
        issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
        subject: TokenSubject::new(format!("subject-{principal}")).unwrap(),
        tenant: Some(TenantId::new(tenant).unwrap()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::new(),
        data_labels: labels
            .iter()
            .map(|label| DataLabelId::new(*label).unwrap())
            .collect(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(now),
    };
    let mut caller = PlaneCaller {
        bearer_token: "signed-token".into(),
        identity: GatewayInternalIdentity {
            issuer: TokenIssuer::new("veoveo-internal").unwrap(),
            profile: GatewayProfileId::new("operator").unwrap(),
            server: ServerSlug::new("media").unwrap(),
            actor: actor.clone(),
            authority: InvocationAuthority {
                work_context: WorkContextId::new("mission").unwrap(),
                tenant: TenantId::new(tenant).unwrap(),
                membership: WorkContextMembershipLevel::Owner,
                policy_revision: PolicyVersion::new("r1").unwrap(),
                output_policy: WorkContextOutputPolicy {
                    owner: AccessSubject::Principal(actor.id.clone()),
                    initial_grants: Vec::new(),
                    classification: None,
                    data_labels: BTreeSet::new(),
                },
                provenance: InvocationProvenance::Direct {
                    initiator: actor.id.clone(),
                },
            },
            request_context: None,
            jwt_id: JwtId::new(uuid::Uuid::new_v4().to_string()).unwrap(),
            issued_at: now,
            not_before: now,
            expires_at: now + TimeDelta::minutes(5),
        },
        memberships: BTreeSet::new(),
    };
    bind_request_context(&mut caller.identity);
    caller
}

pub(super) fn bind_request_context(identity: &mut GatewayInternalIdentity) {
    identity.request_context = Some(request_context(&identity.actor, &identity.authority));
}

pub(crate) fn request_context(
    actor: &Principal,
    authority: &InvocationAuthority,
) -> veoveo_mcp_contract::GatewayRequestContext {
    use veoveo_mcp_contract::{
        AccessTokenSubject, GatewayRequestContext, OAuthClientId, ProtectedResourceId,
    };
    let now = Utc::now();
    let mut principal = actor.clone();
    if let InvocationProvenance::Delegated { initiator, .. } = &authority.provenance {
        principal.id = initiator.clone();
        principal.kind = PrincipalKind::User;
        principal.subject = TokenSubject::new(format!("subject-{initiator}")).unwrap();
    }
    let context = GatewayRequestContext {
        audit: veoveo_mcp_contract::audit::AuditRequest::background(),
        principal: principal.clone(),
        access_token: AccessTokenSubject {
            managed_agent: None,
            issuer: principal.issuer.clone(),
            subject: principal.subject.clone(),
            oauth_client_id: OAuthClientId::new(if actor.kind == PrincipalKind::Service {
                actor.subject.as_str()
            } else {
                "artifact-test"
            })
            .unwrap(),
            session_family: None,
            audience: ProtectedResourceId::new("https://gateway.test/mcp/operator").unwrap(),
            work_context: authority.work_context.clone(),
            invocation_mode: authority.provenance.mode(),
            initiator: authority.provenance.initiator().cloned(),
            delegation_id: match &authority.provenance {
                InvocationProvenance::Delegated { delegation_id, .. } => {
                    Some(delegation_id.clone())
                }
                _ => None,
            },
            scopes: principal.scopes.clone(),
            jwt_id: None,
            issued_at: now,
            not_before: Some(now),
            expires_at: now + TimeDelta::minutes(5),
        },
    };
    context.validate_for(actor, authority).unwrap();
    context
}
