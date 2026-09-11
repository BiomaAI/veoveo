//! Current policy for accepted execution. Admission tokens and attachment grants
//! have separate lifetimes; their expiry cannot silently cancel accepted work.
use crate::{ComputerError, ComputersStore, Operation, Result, api::Action};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use veoveo_mcp_contract::{
    GatewayAction, LocalToolName, PolicyDecision, PolicyEffect, PolicyTarget, ServerSlug, TraceId,
    WorkContextMembershipLevel,
};
use veoveo_platform_store::RecordId;

pub(crate) const AUTHORITY_LIFETIME: Duration = Duration::from_secs(30);

/// Retained with the dispatch and its audit event; contains no credentials.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionDecision {
    pub control_revision: String,
    pub control_sha256: String,
    pub checked_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub decision: PolicyDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automation: Option<AutomationLifecycleDecision>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationLifecycleDecision {
    pub grant_id: uuid::Uuid,
    pub grant_revision: u64,
    pub owner: PolicyDecision,
}
impl ExecutionDecision {
    pub(crate) fn validate(&self, operation: &Operation) -> Result<()> {
        let accepted = &operation.execution_authority;
        if self.control_revision.is_empty()
            || self.control_sha256.len() != 64
            || !self.control_sha256.bytes().all(|c| c.is_ascii_hexdigit())
            || self.valid_until <= self.checked_at
            || self.valid_until - self.checked_at > TimeDelta::seconds(30)
            || self.decision.effect != PolicyEffect::Allow
            || self.decision.profile != accepted.profile
            || self.decision.principal.as_ref() != Some(&accepted.request_context.principal.id)
            || self.decision.tenant.as_ref() != Some(&accepted.invocation.tenant)
            || self.decision.action != GatewayAction::ToolsCall
            || self.decision.target != execution_target(operation.action)
            || self.decision.trace_id.as_str() != operation.operation_id.to_string()
        {
            return Err(ComputerError::Unavailable);
        }
        match (&self.automation, operation.automation_grant_id) {
            (None, None) => {}
            (Some(grant), Some(id))
                if grant.grant_id == id
                    && grant.owner.effect == PolicyEffect::Allow
                    && grant.owner.profile.as_str() == operation.owner.profile
                    && grant.owner.principal.as_ref().map(|p| p.as_str())
                        == Some(operation.owner.principal_key.as_str())
                    && grant.owner.tenant.as_ref().map(|t| t.as_str())
                        == Some(operation.owner.tenant_key())
                    && grant.owner.action == GatewayAction::ToolsCall
                    && grant.owner.target == execution_target(operation.action)
                    && grant.owner.trace_id == self.decision.trace_id => {}
            _ => return Err(ComputerError::Unavailable),
        }
        Ok(())
    }
}

pub(crate) fn execution_target(action: Action) -> PolicyTarget {
    PolicyTarget::Tool {
        server: ServerSlug::new("computers").expect("static server"),
        tool: LocalToolName::new(match action {
            Action::Create => "create",
            Action::Start => "start",
            Action::Stop => "stop",
        })
        .expect("static tool"),
    }
}

pub(crate) struct ExecutionPermit {
    pub evidence: ExecutionDecision,
    pub deadline: Instant,
    pub revision_record: RecordId,
    pub tenant: RecordId,
    pub source: RecordId,
    pub actor: RecordId,
    pub automation: Option<crate::automation_grants::AutomationAuthority>,
}

impl ComputersStore {
    pub(crate) async fn authorize_execution(
        &self,
        operation: &Operation,
    ) -> Result<ExecutionPermit> {
        tokio::time::timeout(
            Duration::from_secs(5),
            self.read_execution_authority(operation),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    async fn read_execution_authority(&self, operation: &Operation) -> Result<ExecutionPermit> {
        if let Some(grant) = operation.automation_grant_id {
            let authority = self
                .authorize_accepted_automation(
                    &operation.execution_authority,
                    operation.computer_id,
                    grant,
                    crate::operation_authority::permission(operation.action)?,
                )
                .await?;
            return authority.lifecycle_permit(operation);
        }
        let snapshot = self.read_authority(&operation.execution_authority).await?;
        snapshot.check_fresh()?;
        let decision = snapshot.decision(
            GatewayAction::ToolsCall,
            &execution_target(operation.action),
            &TraceId::new(operation.operation_id.to_string()).expect("UUID trace"),
        );
        if !snapshot
            .membership
            .allows(WorkContextMembershipLevel::Contributor)
            || decision.effect != PolicyEffect::Allow
        {
            return Err(ComputerError::Forbidden);
        }
        Ok(ExecutionPermit {
            evidence: ExecutionDecision {
                control_revision: snapshot.control_revision,
                control_sha256: snapshot.control_sha256,
                checked_at: snapshot.checked_at,
                valid_until: snapshot.checked_at + TimeDelta::seconds(30),
                decision,
                automation: None,
            },
            deadline: snapshot.deadline,
            revision_record: snapshot.revision_record,
            tenant: snapshot.tenant,
            source: snapshot.source,
            actor: snapshot.actor,
            automation: None,
        })
    }
}
