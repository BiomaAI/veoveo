//! Signed request metadata for current policy checks; contains no bearer token.
use super::*;
use crate::{AccessTokenSubject, PrincipalKind};
use veoveo_types::{InvocationMode, InvocationProvenance};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GatewayRequestContext {
    pub format: GatewayRequestContextFormat,
    /// Gateway-established HTTP correlation, authenticated by the internal assertion.
    pub audit: veoveo_audit_contract::AuditRequest,
    pub access_token: AccessTokenSubject,
    /// JWT-verified source principal before delegated actor derivation.
    pub principal: Principal,
}

impl GatewayRequestContext {
    pub fn validate_for(
        &self,
        actor: &Principal,
        authority: &InvocationAuthority,
    ) -> Result<(), InternalTokenError> {
        let source = &self.principal;
        let token = &self.access_token;
        let common = token.issuer == source.issuer
            && token.subject == source.subject
            && token.scopes == source.scopes
            && token.work_context == authority.work_context
            && source.tenant.as_ref() == Some(&authority.tenant)
            && actor.tenant == source.tenant
            && token.invocation_mode == authority.provenance.mode();
        let provenance = match &authority.provenance {
            InvocationProvenance::Direct { initiator } => {
                actor == source
                    && initiator == &source.id
                    && token.initiator.as_ref() == Some(initiator)
                    && token.delegation_id.is_none()
            }
            InvocationProvenance::Automated => {
                actor == source
                    && source.kind == PrincipalKind::Service
                    && source.subject.as_str() == token.oauth_client_id.as_str()
                    && token.initiator.is_none()
                    && token.delegation_id.is_none()
                    && token.session_family.is_none()
            }
            InvocationProvenance::Delegated {
                initiator,
                delegation_id,
            } => {
                source.kind == PrincipalKind::User
                    && initiator == &source.id
                    && token.initiator.as_ref() == Some(initiator)
                    && token.delegation_id.as_ref() == Some(delegation_id)
                    && actor.kind == PrincipalKind::Service
                    && actor.id.as_str() == format!("{}#{}", token.issuer, token.oauth_client_id)
                    && actor.issuer == token.issuer
                    && actor.subject.as_str() == token.oauth_client_id.as_str()
                    && actor.scopes == source.scopes
                    && actor.data_labels == source.data_labels
                    && actor.assurances == source.assurances
                    && actor.groups.is_empty()
                    && actor.group_roles.is_empty()
                    && actor.roles.is_empty()
                    && token.session_family.is_none()
            }
        };
        if (token.managed_execution.is_some()
            && (token.invocation_mode != InvocationMode::Automated
                || source.kind != PrincipalKind::Service
                || token.session_family.is_some()))
            || !common
            || !provenance
            || (token.session_family.is_some()
                && (source.kind != PrincipalKind::User
                    || token.invocation_mode != InvocationMode::Direct))
        {
            return Err(InternalTokenError::InvalidRequestContext);
        }
        Ok(())
    }
}

impl GatewayRequestContext {
    /// Derive audit attribution only after checking the signed invocation relationship.
    pub fn audit_context(
        &self,
        actor: &Principal,
        authority: &InvocationAuthority,
        profile: &GatewayProfileId,
    ) -> Result<veoveo_audit_contract::AuditContext, InternalTokenError> {
        use veoveo_audit_contract::*;
        self.validate_for(actor, authority)?;
        let managed_agent = self.access_token.managed_execution.clone();
        Ok(AuditContext {
            actor: AuditActor {
                principal: actor.id.clone(),
                kind: match actor.kind {
                    PrincipalKind::User => AuditPrincipalKind::User,
                    PrincipalKind::Service => AuditPrincipalKind::Service,
                },
                tenant: actor.tenant.clone(),
                oauth_client: Some(self.access_token.oauth_client_id.clone()),
                session_family: self.access_token.session_family.clone(),
                delegating_principal: (self.principal.id != actor.id)
                    .then(|| self.principal.id.clone()),
                managed_agent,
            },
            authority: AuditAuthority {
                profile: Some(profile.clone()),
                work_context: Some(authority.work_context.clone()),
                policy_revision: Some(authority.policy_revision.clone()),
                scopes: actor.scopes.clone(),
                data_labels: actor.data_labels.clone(),
            },
            request: self.audit.clone(),
        })
    }
}

impl GatewayInternalIdentity {
    pub fn audit_context(&self) -> Result<veoveo_audit_contract::AuditContext, InternalTokenError> {
        self.request_context
            .as_ref()
            .ok_or(InternalTokenError::InvalidRequestContext)?
            .audit_context(&self.actor, &self.authority, &self.profile)
    }
}

/// The gateway and receiver require a coordinated drain when this signed shape changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum GatewayRequestContextFormat {
    #[serde(rename = "veoveo.ai/gateway-request-context/v2")]
    V2,
}
