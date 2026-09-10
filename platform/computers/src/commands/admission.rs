use super::{CommandOperation, model};
use crate::{
    ComputerActor, ComputerError, ComputersStore, Result,
    api::{AutomationPermission, ComputerPhase},
    automation_grants::AutomationAuthority,
    command_secrets::{CommandBinding, CommandKeyRing, CommandPayload},
    identity::owner_key,
    model::computer_record,
};
use serde::Serialize;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::{OpenObject, OutboxDraft, deterministic_tenant_id};

#[derive(Serialize, SurrealValue)]
struct Content {
    execution_id: Uuid,
    computer_id: Uuid,
    provider_instance_id: Uuid,
    actor_key: String,
    binding: OpenObject,
    authority: OpenObject,
    sealed: OpenObject,
    task: RecordId,
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
        request_id: Uuid,
        payload: &CommandPayload,
        keys: &CommandKeyRing,
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
        request_id: Uuid,
        payload: &CommandPayload,
        keys: &CommandKeyRing,
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
        if let Some(prior) = self.command_request(&request).await? {
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
                veoveo_mcp_contract::DataLabelId::new(label.clone())
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
            execution_id: Uuid::now_v7(),
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
        };
        let sealed = keys.seal(&binding, payload)?;
        let content = Content {
            execution_id: binding.execution_id,
            computer_id,
            provider_instance_id: self.provider_instance_id,
            actor_key,
            binding: object(&binding)?,
            authority: object(actor.accepted())?,
            sealed: object(&sealed)?,
            task: veoveo_task_runtime::TaskId::from_uuid(binding.execution_id).record_id(),
        };
        #[derive(Serialize)]
        struct Event<'a> {
            execution_id: Uuid,
            computer_id: Uuid,
            grant_id: Uuid,
            actor: &'a crate::AcceptedAuthority,
        }
        let event = OutboxDraft::now(
            Some(
                deterministic_tenant_id(actor.owner().tenant_key())
                    .map_err(|_| ComputerError::InvalidInput)?
                    .record_id(),
            ),
            "computer",
            computer_id.to_string(),
            "computer.execution_queued",
            1,
            object(&Event {
                execution_id: binding.execution_id,
                computer_id,
                grant_id: binding.grant_id,
                actor: actor.accepted(),
            })?,
        );
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
            ("policy", self.automation_policy_record().into_value()),
            ("expected_updated_at", computer.updated_at.into_value()),
            (
                "expected_owner_context",
                object(&computer.owner)?.into_value(),
            ),
            ("event", event.into_value()),
        ]);
        self.query(include_str!("../../queries/queue_command.surql"), params)
            .await?;
        let selected = self
            .command_request(&request)
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
    async fn command_request(&self, request: &RecordId) -> Result<Option<CommandOperation>> {
        let mut response = self
            .query(
                "SELECT * FROM ONLY (SELECT VALUE execution FROM ONLY $request);",
                vec![("request", request.clone().into_value())],
            )
            .await?;
        let row: Option<model::Record> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        row.map(CommandOperation::try_from).transpose()
    }
    #[allow(clippy::too_many_arguments)]
    fn match_command(
        &self,
        command: CommandOperation,
        actor: &ComputerActor,
        computer: Uuid,
        grant: Uuid,
        request: Uuid,
        payload: &CommandPayload,
        keys: &CommandKeyRing,
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
