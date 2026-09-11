use super::{
    MaintenanceOperation, MaintenanceSource, MaintenanceTarget, model::MaintenanceRecord, object,
    record,
};
use crate::{
    ComputerActor, ComputerError, ComputersStore, OperationStage, Result,
    api::{Action, ComputerPhase},
    identity::{can_mutate, digest, owner_key, permits},
    model::computer_record,
};
use chrono::{TimeDelta, Utc};
use serde::Serialize;
use std::{collections::BTreeSet, time::Duration};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::{
    OpenObject, OutboxDraft, deterministic_enterprise_id, deterministic_tenant_id,
    gateway_refresh_family_record_id,
};
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskRetentionPin, TaskRuntime};

#[derive(Serialize, SurrealValue)]
struct Content {
    operation_id: Uuid,
    request_id: Uuid,
    computer_id: Uuid,
    task: RecordId,
    owner_key: String,
    actor_context: OpenObject,
    execution_authority: OpenObject,
    provider_instance_id: Uuid,
    source_instance_id: Uuid,
    source_template_id: String,
    source_template_fingerprint: String,
    source: OpenObject,
    target_instance_id: Uuid,
    target_template_id: String,
    target_template_fingerprint: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Reference {
    computer_id: Uuid,
    maintenance_id: Uuid,
}
#[derive(Serialize)]
struct AdmissionEvent<'a> {
    computer_id: Uuid,
    maintenance_id: Uuid,
    actor: &'a str,
    authority: &'a veoveo_mcp_contract::InvocationAuthority,
    target_template_id: &'a str,
}

fn request_record(caller: &TaskOwner, computer: Uuid, request: Uuid) -> Result<RecordId> {
    if request.is_nil() || computer.is_nil() {
        return Err(ComputerError::InvalidInput);
    }
    Ok(RecordId::new(
        "computer_maintenance_request",
        digest(&(
            "veoveo.computer.maintenance.request.v1",
            owner_key(caller)?,
            computer,
            request,
        ))?,
    ))
}
impl ComputersStore {
    pub async fn maintenance(&self, caller: &TaskOwner, id: Uuid) -> Result<MaintenanceOperation> {
        let mut reply = self
            .query(
                "SELECT * FROM ONLY $maintenance;",
                vec![("maintenance", record(id).into_value())],
            )
            .await?;
        let row: Option<MaintenanceRecord> =
            reply.take(0).map_err(|_| ComputerError::Unavailable)?;
        let operation = MaintenanceOperation::try_from(row.ok_or(ComputerError::NotFound)?)?;
        if operation.operation_id != id
            || operation.provider_instance_id != self.provider_instance_id
        {
            return Err(ComputerError::Unavailable);
        }
        permits(&operation.actor, caller)?;
        self.get(caller, operation.computer_id).await?;
        Ok(operation)
    }

    /// Locate the original target before consulting a possibly changed default.
    pub async fn maintenance_for_request(
        &self,
        caller: &TaskOwner,
        computer: Uuid,
        request: Uuid,
    ) -> Result<Option<MaintenanceOperation>> {
        let mut reply = self
            .query(
                "SELECT VALUE operation_id FROM ONLY $request;",
                vec![(
                    "request",
                    request_record(caller, computer, request)?.into_value(),
                )],
            )
            .await?;
        let id: Option<Uuid> = reply.take(0).map_err(|_| ComputerError::Unavailable)?;
        match id {
            Some(id) => {
                let operation = self.maintenance(caller, id).await?;
                if operation.computer_id != computer || operation.request_id != request {
                    return Err(ComputerError::Unavailable);
                }
                Ok(Some(operation))
            }
            None => Ok(None),
        }
    }

    /// Current state eligibility for a UI projection. Admission still checks the
    /// same source and execution slot atomically before acquiring the fence.
    pub async fn maintenance_available(&self, caller: &TaskOwner, id: Uuid) -> Result<bool> {
        let computer = self.get(caller, id).await?;
        match self.maintenance_source(caller, &computer).await {
            Ok(_) => {}
            Err(ComputerError::InvalidState | ComputerError::OperationBusy) => return Ok(false),
            Err(error) => return Err(error),
        }
        let mut reply = self
            .query(
                "SELECT VALUE id FROM ONLY $slot;",
                vec![("slot", crate::commands::slot(id).into_value())],
            )
            .await?;
        let slot: Option<RecordId> = reply.take(0).map_err(|_| ComputerError::Unavailable)?;
        Ok(slot.is_none())
    }

