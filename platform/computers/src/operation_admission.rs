use crate::task_references::LifecycleReference;
use crate::{
    Computer, ComputerActor, ComputerError, ComputersStore, Operation, Result,
    api::{Action, ComputerPhase},
    automation_grants::AutomationAuthority,
    identity::{can_mutate, digest, owner_key, permits},
    model::computer_record,
    operation_reads::OperationParticipant,
};
use serde::Serialize;
use std::collections::BTreeSet;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::task_record_id;
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskRetentionPin, TaskRuntime};
use veoveo_types::TaskTypeDefinition;

#[derive(Serialize, SurrealValue)]
struct Content {
    operation_id: Uuid,
    computer_id: Uuid,
    task: RecordId,
    task_tenant: RecordId,
    actor_context: veoveo_platform_store::TaskOwnerRecord,
    owner_context: veoveo_platform_store::TaskOwnerRecord,
    automation_grant_id: Option<Uuid>,
    execution_authority: crate::AcceptedAuthority,
    provider_instance_id: Uuid,
    template_fingerprint: String,
    replacement_instance_id: Option<Uuid>,
    action: String,
}

pub(crate) fn operation_record(id: veoveo_types::TaskId) -> RecordId {
    RecordId::new(
        "computer_operation",
        surrealdb::types::Uuid::from(id.as_uuid()),
    )
}
impl ComputersStore {
    /// Resolve accepted work without requiring currently available compute. The
    /// original input and private owner remain authoritative on every retry.
    pub async fn operation_for_request(
        &self,
        caller: &TaskOwner,
        computer: veoveo_computers_contract::ComputerId,
        request_id: crate::api::RequestId,
        action: Action,
    ) -> Result<Option<Operation>> {
        let mut response = self
            .query(
                include_str!("../queries/operation_admission/operation_for_request.surql"),
                vec![(
                    "request",
                    request_record(caller, computer, request_id)?.into_value(),
                )],
            )
            .await?;
        let id: Option<Uuid> = response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let Some(id) = id else {
            return Ok(None);
        };
        let operation = self
            .operation(caller, veoveo_types::TaskId::from_uuid(id))
            .await?;
        if operation.computer_id != computer
            || operation.action != action
            || operation.automation_grant_id.is_some()
        {
            return Err(ComputerError::RequestConflict);
        }
        Ok(Some(operation))
    }
    /// Commit the request, Computer fence and audit record before linking the shared Task.
    /// This method never dispatches a provider effect.
    pub async fn queue_operation(
        &self,
        actor: ComputerActor,
        computer_id: veoveo_computers_contract::ComputerId,
        request_id: crate::api::RequestId,
        action: Action,
    ) -> Result<Operation> {
        actor.check_admission()?;
        let caller = actor.owner();
        let computer = self.get(caller, computer_id).await?;
        can_mutate(caller)?;
        self.queue_lifecycle(&actor, computer, request_id, action, None)
            .await
    }

    /// Named authority cannot select or impersonate the retained owner. Admission
    /// consumes its current snapshot and fences its grant revision transactionally.
    pub async fn queue_automation_operation(
        &self,
        actor: &ComputerActor,
        authority: AutomationAuthority,
        request_id: crate::api::RequestId,
        action: Action,
    ) -> Result<Operation> {
        authority.require_actor(actor)?;
        if authority.permission() != super::operation_authority::permission(action)? {
            return Err(ComputerError::Forbidden);
        }
        let computer = authority.computer()?.clone();
        self.queue_lifecycle(actor, computer, request_id, action, Some(authority))
            .await
    }

