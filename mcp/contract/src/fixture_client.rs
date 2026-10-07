//! Direct-hosted owning fixture identity, using the existing runtime issuer.
use crate::{
    AccessTokenSubject, GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalSigningKey,
    GatewayInternalTokenIssuer, GatewayProfileId, GatewayRequestContext, Principal, PrincipalKind,
    ServerSlug, TokenIssuer, TokenSubject,
};
use anyhow::Result;
use chrono::{TimeDelta, Utc};
use veoveo_gateway_contract::{OAuthClientId, ProtectedResourceId};
use veoveo_types::{
    AccessSubject, InvocationAuthority, InvocationProvenance, PolicyVersion, PrincipalId,
    ScopeName, TenantId, WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
};

/// Configuration admitted by the owner before constructing its direct-hosted request.
/// Private key bytes are accepted only by the existing redacted signing-key owner.
pub struct InternalFixtureIdentity {
    pub signing_key_id: String,
    pub server: ServerSlug,
    pub profile: GatewayProfileId,
    pub work_context: WorkContextId,
    pub tenant: TenantId,
    pub subject: TokenSubject,
    pub scopes: Vec<ScopeName>,
}
pub fn issue_internal_fixture_token(
    identity: InternalFixtureIdentity,
    private_key_der: Vec<u8>,
) -> Result<String> {
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        GatewayInternalSigningKey::new(identity.signing_key_id, private_key_der)?,
    );
    let principal_issuer = TokenIssuer::parse("https://conformance.veoveo.local")?;
    let principal_subject = identity.subject;
    let principal = Principal {
        id: PrincipalId::parse(format!("{principal_issuer}#{principal_subject}"))?,
        kind: PrincipalKind::Service,
        issuer: principal_issuer,
        subject: principal_subject,
        tenant: Some(identity.tenant.clone()),
        groups: Default::default(),
        group_roles: Default::default(),
        roles: Default::default(),
        scopes: identity.scopes.into_iter().collect(),
        data_labels: Default::default(),
        assurances: Default::default(),
        authenticated_at: Some(Utc::now()),
    };
    let authority = InvocationAuthority {
        work_context: identity.work_context,
        tenant: identity.tenant.clone(),
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: PolicyVersion::parse("r1")?,
        output_policy: WorkContextOutputPolicy {
            owner: AccessSubject::Principal(principal.id.clone()),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: Default::default(),
        },
        provenance: InvocationProvenance::Automated,
    };
    let now = Utc::now();
    let expires_at = now + TimeDelta::minutes(30);
    let request_context = GatewayRequestContext {
        format: crate::GatewayRequestContextFormat::V2,
        audit: crate::audit::AuditRequest::background(),
        access_token: AccessTokenSubject {
            managed_execution: None,
            issuer: principal.issuer.clone(),
            subject: principal.subject.clone(),
            oauth_client_id: OAuthClientId::parse(principal.subject.as_str())?,
            session_family: None,
            audience: ProtectedResourceId::parse("https://conformance.veoveo.local")?,
            work_context: authority.work_context.clone(),
            invocation_mode: authority.provenance.mode(),
            initiator: None,
            delegation_id: None,
            scopes: principal.scopes.clone(),
            jwt_id: None,
            issued_at: now,
            not_before: Some(now),
            expires_at,
        },
        principal: principal.clone(),
    };
    let token = issuer.issue(
        identity.profile,
        identity.server,
        principal,
        authority,
        Some(request_context),
        expires_at,
    )?;
    Ok(token.bearer_token)
}