    async fn maintenance_source(
        &self,
        caller: &TaskOwner,
        computer: &crate::Computer,
    ) -> Result<MaintenanceSource> {
        match computer.phase {
            ComputerPhase::Ready | ComputerPhase::Stopped => {
                if computer.active_operation.is_some() {
                    return Err(ComputerError::OperationBusy);
                }
                let resource_id = computer
                    .provider_resource_id
                    .clone()
                    .ok_or(ComputerError::InvalidState)?;
                let process_id = computer
                    .process_id
                    .clone()
                    .ok_or(ComputerError::InvalidState)?;
                if computer.phase == ComputerPhase::Ready {
                    Ok(MaintenanceSource::Ready {
                        resource_id,
                        process_id,
                    })
                } else {
                    Ok(MaintenanceSource::Stopped {
                        resource_id,
                        process_id,
                    })
                }
            }
            ComputerPhase::RecoveryRequired
                if computer.replacement_instance_id.is_none()
                    && computer.provider_resource_id.is_none()
                    && computer.process_id.is_none() =>
            {
                let id = computer
                    .active_operation
                    .ok_or(ComputerError::InvalidState)?;
                let original = match self.operation(caller, id).await {
                    Ok(operation) => operation,
                    Err(ComputerError::NotFound) => {
                        let maintenance = self.maintenance(caller, id).await?;
                        if maintenance.computer_id != computer.computer_id {
                            return Err(ComputerError::Unavailable);
                        }
                        return Err(ComputerError::OperationBusy);
                    }
                    Err(error) => return Err(error),
                };
                if original.computer_id != computer.computer_id
                    || original.action != Action::Create
                    || original.stage != OperationStage::RecoveryRequired
                    || original.instance_id() != computer.computer_id
                    || original.template_fingerprint != computer.template_fingerprint
                    || original.provider_instance_id != self.provider_instance_id
                    || original.dispatch_id.is_none()
                    || original
                        .observation_deadline
                        .is_none_or(|deadline| deadline > Utc::now())
                {
                    return Err(ComputerError::InvalidState);
                }
                Ok(MaintenanceSource::InitialFailure { operation_id: id })
            }
            _ => Err(ComputerError::InvalidState),
        }
    }

