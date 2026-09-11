use super::{FileOperation, FileTransferAuthority};
use crate::{
    ComputerActor, ComputerError, ComputersStore, Result,
    api::ComputerPhase,
    identity::owner_key,
    secrets::{ComputerKeyRing, FileTransferBinding, FileTransferPayload},
};
use serde::Serialize;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::{OpenObject, OutboxDraft, deterministic_tenant_id};

#[derive(Serialize, SurrealValue)]
struct Content {
    transfer_id: Uuid,
    computer_id: Uuid,
    provider_instance_id: Uuid,
    owner_key: String,
    actor_key: String,
    binding: OpenObject,
    authority: OpenObject,
    sealed: OpenObject,
    task: RecordId,
}
impl ComputersStore {
    pub async fn queue_file_transfer(
        &self,
        actor: &ComputerActor,
        authority: FileTransferAuthority,
        request_id: Uuid,
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
        request_id: Uuid,
        payload: &FileTransferPayload,
        keys: &ComputerKeyRing,
    ) -> Result<FileOperation> {
        authority.require_actor(actor)?;
        if request_id.is_nil() {
            return Err(ComputerError::InvalidInput);
        }
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
        if let Some(prior) = self.file_request(&request).await? {
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
            transfer_id: Uuid::now_v7(),
            request_id,
            computer_id: computer.computer_id,
            instance_id: computer
                .replacement_instance_id
                .unwrap_or(computer.computer_id),
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
            transfer_id: binding.transfer_id,
            computer_id: binding.computer_id,
            provider_instance_id: self.provider_instance_id,
            owner_key: binding.owner_key.clone(),
            actor_key,
            binding: super::object(&binding)?,
            authority: super::object(actor.accepted())?,
            sealed: super::object(&sealed)?,
            task: veoveo_task_runtime::TaskId::from_uuid(binding.transfer_id).record_id(),
        };
        #[derive(Serialize)]
        struct Event<'a> {
            transfer_id: Uuid,
            computer_id: Uuid,
            direction: crate::api::FileTransferDirection,
            actor: &'a crate::AcceptedAuthority,
        }
        let event = OutboxDraft::now(
            Some(
                deterministic_tenant_id(actor.owner().tenant_key())
                    .map_err(|_| ComputerError::InvalidInput)?
                    .record_id(),
            ),
            "computer",
            binding.computer_id.to_string(),
            "computer.file_transfer_queued",
            1,
            super::object(&Event {
                transfer_id: binding.transfer_id,
                computer_id: binding.computer_id,
                direction: binding.direction,
                actor: actor.accepted(),
            })?,
        );
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
            ("event", event.into_value()),
            ("expected_updated_at", computer.updated_at.into_value()),
            (
                "expected_owner_context",
                super::object(&computer.owner)?.into_value(),
            ),
            ("policy", self.automation_policy_record().into_value()),
        ]);
        self.query(
            include_str!("../../queries/queue_file_transfer.surql"),
            params,
        )
        .await?;
        let selected = self
            .file_request(&request)
            .await?
            .ok_or(ComputerError::Unavailable)?;
        self.match_file(selected, actor, &authority, request_id, payload, keys)
    }
    async fn file_request(&self, request: &RecordId) -> Result<Option<FileOperation>> {
        let mut response = self
            .query(
                "SELECT * FROM ONLY (SELECT VALUE transfer FROM ONLY $request);",
                vec![("request", request.clone().into_value())],
            )
            .await?;
        let row: Option<super::model::Record> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        row.map(FileOperation::try_from).transpose()
    }
    fn match_file(
        &self,
        operation: FileOperation,
        actor: &ComputerActor,
        authority: &FileTransferAuthority,
        request: Uuid,
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
