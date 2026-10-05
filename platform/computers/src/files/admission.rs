use super::{FileOperation, FileTransferAuthority};
use crate::{
    ComputerActor, ComputerError, ComputersStore, Result,
    api::ComputerPhase,
    identity::owner_key,
    secrets::{ComputerKeyRing, FileTransferBinding, FileTransferPayload, SealedFileTransfer},
};
use serde::Serialize;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::task_record_id;

#[derive(Serialize, SurrealValue)]
struct Content {
    transfer_id: Uuid,
    computer_id: Uuid,
    provider_instance_id: Uuid,
    owner_key: String,
    actor_key: String,
    binding: crate::secrets::FileTransferBinding,
    authority: crate::AcceptedAuthority,
    payload: RecordId,
    task: RecordId,
    task_tenant: RecordId,
}
#[derive(SurrealValue)]
struct Payload {
    journal: RecordId,
    #[surreal(wrap)]
    sealed: SealedFileTransfer,
}
impl ComputersStore {
    pub async fn queue_file_transfer(
        &self,
        actor: &ComputerActor,
        authority: FileTransferAuthority,
        request_id: crate::api::RequestId,
        payload: &FileTransferPayload,
        keys: &ComputerKeyRing,
    ) -> Result<FileOperation> {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.queue_file(actor, authority, request_id, payload, keys),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    async fn queue_file(
        &self,
        actor: &ComputerActor,
        authority: FileTransferAuthority,
        request_id: crate::api::RequestId,
        payload: &FileTransferPayload,
        keys: &ComputerKeyRing,
    ) -> Result<FileOperation> {
        authority.require_actor(actor)?;
        let computer = authority.computer()?;
        let actor_key = super::actor_key(actor.accepted())?;
        let request = RecordId::new(
            "computer_file_transfer_request",
            crate::identity::digest(&(
                "veoveo.computer.file-request.v1",
                &actor_key,
                computer.computer_id,
                request_id,
            ))?,
        );
        if let Some(prior) = self
            .file_request(&request, actor, &actor_key, computer.computer_id)
            .await?
        {
            return self.match_file(prior, actor, &authority, request_id, payload, keys);
        }
        let limits = authority.limits()?;
        if payload.limits().maximum_seconds > limits.maximum_seconds
            || payload.limits().maximum_bytes > limits.maximum_bytes
        {
            return Err(ComputerError::InvalidInput);
        }
        if computer.phase != ComputerPhase::Ready {
            return Err(ComputerError::InvalidState);
        }
        if computer.active_operation.is_some() {
            return Err(ComputerError::OperationBusy);
        }
        let binding = FileTransferBinding {
            transfer_id: veoveo_computers_contract::FileTransferId::new(),
            request_id,
            computer_id: computer.computer_id,
            instance_id: computer
                .replacement_instance_id
                .unwrap_or(computer.computer_id.as_uuid()),
            provider_instance_id: self.provider_instance_id,
            grant_id: authority.grant_id(),
            direction: payload.transfer().direction(),
            owner_key: owner_key(&computer.owner)?,
            actor_key: actor_key.clone(),
            template_fingerprint: computer.template_fingerprint.clone(),
            resource_id: computer
                .provider_resource_id
                .clone()
                .ok_or(ComputerError::InvalidState)?,
            process_id: computer
                .process_id
                .clone()
                .ok_or(ComputerError::InvalidState)?,
            required_labels: authority.required_labels(actor)?,
        };
        let sealed = keys.seal_file_transfer(&binding, payload)?;
        let content = Content {
            transfer_id: binding.transfer_id.as_uuid(),
            computer_id: binding.computer_id.as_uuid(),
            provider_instance_id: self.provider_instance_id.as_uuid(),
            owner_key: binding.owner_key.clone(),
            actor_key: actor_key.clone(),
            binding: binding.clone(),
            authority: actor.accepted().clone(),
            payload: super::payload_record(binding.transfer_id),
            task: task_record_id(binding.transfer_id.task_id()),
            task_tenant: crate::identity::task_tenant(actor.owner())?,
        };

        let mut params = authority.transaction_bindings()?;
        params.extend([
            ("request", request.clone().into_value()),
            ("transfer", super::record(binding.transfer_id).into_value()),
            (
                "computer",
                crate::model::computer_record(binding.computer_id).into_value(),
            ),
            (
                "slot",
                crate::commands::slot(binding.computer_id).into_value(),
            ),
            ("content", content.into_value()),
            (
                "payload",
                super::payload_record(binding.transfer_id).into_value(),
            ),
            (
                "payload_content",
                Payload {
                    journal: super::record(binding.transfer_id),
                    sealed,
                }
                .into_value(),
            ),
            crate::audit::binding(
                self.platform.audit_targets(),
                actor.accepted(),
                binding.computer_id,
                crate::audit::Transition::accepted(
                    veoveo_audit_contract::ComputerActivity::FileTransfer,
                    veoveo_audit_contract::ComputerAuditStage::Queued,
                )
                .task(binding.transfer_id.task_id()),
            )?,
            ("expected_updated_at", computer.updated_at.into_value()),
            (
                "expected_owner_context",
                crate::identity::stored_owner(&computer.owner)?.into_value(),
            ),
            ("policy", self.automation_policy_record().into_value()),
        ]);
        self.query(
            include_str!("../../queries/queue_file_transfer.surql"),
            params,
        )
        .await?;
        let selected = self
            .file_request(&request, actor, &actor_key, computer.computer_id)
            .await?
            .ok_or(ComputerError::Unavailable)?;
        self.match_file(selected, actor, &authority, request_id, payload, keys)
    }
    async fn file_request(
        &self,
        request: &RecordId,
        actor: &ComputerActor,
        actor_key: &str,
        computer: crate::api::ComputerId,
    ) -> Result<Option<FileOperation>> {
        let mut receipt = self
            .query(
                include_str!("../../queries/files/admission/file_request.surql"),
                vec![("request", request.clone().into_value())],
            )
            .await?;
        #[derive(SurrealValue)]
        struct Receipt {
            transfer: RecordId,
        }
        let prior: Option<Receipt> = receipt.take(0).map_err(|_| ComputerError::Unavailable)?;
        let Some(prior) = prior else {
            return Ok(None);
        };
        let mut response = self
            .query(
                include_str!("../../queries/accepted_execution.surql"),
                vec![
                    ("journal", prior.transfer.into_value()),
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
                    ("file", true.into_value()),
                ],
            )
            .await?;
        let row: Option<super::model::Record> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        Ok(Some(FileOperation::try_from(
            row.ok_or(ComputerError::NotFound)?,
        )?))
    }
    fn match_file(
        &self,
        operation: FileOperation,
        actor: &ComputerActor,
        authority: &FileTransferAuthority,
        request: crate::api::RequestId,
        payload: &FileTransferPayload,
        keys: &ComputerKeyRing,
    ) -> Result<FileOperation> {
        authority.require_actor(actor)?;
        if operation.binding.actor_key != super::actor_key(actor.accepted())?
            || operation.computer_id() != authority.computer()?.computer_id
            || operation.binding.provider_instance_id != self.provider_instance_id
        {
            return Err(ComputerError::NotFound);
        }
        if operation.binding.request_id != request
            || operation.binding.grant_id != authority.grant_id()
            || operation.binding.direction != payload.transfer().direction()
            || !keys.matches_file_transfer(&operation.binding, &operation.sealed, payload)?
        {
            return Err(ComputerError::RequestConflict);
        }
        Ok(operation)
    }
}
