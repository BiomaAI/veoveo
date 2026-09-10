//! Accepted execution identity is separate from the lifetime of an admission token.
use crate::{ComputerError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{
    GatewayInternalIdentity, GatewayProfileId, GatewayRequestContext, InvocationAuthority,
    Principal,
};
use veoveo_task_runtime::TaskOwner;

/// Verified request boundary for accepting new Computer work. Public request bodies
/// never deserialize this type. Facades construct it only after verifying the JWT.
pub struct ComputerActor {
    accepted: AcceptedAuthority,
    assertion_expires_at: DateTime<Utc>,
    owner: TaskOwner,
}

/// Immutable evidence for one accepted operation; no bearer token is retained.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedAuthority {
    pub(crate) actor: Principal,
    pub(crate) profile: GatewayProfileId,
    pub(crate) invocation: InvocationAuthority,
    pub(crate) request_context: GatewayRequestContext,
}

impl ComputerActor {
    pub fn from_verified(identity: &GatewayInternalIdentity) -> Result<Self> {
        if identity.server.as_str() != "computers" || identity.not_before > Utc::now() {
            return Err(ComputerError::Forbidden);
        }
        let accepted = AcceptedAuthority {
            actor: identity.actor.clone(),
            profile: identity.profile.clone(),
            invocation: identity.authority.clone(),
            request_context: identity
                .request_context
                .clone()
                .ok_or(ComputerError::Forbidden)?,
        };
        accepted.validate()?;
        let actor = Self {
            owner: accepted.task_owner(),
            accepted,
            assertion_expires_at: identity.expires_at,
        };
        actor.check_admission()?;
        Ok(actor)
    }
    pub fn owner(&self) -> &TaskOwner {
        &self.owner
    }
    pub(crate) fn admission_expires_at(&self) -> DateTime<Utc> {
        self.assertion_expires_at
            .min(self.accepted.request_context.access_token.expires_at)
    }
    pub(crate) fn check_admission(&self) -> Result<()> {
        let now = Utc::now();
        if self.admission_expires_at() <= now {
            return Err(ComputerError::Forbidden);
        }
        Ok(())
    }
    pub(crate) fn accepted(&self) -> &AcceptedAuthority {
        &self.accepted
    }
}

impl AcceptedAuthority {
    pub(crate) fn validate(&self) -> Result<()> {
        self.request_context
            .validate_for(&self.actor, &self.invocation)
            .map_err(|_| ComputerError::Forbidden)
    }
    pub(crate) fn task_owner(&self) -> TaskOwner {
        TaskOwner {
            principal_key: self.actor.id.to_string(),
            principal_kind: match self.actor.kind {
                veoveo_mcp_contract::PrincipalKind::User => {
                    veoveo_platform_store::PrincipalKind::User
                }
                veoveo_mcp_contract::PrincipalKind::Service => {
                    veoveo_platform_store::PrincipalKind::Service
                }
            },
            issuer: self.actor.issuer.to_string(),
            subject: self.actor.subject.to_string(),
            profile: self.profile.to_string(),
            tenant_key: self.actor.tenant.as_ref().map(ToString::to_string),
            data_labels: self
                .actor
                .data_labels
                .iter()
                .map(ToString::to_string)
                .collect(),
            authority: self.invocation.clone(),
        }
    }
}
