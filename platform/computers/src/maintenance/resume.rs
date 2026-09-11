//! Explicit finite recovery windows; a retry cannot renew the same window twice.
use super::{MaintenanceOperation, MaintenanceStage, authority::resume_target, object, record};
use crate::{
    ComputerActor, ComputerError, ComputersStore, Result,
    api::ResumeUpdateInput,
    identity::{can_mutate, digest, owner_key},
    model::computer_record,
};
use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::{
    OpenObject, OutboxDraft, deterministic_enterprise_id, deterministic_tenant_id,
    gateway_refresh_family_record_id,
};
use veoveo_task_runtime::{TaskError, TaskRuntime};

#[derive(Serialize, SurrealValue)]
struct Content {
    operation_id: Uuid,
    request_id: Uuid,
    fingerprint: String,
    input: OpenObject,
    authority: OpenObject,
    decision: OpenObject,
    previous_progress: OpenObject,
}
#[derive(Deserialize, SurrealValue)]
struct Receipt {
    operation_id: Uuid,
    request_id: Uuid,
    fingerprint: String,
    input: OpenObject,
}
fn request_record(actor: &ComputerActor, input: &ResumeUpdateInput) -> Result<RecordId> {
    if input.computer_id.is_nil()
        || input.task_id.get_version_num() != 7
        || input.request_id.is_nil()
    {
        return Err(ComputerError::InvalidInput);
    }
    Ok(RecordId::new(
        "computer_maintenance_resume",
        digest(&(
            "veoveo.computer.maintenance-resume.request.v1",
            owner_key(actor.owner())?,
            input.computer_id,
            input.task_id,
            input.request_id,
        ))?,
    ))
}
fn fingerprint(input: &ResumeUpdateInput) -> Result<String> {
    digest(&("veoveo.computer.maintenance-resume.input.v1", input))
}