    async fn queue_lifecycle(
        &self,
        actor: &ComputerActor,
        computer: Computer,
        request_id: crate::api::RequestId,
        action: Action,
        authority: Option<AutomationAuthority>,
    ) -> Result<Operation> {
        actor.check_admission()?;
        if computer.provider_instance_id != self.provider_instance_id {
            return Err(ComputerError::InvalidInput);
        }
        let caller = actor.owner();
        let computer_id = computer.computer_id;
        let grant_id = authority.as_ref().map(AutomationAuthority::grant_id);
        let key = owner_key(&computer.owner)?;
        let request = request_record(caller, computer_id, request_id)?;
        let fingerprint = match grant_id {
            Some(grant) => digest(&(
                "veoveo.computer.operation.input.v2",
                computer_id,
                action,
                grant,
            ))?,
            None => digest(&("veoveo.computer.operation.input.v1", computer_id, action))?,
        };
        let id = veoveo_types::TaskId::new();
        let audit_activity = crate::audit::lifecycle_activity(action);
        let (action, previous, next) = match action {
            Action::Create => (
                "create",
                ComputerPhase::Reserved,
                ComputerPhase::Provisioning,
            ),
            Action::Start => ("start", ComputerPhase::Stopped, ComputerPhase::Starting),
            Action::Stop => ("stop", ComputerPhase::Ready, ComputerPhase::Stopping),
        };
        let content = Content {
            operation_id: id.as_uuid(),
            computer_id: computer_id.as_uuid(),
            task: task_record_id(id),
            task_tenant: crate::identity::task_tenant(caller)?,
            actor_context: crate::identity::stored_owner(caller)?,
            owner_context: crate::identity::stored_owner(&computer.owner)?,
            automation_grant_id: grant_id.map(crate::api::AutomationGrantId::as_uuid),
            execution_authority: actor.accepted().clone(),
            provider_instance_id: computer.provider_instance_id.as_uuid(),
            template_fingerprint: computer.template_fingerprint,
            replacement_instance_id: computer.replacement_instance_id,
            action: action.into(),
        };

        let mut bindings = vec![
            ("request", request.clone().into_value()),
            (
                "admission_expires_at",
                actor.admission_expires_at().into_value(),
            ),
            ("fingerprint", fingerprint.into_value()),
            ("computer", computer_record(computer_id).into_value()),
            (
                "execution_slot",
                crate::commands::slot(computer_id).into_value(),
            ),
            ("owner_key", key.into_value()),
            (
                "expected_owner_context",
                crate::identity::stored_owner(&computer.owner)?.into_value(),
            ),
            ("automation", grant_id.is_some().into_value()),
            ("operation", operation_record(id).into_value()),
            ("content", content.into_value()),
            ("expected_updated_at", computer.updated_at.into_value()),
            ("previous_phase", phase(previous).into_value()),
            ("next_phase", phase(next).into_value()),
        ];
        bindings.push(crate::audit::binding(
            self.platform.audit_targets(),
            actor.accepted(),
            computer_id,
            crate::audit::Transition::accepted(
                audit_activity,
                veoveo_audit_contract::ComputerAuditStage::Queued,
            )
            .task(id),
        )?);
        if let Some(authority) = &authority {
            bindings.extend(authority.transaction_bindings()?);
            bindings.push(("policy", self.automation_policy_record().into_value()));
        }
        self.query(include_str!("../queries/queue_operation.surql"), bindings)
            .await?;
        let mut selected = self
            .query(
                include_str!("../queries/operation_admission/operation_for_request.surql"),
                vec![("request", request.into_value())],
            )
            .await?;
        let operation: Option<Uuid> = selected.take(0).map_err(|_| ComputerError::Unavailable)?;
        let lookup = self
            .operation_lookup(
                caller,
                veoveo_types::TaskId::from_uuid(operation.ok_or(ComputerError::Unavailable)?),
                OperationParticipant::Actor,
            )
            .await?;
        let operation = self
            .read_admitted_operation(caller, lookup, &computer.owner)
            .await?;
        permits(&operation.actor, caller)?;
        if operation.automation_grant_id != grant_id {
            return Err(ComputerError::RequestConflict);
        }
        Ok(operation)
    }
    pub async fn operation(
        &self,
        caller: &TaskOwner,
        id: veoveo_types::TaskId,
    ) -> Result<Operation> {
        let lookup = self
            .operation_lookup(caller, id, OperationParticipant::Owner)
            .await?;
        let computer = self.get(caller, lookup.computer).await?;
        self.read_admitted_operation(caller, lookup, &computer.owner)
            .await
    }
    /// Idempotent second half of acceptance. The durable operation reconstructs the
    /// same Task after a process crash or lost task-creation reply.
    pub async fn ensure_operation_task(
        &self,
        caller: &TaskOwner,
        id: veoveo_types::TaskId,
    ) -> Result<Operation> {
        let operation = self.operation(caller, id).await?;
        self.link_operation_task(operation).await
    }
    pub(crate) async fn link_operation_task(&self, operation: Operation) -> Result<Operation> {
        let id = operation.operation_id;
        let reference = serde_json::to_value(LifecycleReference {
            computer_id: operation.computer_id,
            operation_id: operation.task_id(),
        })
        .map_err(|_| ComputerError::Unavailable)?;
        let runtime = TaskRuntime::new(self.platform.clone(), "computers", "admission");
        let task = runtime
            .create(CreateTask {
                task_id: operation.task_id(),
                owner: operation.actor.clone(),
                server: "computers".into(),
                task_type: crate::api::ComputerTaskKind::Lifecycle.name(),
                request: reference.clone(),
                recovery_class: RecoveryClass::ProviderWait,
                idempotency_key: Some(format!("computer-operation/{id}")),
                ttl_ms: None,
                poll_interval_ms: Some(1000),
                retention_pins: BTreeSet::from([TaskRetentionPin::new(format!(
                    "computer-operation/{id}"
                ))
                .map_err(|_| ComputerError::Unavailable)?]),
            })
            .await
            .map_err(|_| ComputerError::Unavailable)?
            .snapshot;
        if task.task_id != operation.task_id()
            || task.request != reference
            || task.recovery_class != RecoveryClass::ProviderWait
            || task.owner != operation.actor
        {
            return Err(ComputerError::Unavailable);
        }
        Ok(operation)
    }
}
pub(crate) fn request_record(
    caller: &TaskOwner,
    computer: veoveo_computers_contract::ComputerId,
    request: crate::api::RequestId,
) -> Result<RecordId> {
    Ok(RecordId::new(
        "computer_operation_request",
        digest(&(
            "veoveo.computer.operation.request.v1",
            owner_key(caller)?,
            computer,
            request,
        ))?,
    ))
}
fn phase(value: ComputerPhase) -> String {
    match value {
        ComputerPhase::Reserved => "reserved",
        ComputerPhase::Provisioning => "provisioning",
        ComputerPhase::Stopped => "stopped",
        ComputerPhase::Starting => "starting",
        ComputerPhase::Ready => "ready",
        ComputerPhase::Stopping => "stopping",
        ComputerPhase::Error => "error",
        ComputerPhase::RecoveryRequired => "recovery_required",
    }
    .into()
}
