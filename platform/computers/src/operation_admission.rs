use crate::{
    ComputerActor, ComputerError, ComputersStore, Operation, Result,
    api::{Action, ComputerPhase},
    identity::{can_mutate, digest, owner_key, permits},
    model::computer_record,
    operation::OperationRecord,
};
use serde::Serialize;
use std::collections::BTreeSet;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::{OpenObject, OutboxDraft, deterministic_tenant_id};
use veoveo_task_runtime::{CreateTask, RecoveryClass, TaskOwner, TaskRetentionPin, TaskRuntime};

#[derive(Serialize, SurrealValue)]
struct Content {
    operation_id: Uuid,
    computer_id: Uuid,
    task: RecordId,
    actor_context: OpenObject,
    execution_authority: OpenObject,
    provider_instance_id: Uuid,
    template_fingerprint: String,
    action: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationRef {
    computer_id: Uuid,
    operation_id: Uuid,
}

pub(crate) fn operation_record(id: Uuid) -> RecordId {
    RecordId::new("computer_operation", surrealdb::types::Uuid::from(id))
}
impl ComputersStore {
    /// Resolve accepted work without requiring currently available compute. The
    /// original input and private owner remain authoritative on every retry.
    pub async fn operation_for_request(
        &self,
        caller: &TaskOwner,
        computer: Uuid,
        request_id: Uuid,
        action: Action,
    ) -> Result<Option<Operation>> {
        if request_id.is_nil() || computer.is_nil() {
            return Err(ComputerError::InvalidInput);
        }
        let mut response = self
            .query(
                "SELECT VALUE operation_id FROM ONLY $request;",
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
        let operation = self.operation(caller, id).await?;
        if operation.computer_id != computer || operation.action != action {
            return Err(ComputerError::RequestConflict);
        }
        Ok(Some(operation))
    }
    /// Commit the request, Computer fence and outbox before linking the shared Task.
    /// This method never dispatches a provider effect.
    pub async fn queue_operation(
        &self,
        actor: ComputerActor,
        computer_id: Uuid,
        request_id: Uuid,
        action: Action,
    ) -> Result<Operation> {
        actor.check_admission()?;
        let caller = actor.owner();
        if request_id.is_nil() {
            return Err(ComputerError::InvalidInput);
        }
        let computer = self.get(caller, computer_id).await?;
        can_mutate(caller)?;
        let key = owner_key(caller)?;
        let request = request_record(caller, computer_id, request_id)?;
        let fingerprint = digest(&("veoveo.computer.operation.input.v1", computer_id, action))?;
        let id = Uuid::now_v7();
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
            operation_id: id,
            computer_id,
            task: veoveo_task_runtime::TaskId::from_uuid(id).record_id(),
            actor_context: object(caller)?,
            execution_authority: object(actor.accepted())?,
            provider_instance_id: computer.provider_instance_id,
            template_fingerprint: computer.template_fingerprint,
            action: action.into(),
        };
        let event = OutboxDraft::now(
            Some(
                deterministic_tenant_id(caller.tenant_key())
                    .map_err(|_| ComputerError::InvalidInput)?
                    .record_id(),
            ),
            "computer",
            computer_id.to_string(),
            "computer.operation_queued",
            1,
            object(&Event {
                computer_id,
                operation_id: id,
                action,
                actor: &caller.principal_key,
                authority: &caller.authority,
            })?,
        );
        self.query(
            include_str!("../queries/queue_operation.surql"),
            vec![
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
                ("operation", operation_record(id).into_value()),
                ("content", content.into_value()),
                ("expected_updated_at", computer.updated_at.into_value()),
                ("previous_phase", phase(previous).into_value()),
                ("next_phase", phase(next).into_value()),
                ("event", event.into_value()),
            ],
        )
        .await?;
        let mut selected = self
            .query(
                "SELECT VALUE operation_id FROM ONLY $request;",
                vec![("request", request.into_value())],
            )
            .await?;
        let operation: Option<Uuid> = selected.take(0).map_err(|_| ComputerError::Unavailable)?;
        self.operation(caller, operation.ok_or(ComputerError::Unavailable)?)
            .await
    }
    pub async fn operation(&self, caller: &TaskOwner, id: Uuid) -> Result<Operation> {
        owner_key(caller)?;
        let mut response = self
            .query(
                "SELECT * FROM ONLY $operation;",
                vec![("operation", operation_record(id).into_value())],
            )
            .await?;
        let record: Option<OperationRecord> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        let operation = Operation::try_from(record.ok_or(ComputerError::NotFound)?)?;
        // A private operation requires the current Computer and original actor authority.
        self.get(caller, operation.computer_id).await?;
        permits(&operation.actor, caller)?;
        Ok(operation)
    }
    /// Idempotent second half of acceptance. The durable operation reconstructs the
    /// same Task after a process crash or lost task-creation reply.
    pub async fn ensure_operation_task(&self, caller: &TaskOwner, id: Uuid) -> Result<Operation> {
        let operation = self.operation(caller, id).await?;
        let reference = serde_json::to_value(OperationRef {
            computer_id: operation.computer_id,
            operation_id: id,
        })
        .map_err(|_| ComputerError::Unavailable)?;
        let runtime = TaskRuntime::new(self.platform.clone(), "computers", "admission");
        let task = runtime
            .create(CreateTask {
                task_id: operation.task_id(),
                owner: operation.actor.clone(),
                server: "computers".into(),
                task_type: "computer.lifecycle".into(),
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
        {
            return Err(ComputerError::Unavailable);
        }
        Ok(operation)
    }
}
fn request_record(caller: &TaskOwner, computer: Uuid, request: Uuid) -> Result<RecordId> {
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
#[derive(Serialize)]
struct Event<'a> {
    computer_id: Uuid,
    operation_id: Uuid,
    action: &'a str,
    actor: &'a str,
    authority: &'a veoveo_mcp_contract::InvocationAuthority,
}
fn object(value: &impl Serialize) -> Result<OpenObject> {
    let serde_json::Value::Object(fields) =
        serde_json::to_value(value).map_err(|_| ComputerError::InvalidInput)?
    else {
        return Err(ComputerError::InvalidInput);
    };
    Ok(OpenObject::new(fields.into_iter().collect()))
}