    /// Reserve one immutable replacement and fence all ordinary Computer control.
    /// The application must qualify the selected template transition before calling.
    /// This does not retire compute, release quota or establish allocator evidence.
    pub async fn queue_maintenance(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
        request_id: Uuid,
        target: &MaintenanceTarget,
    ) -> Result<MaintenanceOperation> {
        tokio::time::timeout(
            Duration::from_secs(5),
            self.queue_maintenance_inner(actor, computer_id, request_id, target),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    async fn queue_maintenance_inner(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
        request_id: Uuid,
        target: &MaintenanceTarget,
    ) -> Result<MaintenanceOperation> {
        target.validate()?;
        let caller = actor.owner();
        can_mutate(caller)?;
        let snapshot = self.maintenance_admission_authority(actor).await?;
        let request = request_record(caller, computer_id, request_id)?;
        if let Some(prior) = self
            .maintenance_for_request(caller, computer_id, request_id)
            .await?
        {
            if prior.target != *target {
                return Err(ComputerError::RequestConflict);
            }
            return self
                .ensure_maintenance_task(caller, prior.operation_id)
                .await;
        }
        let computer = self.get(caller, computer_id).await?;
        // A concurrent exact request can commit after the first request lookup
        // and before this Computer read. Resolve its identity before interpreting
        // the newly acquired fence as a different operation.
        if computer.active_operation.is_some()
            && let Some(prior) = self
                .maintenance_for_request(caller, computer_id, request_id)
                .await?
        {
            if prior.target != *target {
                return Err(ComputerError::RequestConflict);
            }
            return self
                .ensure_maintenance_task(caller, prior.operation_id)
                .await;
        }
        if computer.provider_instance_id != self.provider_instance_id {
            return Err(ComputerError::Unavailable);
        }
        let source = self.maintenance_source(caller, &computer).await?;
        source.validate()?;
        let source_operation = match source {
            MaintenanceSource::InitialFailure { operation_id } => {
                Some(crate::operation_admission::operation_record(operation_id))
            }
            _ => None,
        };
        let id = Uuid::now_v7();
        let content = Content {
            operation_id: id,
            request_id,
            computer_id,
            task: veoveo_task_runtime::TaskId::from_uuid(id).record_id(),
            owner_key: owner_key(caller)?,
            actor_context: object(caller)?,
            execution_authority: object(actor.accepted())?,
            provider_instance_id: self.provider_instance_id,
            source_instance_id: computer.instance_id(),
            source_template_id: computer.template_id.clone(),
            source_template_fingerprint: computer.template_fingerprint.clone(),
            source: object(&source)?,
            target_instance_id: Uuid::now_v7(),
            target_template_id: target.template_id.clone(),
            target_template_fingerprint: target.template_fingerprint.clone(),
        };
        let event = OutboxDraft::now(
            Some(
                deterministic_tenant_id(caller.tenant_key())
                    .map_err(|_| ComputerError::InvalidInput)?
                    .record_id(),
            ),
            "computer",
            computer_id.to_string(),
            "computer.maintenance_queued",
            1,
            object(&AdmissionEvent {
                computer_id,
                maintenance_id: id,
                actor: &caller.principal_key,
                authority: &caller.authority,
                target_template_id: &target.template_id,
            })?,
        );
        let family = actor
            .accepted()
            .request_context
            .access_token
            .session_family
            .as_ref()
            .map(|id| {
                id.as_str()
                    .parse()
                    .map(gateway_refresh_family_record_id)
                    .map_err(|_| ComputerError::Forbidden)
            })
            .transpose()?;
        snapshot.check_fresh()?;
        self.query(
            include_str!("../../queries/queue_maintenance.surql"),
            vec![
                ("request", request.into_value()),
                (
                    "fingerprint",
                    digest(&("veoveo.computer.maintenance.input.v1", computer_id, target))?
                        .into_value(),
                ),
                ("computer", computer_record(computer_id).into_value()),
                ("maintenance", record(id).into_value()),
                ("content", content.into_value()),
                ("event", event.into_value()),
                (
                    "execution_slot",
                    RecordId::new(
                        "computer_execution_slot",
                        surrealdb::types::Uuid::from(computer_id),
                    )
                    .into_value(),
                ),
                ("expected_updated_at", computer.updated_at.into_value()),
                (
                    "expected_owner_context",
                    object(&computer.owner)?.into_value(),
                ),
                (
                    "expected_phase",
                    serde_json::to_value(computer.phase)
                        .map_err(|_| ComputerError::Unavailable)?
                        .as_str()
                        .ok_or(ComputerError::Unavailable)?
                        .to_owned()
                        .into_value(),
                ),
                (
                    "expected_resource_id",
                    computer.provider_resource_id.into_value(),
                ),
                ("expected_process_id", computer.process_id.into_value()),
                (
                    "expected_replacement",
                    computer.replacement_instance_id.into_value(),
                ),
                ("source_operation", source_operation.into_value()),
                (
                    "expected_active_operation",
                    computer.active_operation.into_value(),
                ),
                (
                    "authority_expires_at",
                    actor
                        .admission_expires_at()
                        .min(snapshot.checked_at + TimeDelta::seconds(5))
                        .into_value(),
                ),
                ("authority_revision", snapshot.revision_record.into_value()),
                (
                    "authority_revision_id",
                    snapshot.control_revision.into_value(),
                ),
                ("authority_sha256", snapshot.control_sha256.into_value()),
                (
                    "authority_enterprise",
                    deterministic_enterprise_id().record_id().into_value(),
                ),
                ("authority_tenant", snapshot.tenant.into_value()),
                ("authority_source", snapshot.source.into_value()),
                ("authority_actor", snapshot.actor.into_value()),
                ("family", family.into_value()),
            ],
        )
        .await?;
        let queued = self
            .maintenance_for_request(caller, computer_id, request_id)
            .await?
            .ok_or(ComputerError::Unavailable)?;
        self.ensure_maintenance_task(caller, queued.operation_id)
            .await
    }

    pub async fn ensure_maintenance_task(
        &self,
        caller: &TaskOwner,
        id: Uuid,
    ) -> Result<MaintenanceOperation> {
        let operation = self.maintenance(caller, id).await?;
        let reference = serde_json::to_value(Reference {
            computer_id: operation.computer_id,
            maintenance_id: id,
        })
        .map_err(|_| ComputerError::Unavailable)?;
        let runtime = TaskRuntime::new(self.platform.clone(), "computers", "maintenance-admission");
        let task = runtime
            .create(CreateTask {
                task_id: operation.task_id(),
                owner: operation.actor.clone(),
                server: "computers".into(),
                task_type: "computer.maintenance".into(),
                request: reference.clone(),
                recovery_class: RecoveryClass::ProviderWait,
                idempotency_key: Some(format!("computer-maintenance/{id}")),
                ttl_ms: None,
                poll_interval_ms: Some(1000),
                retention_pins: BTreeSet::from([TaskRetentionPin::new(format!(
                    "computer-maintenance/{id}"
                ))
                .map_err(|_| ComputerError::Unavailable)?]),
            })
            .await
            .map_err(|_| ComputerError::Unavailable)?
            .snapshot;
        if task.task_id != operation.task_id()
            || task.request != reference
            || task.owner != operation.actor
            || task.recovery_class != RecoveryClass::ProviderWait
        {
            return Err(ComputerError::Unavailable);
        }
        Ok(operation)
    }
}