impl ComputersStore {
    /// Requires current named recovery authority and private Computer ownership.
    /// It admits another finite window, never provider dispatch or fence release.
    pub async fn resume_maintenance(
        &self,
        actor: &ComputerActor,
        input: &ResumeUpdateInput,
    ) -> Result<MaintenanceOperation> {
        tokio::time::timeout(
            Duration::from_secs(5),
            self.resume_maintenance_inner(actor, input),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    async fn resumed_request(
        &self,
        actor: &ComputerActor,
        input: &ResumeUpdateInput,
    ) -> Result<Option<MaintenanceOperation>> {
        let mut reply = self
            .query(
                "SELECT * FROM ONLY $request;",
                vec![("request", request_record(actor, input)?.into_value())],
            )
            .await?;
        let prior: Option<Receipt> = reply.take(0).map_err(|_| ComputerError::Unavailable)?;
        let Some(prior) = prior else { return Ok(None) };
        let saved: ResumeUpdateInput = serde_json::from_value(
            serde_json::to_value(prior.input).map_err(|_| ComputerError::Unavailable)?,
        )
        .map_err(|_| ComputerError::Unavailable)?;
        if prior.operation_id != input.task_id
            || prior.request_id != input.request_id
            || prior.fingerprint != fingerprint(&saved)?
        {
            return Err(ComputerError::Unavailable);
        }
        if saved != *input {
            return Err(ComputerError::RequestConflict);
        }
        let operation = self.maintenance(actor.owner(), input.task_id).await?;
        if operation.computer_id != input.computer_id {
            return Err(ComputerError::NotFound);
        }
        Ok(Some(operation))
    }

    async fn resume_maintenance_inner(
        &self,
        actor: &ComputerActor,
        input: &ResumeUpdateInput,
    ) -> Result<MaintenanceOperation> {
        can_mutate(actor.owner())?;
        let request = request_record(actor, input)?;
        let authority = self
            .maintenance_request_authority(actor, &resume_target())
            .await?;
        if let Some(prior) = self.resumed_request(actor, input).await? {
            return Ok(prior);
        }
        let before = self.maintenance(actor.owner(), input.task_id).await?;
        if before.computer_id != input.computer_id {
            return Err(ComputerError::NotFound);
        }
        if before.stage != MaintenanceStage::RecoveryRequired
            || before.updated_at != input.expected_updated_at
        {
            // A concurrent exact request may have committed since the first lookup.
            return self
                .resumed_request(actor, input)
                .await?
                .ok_or(ComputerError::StateConflict);
        }
        let tasks = TaskRuntime::new(
            self.platform.clone(),
            "computers",
            format!("maintenance-resume/{}", Uuid::now_v7()),
        );
        let claim = match tasks
            .claim_observation(&before.task_id().to_string(), Duration::from_secs(30))
            .await
        {
            Ok(claim) => claim,
            Err(_) => {
                return self
                    .resumed_request(actor, input)
                    .await?
                    .ok_or(ComputerError::StateConflict);
            }
        };
        let result = async {
            let clock = self.worker_maintenance(&claim).await?;
            let before = clock.operation;
            if before.stage != MaintenanceStage::RecoveryRequired
                || before.updated_at != input.expected_updated_at
            {
                return Err(ComputerError::StateConflict);
            }
            let computer = self.get(actor.owner(), input.computer_id).await?;
            let mut after = before.clone();
            after.progress.recovery = None;
            after.stage = after.resumed_stage();
            if let Some(step) = after.progress.steps.last_mut() {
                step.observation_started_at = clock.database_time;
                step.observation_deadline = clock.database_time + TimeDelta::seconds(180);
                step.observation_reads = 0;
                step.last_observation_id = None;
                step.next_observation_at = None;
            }
            after.validate_progress()?;
            let decision = authority.decision(
                veoveo_mcp_contract::GatewayAction::ToolsCall,
                &resume_target(),
                &veoveo_mcp_contract::TraceId::new(input.request_id.to_string())
                    .expect("UUID trace"),
            );
            let content = Content {
                operation_id: input.task_id,
                request_id: input.request_id,
                fingerprint: fingerprint(input)?,
                input: object(input)?,
                authority: object(actor.accepted())?,
                decision: object(&crate::ExecutionDecision {
                    control_revision: authority.control_revision.clone(),
                    control_sha256: authority.control_sha256.clone(),
                    checked_at: authority.checked_at,
                    valid_until: authority.checked_at + TimeDelta::seconds(5),
                    decision,
                })?,
                previous_progress: object(&before.progress)?,
            };
            #[derive(Serialize)]
            struct Event<'a> {
                computer_id: Uuid,
                maintenance_id: Uuid,
                request_id: Uuid,
                authority: &'a crate::AcceptedAuthority,
                acknowledged_cancellation_at: Option<chrono::DateTime<Utc>>,
            }
            let event = OutboxDraft::now(
                Some(
                    deterministic_tenant_id(actor.owner().tenant_key())
                        .map_err(|_| ComputerError::InvalidInput)?
                        .record_id(),
                ),
                "computer",
                input.computer_id.to_string(),
                "computer.maintenance_resumed",
                1,
                object(&Event {
                    computer_id: input.computer_id,
                    maintenance_id: input.task_id,
                    request_id: input.request_id,
                    authority: actor.accepted(),
                    acknowledged_cancellation_at: input.acknowledged_cancellation_at,
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
            actor.check_admission()?;
            authority.check_fresh()?;
            tasks
                .resume_provider_journal(
                    &claim,
                    input.acknowledged_cancellation_at,
                    include_str!("../../queries/resume_maintenance.surql"),
                    vec![
                        ("request", request.into_value()),
                        ("content", content.into_value()),
                        ("maintenance", record(input.task_id).into_value()),
                        ("operation_id", input.task_id.into_value()),
                        ("computer", computer_record(input.computer_id).into_value()),
                        ("provider", self.provider_instance_id.into_value()),
                        ("expected_updated_at", before.updated_at.into_value()),
                        ("expected_progress", object(&before.progress)?.into_value()),
                        ("progress", object(&after.progress)?.into_value()),
                        (
                            "stage",
                            serde_json::to_value(after.stage)
                                .map_err(|_| ComputerError::Unavailable)?
                                .as_str()
                                .ok_or(ComputerError::Unavailable)?
                                .to_owned()
                                .into_value(),
                        ),
                        ("computer_updated_at", computer.updated_at.into_value()),
                        ("source_owner", object(&computer.owner)?.into_value()),
                        ("event", event.into_value()),
                        ("family", family.into_value()),
                        (
                            "authority_expires_at",
                            actor
                                .admission_expires_at()
                                .min(authority.checked_at + TimeDelta::seconds(5))
                                .into_value(),
                        ),
                        ("authority_revision", authority.revision_record.into_value()),
                        (
                            "authority_revision_id",
                            authority.control_revision.into_value(),
                        ),
                        ("authority_sha256", authority.control_sha256.into_value()),
                        (
                            "authority_enterprise",
                            deterministic_enterprise_id().record_id().into_value(),
                        ),
                        ("authority_tenant", authority.tenant.into_value()),
                        ("authority_source", authority.source.into_value()),
                        ("authority_actor", authority.actor.into_value()),
                    ],
                )
                .await
                .map_err(|error| match error {
                    TaskError::Conflict(_) | TaskError::LeaseHeld(_) => {
                        ComputerError::StateConflict
                    }
                    _ => ComputerError::Unavailable,
                })?;
            self.maintenance(actor.owner(), input.task_id).await
        }
        .await;
        // No provider future is active in the admission handler. An unknown commit
        // reply still permits relinquishing this exact lease; the fence persists.
        let _ = tasks.release_observation(&claim).await;
        match result {
            Ok(operation) => Ok(operation),
            Err(error) => match self.resumed_request(actor, input).await? {
                Some(operation) => Ok(operation),
                None => Err(error),
            },
        }
    }
}
