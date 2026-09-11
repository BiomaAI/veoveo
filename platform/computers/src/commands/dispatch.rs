use super::{CommandOperation, CommandStage};
use crate::{
    ComputerError, ComputersStore, Result,
    api::{AutomationExecutionLimits, AutomationPermission},
    command_secrets::{CommandBinding, CommandKeyRing, CommandOutputAccess, CommandPayload},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use surrealdb::types::SurrealValue;
use uuid::Uuid;
use veoveo_mcp_contract::{GatewayAction, PolicyDecision, PolicyEffect, PolicyTarget};
use veoveo_task_runtime::{ClaimedTask, ProviderCommit};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDispatchDecision {
    pub control_revision: String,
    pub control_sha256: String,
    pub grant_id: Uuid,
    pub grant_revision: u64,
    pub checked_at: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub source: PolicyDecision,
    pub owner: PolicyDecision,
}
impl CommandDispatchDecision {
    pub(super) fn validate(&self, command: &CommandOperation) -> Result<()> {
        if self.grant_id != command.binding.grant_id
            || self.control_revision.is_empty()
            || self.control_sha256.len() != 64
            || !self.control_sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || self.valid_until <= self.checked_at
            || self.valid_until - self.checked_at > chrono::TimeDelta::seconds(30)
            || self.source.principal.as_ref()
                != Some(&command.authority.request_context.principal.id)
        {
            return Err(ComputerError::Unavailable);
        }
        for decision in [&self.source, &self.owner] {
            if decision.effect != PolicyEffect::Allow
                || decision.action != GatewayAction::ToolsCall
                || decision.profile != command.authority.profile
                || decision.tenant.as_ref() != Some(&command.authority.invocation.tenant)
                || decision.trace_id.as_str() != command.execution_id().to_string()
                || decision.target != execute_target()
            {
                return Err(ComputerError::Unavailable);
            }
        }
        Ok(())
    }
}
pub(crate) fn execute_target() -> PolicyTarget {
    PolicyTarget::Tool {
        server: veoveo_mcp_contract::ServerSlug::new("computers").expect("static server"),
        tool: veoveo_mcp_contract::LocalToolName::new("execute").expect("static tool"),
    }
}

/// Only one committed queued-to-dispatched transition can return this value.
/// Losing it permits containment/recovery, never a second command dispatch.
pub struct CommandDispatchTicket {
    operation: CommandOperation,
    payload: CommandPayload,
    output_access: CommandOutputAccess,
    authority_deadline: Instant,
    execution_deadline: Instant,
}
impl CommandDispatchTicket {
    pub(super) fn into_operation(self) -> CommandOperation {
        self.operation
    }
    pub fn operation(&self) -> &CommandOperation {
        &self.operation
    }
    pub fn binding(&self) -> &CommandBinding {
        &self.operation.binding
    }
    pub fn payload(&self) -> &CommandPayload {
        &self.payload
    }
    pub fn output_access(&self) -> &CommandOutputAccess {
        &self.output_access
    }
    pub fn limits(&self) -> AutomationExecutionLimits {
        self.operation
            .effective_limits
            .expect("validated dispatched limits")
    }
    pub fn authority_deadline(&self) -> Instant {
        self.authority_deadline
    }
    pub fn execution_deadline(&self) -> Instant {
        self.execution_deadline
    }
}

impl ComputersStore {
    pub async fn begin_command_dispatch(
        &self,
        claim: &ClaimedTask,
        keys: &CommandKeyRing,
    ) -> Result<CommandDispatchTicket> {
        tokio::time::timeout(Duration::from_secs(10), self.dispatch_command(claim, keys))
            .await
            .map_err(|_| ComputerError::Unavailable)?
    }
    async fn dispatch_command(
        &self,
        claim: &ClaimedTask,
        keys: &CommandKeyRing,
    ) -> Result<CommandDispatchTicket> {
        let mut operation = self.worker_command(claim).await?.operation;
        if operation.stage != CommandStage::Queued {
            return Err(ComputerError::StateConflict);
        }
        let permit = self
            .authorize_accepted_automation(
                &operation.authority,
                operation.computer_id(),
                operation.binding.grant_id,
                AutomationPermission::Execute,
            )
            .await?;
        let payload = keys.open(&operation.binding, &operation.sealed)?;
        let sealed_output = operation
            .output_access
            .as_ref()
            .ok_or(ComputerError::InvalidState)?;
        let output_access = keys.open_output_access(&operation.binding, sealed_output)?;
        let limits = permit.execution_limits()?.ok_or(ComputerError::Forbidden)?;
        let effective = AutomationExecutionLimits {
            maximum_seconds: payload.limits().maximum_seconds.min(limits.maximum_seconds),
            maximum_output_bytes: payload
                .limits()
                .maximum_output_bytes
                .min(limits.maximum_output_bytes)
                .min(output_access.maximum_output_bytes()),
            on_interruption: limits.on_interruption,
        };
        let current = permit.computer()?;
        if current.phase != crate::api::ComputerPhase::Ready {
            return Err(ComputerError::InvalidState);
        }
        if current.active_operation.is_some() {
            return Err(ComputerError::OperationBusy);
        }
        if current.provider_instance_id != operation.binding.provider_instance_id
            || crate::identity::owner_key(&current.owner)? != operation.binding.owner_key
            || current.template_fingerprint != operation.binding.template_fingerprint
            || current.provider_resource_id.as_deref() != Some(&operation.binding.resource_id)
            || current.process_id.as_deref() != Some(&operation.binding.process_id)
        {
            return Err(ComputerError::StateConflict);
        }
        let decision = permit.command_decision(operation.execution_id())?;
        decision.validate(&operation)?;
        let as_object = |value: serde_json::Value| -> Result<veoveo_platform_store::OpenObject> {
            serde_json::from_value(value).map_err(|_| ComputerError::Unavailable)
        };
        let evidence =
            as_object(serde_json::to_value(&decision).map_err(|_| ComputerError::Unavailable)?)?;
        operation.dispatch_authority = Some(decision);
        let id = Uuid::now_v7();
        let mut params = permit.transaction_bindings()?;
        params.extend([
            ("policy", self.automation_policy_record().into_value()),
            ("dispatch_id", id.into_value()),
            (
                "preparation_budget",
                surrealdb::types::Duration::from_secs(u64::from(
                    super::output_access::PREPARATION_ALLOWANCE_SECONDS,
                ))
                .into_value(),
            ),
            (
                "expected_output",
                as_object(
                    serde_json::to_value(sealed_output).map_err(|_| ComputerError::Unavailable)?,
                )?
                .into_value(),
            ),
            (
                "output_expires",
                output_access.capability().expires_at.into_value(),
            ),
            (
                "publication_budget",
                surrealdb::types::Duration::from_secs(u64::from(
                    super::output_access::OUTPUT_PUBLICATION_SECONDS,
                ))
                .into_value(),
            ),
            (
                "expected_binding",
                as_object(
                    serde_json::to_value(&operation.binding)
                        .map_err(|_| ComputerError::Unavailable)?,
                )?
                .into_value(),
            ),
            ("decision", evidence.into_value()),
            (
                "effective_limits",
                as_object(
                    serde_json::to_value(effective).map_err(|_| ComputerError::Unavailable)?,
                )?
                .into_value(),
            ),
            (
                "duration",
                surrealdb::types::Duration::from_secs(u64::from(effective.maximum_seconds))
                    .into_value(),
            ),
            (
                "expected_owner_context",
                as_object(
                    serde_json::to_value(&current.owner).map_err(|_| ComputerError::Unavailable)?,
                )?
                .into_value(),
            ),
        ]);
        self.commit_command(
            claim,
            &operation,
            ProviderCommit::Dispatch,
            include_str!("../../queries/dispatch_command.surql"),
            params,
            "computer.execution_dispatched",
        )
        .await?;
        let selected = self.worker_command(claim).await?;
        let deadline = selected
            .operation
            .execution_deadline
            .ok_or(ComputerError::Unavailable)?;
        let remaining = (deadline - selected.database_time)
            .to_std()
            .unwrap_or_default()
            .saturating_sub(selected.read_started.elapsed());
        let lease_remaining = (claim.lease_expires_at - selected.database_time)
            .to_std()
            .unwrap_or_default()
            .saturating_sub(selected.read_started.elapsed());
        // Start the short live-authority window at the actual dispatch commit,
        // never at receipt delivery after a delayed database response.
        let authority_remaining = selected.remaining(selected.operation.dispatched_at.map(|at| {
            at + chrono::TimeDelta::seconds(super::continuation::AUTHORITY_WINDOW.as_secs() as i64)
        }));
        if selected.operation.dispatch_id != Some(id)
            || selected.operation.stage != CommandStage::Dispatched
            || remaining.is_zero()
            || lease_remaining.is_zero()
            || authority_remaining.is_zero()
            || permit.valid_until() <= Instant::now()
        {
            return Err(ComputerError::StateConflict);
        }
        Ok(CommandDispatchTicket {
            operation: selected.operation,
            payload,
            output_access,
            authority_deadline: permit
                .valid_until()
                .min(Instant::now() + lease_remaining.min(authority_remaining)),
            execution_deadline: Instant::now() + remaining,
        })
    }
}
