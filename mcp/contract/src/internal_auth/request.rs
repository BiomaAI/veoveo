//! Signed request metadata for current policy checks; contains no bearer token.
use super::*;
use crate::{AccessTokenSubject, InvocationMode, InvocationProvenance, PrincipalKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GatewayRequestContext {
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
        if !common
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
