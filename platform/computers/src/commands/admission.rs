use super::{CommandOperation, model};
use crate::{
    ComputerActor, ComputerError, ComputersStore, Result,
    api::{AutomationPermission, ComputerPhase},
    automation_grants::AutomationAuthority,
    identity::owner_key,
    model::computer_record,
    secrets::{CommandBinding, CommandPayload, ComputerKeyRing, SealedCommand},
};
use serde::Serialize;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::OpenObject;
use veoveo_platform_store::task_record_id;

#[derive(Serialize, SurrealValue)]
struct Content {
    execution_id: Uuid,
    computer_id: Uuid,
    provider_instance_id: Uuid,
    actor_key: String,
    binding: OpenObject,
    authority: OpenObject,
    payload: RecordId,
    task: RecordId,
}
#[derive(SurrealValue)]
struct Payload {
    journal: RecordId,
    #[surreal(wrap)]
    sealed: SealedCommand,
}
fn object(value: &impl Serialize) -> Result<OpenObject> {
    serde_json::from_value(serde_json::to_value(value).map_err(|_| ComputerError::Unavailable)?)
        .map_err(|_| ComputerError::Unavailable)
}
impl ComputersStore {
    /// Consume current named authority and atomically reserve its one command slot.
    /// The caller prepares authority separately; the transaction rechecks its grant
    /// revision and policy. A successful queue does not authorize native execution.
    pub async fn queue_command(
        &self,
        actor: &ComputerActor,
        authority: AutomationAuthority,
        request_id: crate::api::RequestId,
        payload: &CommandPayload,
        keys: &ComputerKeyRing,
    ) -> Result<CommandOperation> {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.queue(actor, authority, request_id, payload, keys),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    async fn queue(
        &self,
        actor: &ComputerActor,
        authority: AutomationAuthority,
        request_id: crate::api::RequestId,
        payload: &CommandPayload,
        keys: &ComputerKeyRing,
    ) -> Result<CommandOperation> {
        authority.require_actor(actor)?;
        if authority.permission() != AutomationPermission::Execute {
            return Err(ComputerError::Forbidden);
        }
        let computer = authority.computer()?;
        if computer.provider_instance_id != self.provider_instance_id {
            return Err(ComputerError::Forbidden);
        }
        let actor_key = super::actor_key(actor.accepted())?;
        let request = super::request(&actor_key, computer.computer_id, request_id)?;
        let computer_id = computer.computer_id;
        if let Some(prior) = self
            .command_request(&request, actor, &actor_key, computer_id)
            .await?
        {
            return self.match_command(
                prior,
                actor,
                computer_id,
                authority.grant_id(),
                request_id,
                payload,
                keys,
            );
        }
        let limits = authority
            .execution_limits()?
            .ok_or(ComputerError::Forbidden)?;
        if payload.limits().maximum_seconds > limits.maximum_seconds
            || payload.limits().maximum_output_bytes > limits.maximum_output_bytes
        {
            return Err(ComputerError::InvalidInput);
        }
        if computer.phase != ComputerPhase::Ready {
            return Err(ComputerError::InvalidState);
        }
        if computer.active_operation.is_some() {
            return Err(ComputerError::OperationBusy);
        }
        let mut required_output_labels = computer
            .owner
            .data_labels
            .iter()
            .map(|label| {
                veoveo_types::DataLabelId::parse(label.clone())
                    .map_err(|_| ComputerError::Unavailable)
            })
            .collect::<Result<std::collections::BTreeSet<_>>>()?;
        for output_policy in [
            &computer.owner.authority.output_policy,
            &actor.owner().authority.output_policy,
        ] {
            required_output_labels.extend(output_policy.data_labels.iter().cloned());
            required_output_labels.extend(output_policy.classification.iter().cloned());
        }
        let binding = CommandBinding {
            execution_id: veoveo_computers_contract::ExecutionId::new(),
            request_id,
            computer_id,
            grant_id: authority.grant_id(),
            provider_instance_id: self.provider_instance_id,
            actor_key: actor_key.clone(),
            owner_key: owner_key(&computer.owner)?,
            template_fingerprint: computer.template_fingerprint.clone(),
            resource_id: computer
                .provider_resource_id
                .clone()
                .ok_or(ComputerError::InvalidState)?,
            process_id: computer
                .process_id
                .clone()
                .ok_or(ComputerError::InvalidState)?,
            required_output_labels,
            replacement_instance_id: computer.replacement_instance_id,
        };
        let sealed = keys.seal(&binding, payload)?;
        let content = Content {
            execution_id: binding.execution_id.as_uuid(),
            computer_id: computer_id.as_uuid(),
            provider_instance_id: self.provider_instance_id.as_uuid(),
            actor_key: actor_key.clone(),
            binding: object(&binding)?,
            authority: object(actor.accepted())?,
            payload: super::payload_record(binding.execution_id),
            task: task_record_id(binding.execution_id.task_id()),
        };

        let mut params = authority.transaction_bindings()?;
        params.extend([
            ("computer", computer_record(computer_id).into_value()),
            ("request", request.clone().into_value()),
            (
                "execution",
                super::record(binding.execution_id).into_value(),
            ),
            ("slot", super::slot(computer_id).into_value()),
            ("content", content.into_value()),
            (
                "payload",
                super::payload_record(binding.execution_id).into_value(),
            ),
            (
                "payload_content",
                Payload {
                    journal: super::record(binding.execution_id),
                    sealed,
                }
                .into_value(),
            ),
            ("policy", self.automation_policy_record().into_value()),
            ("expected_updated_at", computer.updated_at.into_value()),
            (
                "expected_owner_context",
                object(&computer.owner)?.into_value(),
            ),
            crate::audit::binding(
                actor.accepted(),
                binding.computer_id,
                crate::audit::Transition::accepted(
                    veoveo_audit_contract::ComputerActivity::Command,
                    veoveo_audit_contract::ComputerAuditStage::Queued,
                )
                .task(binding.execution_id.task_id()),
            )?,
        ]);
        self.query(include_str!("../../queries/queue_command.surql"), params)
            .await?;
        let selected = self
            .command_request(&request, actor, &actor_key, computer_id)
            .await?
            .ok_or(ComputerError::Unavailable)?;
        self.match_command(
            selected,
            actor,
            computer_id,
            binding.grant_id,
            request_id,
            payload,
            keys,
        )
    }
    async fn command_request(
        &self,
        request: &RecordId,
        actor: &ComputerActor,
        actor_key: &str,
        computer: crate::api::ComputerId,
    ) -> Result<Option<CommandOperation>> {
        let mut receipt = self
            .query(
                "SELECT execution FROM ONLY $request;",
                vec![("request", request.clone().into_value())],
            )
            .await?;
        #[derive(SurrealValue)]
        struct Receipt {
            execution: RecordId,
        }
        let prior: Option<Receipt> = receipt.take(0).map_err(|_| ComputerError::Unavailable)?;
        let Some(prior) = prior else {
            return Ok(None);
        };
        let mut response = self
            .query(
                include_str!("../../queries/accepted_execution.surql"),
                vec![
                    ("journal", prior.execution.into_value()),
                    ("provider", self.provider_instance_id.as_uuid().into_value()),
                    ("computer_id", computer.as_uuid().into_value()),
                    ("computer_text", computer.to_string().into_value()),
                    ("actor_key", actor_key.to_owned().into_value()),
                    (
                        "labels",
                        actor
                            .owner()
                            .data_labels
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .into_value(),
                    ),
                    ("file", false.into_value()),
                ],
            )
            .await?;
        let row: Option<model::Record> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        Ok(Some(CommandOperation::try_from(
            row.ok_or(ComputerError::NotFound)?,
        )?))
    }
    #[allow(clippy::too_many_arguments)]
    fn match_command(
        &self,
        command: CommandOperation,
        actor: &ComputerActor,
        computer: veoveo_computers_contract::ComputerId,
        grant: veoveo_computers_contract::AutomationGrantId,
        request: crate::api::RequestId,
        payload: &CommandPayload,
        keys: &ComputerKeyRing,
    ) -> Result<CommandOperation> {
        actor.check_admission()?;
        if command.binding.actor_key != super::actor_key(actor.accepted())?
            || command.binding.computer_id != computer
            || command.binding.provider_instance_id != self.provider_instance_id
        {
            return Err(ComputerError::NotFound);
        }
        if command.binding.request_id != request
            || command.binding.grant_id != grant
            || !keys.matches(&command.binding, &command.sealed, payload)?
        {
            return Err(ComputerError::RequestConflict);
        }
        Ok(command)
    }
}
