//! Encrypted checkpoint persistence shares the atomic Capture settlement.
use super::{
    MaintenanceEvidence, MaintenanceOperation, MaintenanceSource, MaintenanceStep,
    MaintenanceTicket, journal::policy_record,
};
use crate::{
    ComputerError, ComputersStore, Result,
    identity::{digest, owner_key},
    secrets::{
        ComputerKeyRing, MaintenanceBinding, MaintenanceCheckpoint, SealedMaintenanceCheckpoint,
    },
};
use serde::Deserialize;
use surrealdb::types::SurrealValue;
use veoveo_platform_store::OpenObject;
use veoveo_task_runtime::ClaimedTask;

#[derive(Deserialize, SurrealValue)]
struct PolicyRecord {
    operation_id: uuid::Uuid,
    envelope: OpenObject,
}
impl ComputersStore {
    pub async fn complete_maintenance_capture(
        &self,
        claim: &ClaimedTask,
        ticket: MaintenanceTicket,
        keys: &ComputerKeyRing,
        checkpoint: &MaintenanceCheckpoint,
    ) -> Result<MaintenanceOperation> {
        if ticket.step() != MaintenanceStep::Capture {
            return Err(ComputerError::InvalidState);
        }
        let binding = self.maintenance_binding(ticket.operation()).await?;
        let sealed = keys.seal_maintenance(&binding, checkpoint)?;
        self.settle_maintenance_step(claim, ticket, MaintenanceEvidence::Captured, Some(&sealed))
            .await
    }
    /// Private worker read. A missing key or altered binding cannot recapture or
    /// replace the original policy after source retirement.
    pub async fn maintenance_checkpoint(
        &self,
        claim: &ClaimedTask,
        keys: &ComputerKeyRing,
    ) -> Result<MaintenanceCheckpoint> {
        let operation = self.maintenance_for_claim(claim).await?;
        if !operation
            .steps()
            .iter()
            .any(|step| step.evidence == Some(MaintenanceEvidence::Captured))
        {
            return Err(ComputerError::InvalidState);
        }
        let binding = self.maintenance_binding(&operation).await?;
        let mut read = self
            .query(
                "SELECT * FROM ONLY $policy;",
                vec![("policy", policy_record(operation.operation_id).into_value())],
            )
            .await?;
        let row: Option<PolicyRecord> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let row = row.ok_or(ComputerError::Unavailable)?;
        if row.operation_id != operation.operation_id {
            return Err(ComputerError::Unavailable);
        }
        let sealed: SealedMaintenanceCheckpoint = serde_json::from_value(
            serde_json::to_value(row.envelope).map_err(|_| ComputerError::Unavailable)?,
        )
        .map_err(|_| ComputerError::Unavailable)?;
        keys.open_maintenance(&binding, &sealed)
    }
    async fn maintenance_binding(
        &self,
        operation: &MaintenanceOperation,
    ) -> Result<MaintenanceBinding> {
        let computer = self.get(&operation.actor, operation.computer_id).await?;
        let (resource, process) = match &operation.source {
            MaintenanceSource::Ready {
                resource_id,
                process_id,
            }
            | MaintenanceSource::Stopped {
                resource_id,
                process_id,
            } => (resource_id, process_id),
            _ => return Err(ComputerError::InvalidState),
        };
        let mut labels = computer
            .owner
            .data_labels
            .iter()
            .map(|l| {
                veoveo_mcp_contract::DataLabelId::new(l.clone())
                    .map_err(|_| ComputerError::Unavailable)
            })
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        for output in [
            &computer.owner.authority.output_policy,
            &operation.actor.authority.output_policy,
        ] {
            labels.extend(output.data_labels.iter().cloned());
            labels.extend(output.classification.iter().cloned());
        }
        Ok(MaintenanceBinding {
            operation_id: operation.operation_id,
            request_id: operation.request_id,
            computer_id: operation.computer_id,
            provider_instance_id: operation.provider_instance_id,
            owner_key: owner_key(&computer.owner)?,
            actor_key: digest(&(
                "veoveo.computer.maintenance-actor.v1",
                owner_key(&operation.actor)?,
                &operation.execution_authority.request_context.principal.id,
                &operation
                    .execution_authority
                    .request_context
                    .access_token
                    .oauth_client_id,
            ))?,
            source_instance_id: operation.source_instance_id,
            target_instance_id: operation.target_instance_id,
            source_template_fingerprint: operation.source_template_fingerprint.clone(),
            target_template_fingerprint: operation.target.template_fingerprint.clone(),
            source_resource_id: resource.clone(),
            source_process_id: process.clone(),
            required_labels: labels,
        })
    }
}
