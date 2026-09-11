//! Task-lease guarded journal commits. Unknown database replies return no ticket.
use super::{MaintenanceOperation, model::MaintenanceRecord, object, record};
use crate::{ComputerError, ComputersStore, Result, current_authority::ExecutionPermit};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::time::{Duration, Instant};
use surrealdb::types::{SurrealValue, Value};
use uuid::Uuid;
use veoveo_platform_store::{OutboxDraft, deterministic_enterprise_id, deterministic_tenant_id};
use veoveo_task_runtime::{ClaimedTask, ProviderCommit, TaskError, TaskRuntime};

pub(super) struct ClockedMaintenance {
    pub operation: MaintenanceOperation,
    pub database_time: DateTime<Utc>,
    pub read_started: Instant,
}
impl ClockedMaintenance {
    pub fn remaining(&self, deadline: DateTime<Utc>) -> Duration {
        (deadline - self.database_time)
            .to_std()
            .unwrap_or_default()
            .saturating_sub(self.read_started.elapsed())
    }
}

pub(super) struct JournalChange<'a> {
    pub kind: ProviderCommit,
    pub permit: Option<&'a ExecutionPermit>,
    pub event: &'static str,
    pub checkpoint: Option<&'a crate::secrets::SealedMaintenanceCheckpoint>,
}
impl ComputersStore {
    pub async fn maintenance_for_claim(&self, claim: &ClaimedTask) -> Result<MaintenanceOperation> {
        Ok(self.worker_maintenance(claim).await?.operation)
    }
    pub(super) async fn worker_maintenance(
        &self,
        claim: &ClaimedTask,
    ) -> Result<ClockedMaintenance> {
        if claim.snapshot.server != "computers"
            || claim.snapshot.task_type != "computer.maintenance"
        {
            return Err(ComputerError::InvalidInput);
        }
        let id = Uuid::parse_str(&claim.snapshot.task_id.to_string())
            .map_err(|_| ComputerError::InvalidInput)?;
        let started = Instant::now();
        let mut read = self
            .query(
                "SELECT * FROM ONLY $maintenance; RETURN time::now();",
                vec![("maintenance", record(id).into_value())],
            )
            .await?;
        let row: Option<MaintenanceRecord> =
            read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let time: Option<DateTime<Utc>> = read.take(1).map_err(|_| ComputerError::Unavailable)?;
        let operation = MaintenanceOperation::try_from(row.ok_or(ComputerError::NotFound)?)?;
        if operation.task_id() != claim.snapshot.task_id
            || operation.actor != claim.snapshot.owner
            || operation.provider_instance_id != self.provider_instance_id
            || claim.snapshot.request
                != serde_json::json!({"computerId": operation.computer_id, "maintenanceId": id})
        {
            return Err(ComputerError::StateConflict);
        }
        Ok(ClockedMaintenance {
            operation,
            database_time: time.ok_or(ComputerError::Unavailable)?,
            read_started: started,
        })
    }

    pub(super) async fn commit_maintenance(
        &self,
        claim: &ClaimedTask,
        before: &MaintenanceOperation,
        after: &MaintenanceOperation,
        change: JournalChange<'_>,
    ) -> Result<()> {
        after.validate_progress()?;
        #[derive(Serialize)]
        struct Event<'a> {
            computer_id: Uuid,
            maintenance_id: Uuid,
            actor: &'a crate::AcceptedAuthority,
            stage: super::MaintenanceStage,
            step: Option<&'a super::MaintenanceStepRecord>,
            recovery: Option<super::MaintenanceRecovery>,
        }
        let event = OutboxDraft::now(
            Some(
                deterministic_tenant_id(before.actor.tenant_key())
                    .map_err(|_| ComputerError::InvalidInput)?
                    .record_id(),
            ),
            "computer",
            before.computer_id.to_string(),
            change.event,
            1,
            object(&Event {
                computer_id: before.computer_id,
                maintenance_id: before.operation_id,
                actor: &before.execution_authority,
                stage: after.stage,
                step: after.steps().last(),
                recovery: after.recovery(),
            })?,
        );
        let computer = self.get(&before.actor, before.computer_id).await?;
        let policy = change.permit.map(|permit| &permit.evidence);
        let (resource, process) = after
            .created_run()
            .map(|(r, p)| (Some(r.to_owned()), Some(p.to_owned())))
            .unwrap_or_default();
        let mut params = vec![
            ("maintenance", record(before.operation_id).into_value()),
            ("operation_id", before.operation_id.into_value()),
            ("provider", self.provider_instance_id.into_value()),
            (
                "computer",
                crate::model::computer_record(before.computer_id).into_value(),
            ),
            ("expected_updated_at", before.updated_at.into_value()),
            ("expected_progress", object(&before.progress)?.into_value()),
            ("expected_stage", enum_value(before.stage)?),
            ("progress", object(&after.progress)?.into_value()),
            ("stage", enum_value(after.stage)?),
            ("computer_updated_at", computer.updated_at.into_value()),
            ("source_owner", object(&computer.owner)?.into_value()),
            ("event", event.into_value()),
            ("policy", policy.map(object).transpose()?.into_value()),
            (
                "checkpoint",
                change.checkpoint.map(object).transpose()?.into_value(),
            ),
            (
                "policy_record",
                policy_record(before.operation_id).into_value(),
            ),
            ("target_resource", resource.into_value()),
            ("target_process", process.into_value()),
            (
                "initial_operation",
                match before.source {
                    super::MaintenanceSource::InitialFailure { operation_id } => Some(operation_id),
                    _ => None,
                }
                .into_value(),
            ),
        ];
        if let Some(permit) = change.permit {
            if permit.deadline <= Instant::now() {
                return Err(ComputerError::Forbidden);
            }
            params.extend([
                (
                    "authority_revision",
                    permit.revision_record.clone().into_value(),
                ),
                (
                    "authority_enterprise",
                    deterministic_enterprise_id().record_id().into_value(),
                ),
                ("authority_tenant", permit.tenant.clone().into_value()),
                ("authority_source", permit.source.clone().into_value()),
                ("authority_actor", permit.actor.clone().into_value()),
            ]);
        }
        TaskRuntime::new(self.platform.clone(), "computers", &claim.lease_owner)
            .commit_provider_journal(
                claim,
                change.kind,
                include_str!("../../queries/maintenance_commit.surql"),
                params,
            )
            .await
            .map_err(|e| match e {
                TaskError::Conflict(_) | TaskError::LeaseHeld(_) => ComputerError::StateConflict,
                _ => ComputerError::Unavailable,
            })
    }
}
pub(super) fn policy_record(id: Uuid) -> surrealdb::types::RecordId {
    surrealdb::types::RecordId::new(
        "computer_maintenance_policy",
        surrealdb::types::Uuid::from(id),
    )
}
fn enum_value(value: impl Serialize) -> Result<Value> {
    Ok(serde_json::to_value(value)
        .map_err(|_| ComputerError::Unavailable)?
        .as_str()
        .ok_or(ComputerError::Unavailable)?
        .to_owned()
        .into_value())
}
