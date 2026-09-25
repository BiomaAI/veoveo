//! A provider read may refresh an idle retained run after a host restart.
use crate::{
    Computer, ComputerError, ComputersStore, ReachedPhase, ReachedState, Result, api::ComputerPhase,
};
use chrono::Utc;
use std::time::{Duration, Instant};
use surrealdb::types::SurrealValue;
use veoveo_platform_store::{OutboxDraft, deterministic_tenant_id};

impl ComputersStore {
    /// Internal provider observation, never a client-supplied process identity.
    /// The service must authorize the caller before reading this exact binding.
    /// This cannot settle an operation, replace a resource or release an execution fence.
    pub async fn record_observed_restart(
        &self,
        before: &Computer,
        observed: ReachedState,
        checked_at: Instant,
    ) -> Result<()> {
        let remaining = checked_at
            .checked_add(Duration::from_secs(10))
            .and_then(|end| end.checked_duration_since(Instant::now()))
            .filter(|_| checked_at <= Instant::now())
            .ok_or(ComputerError::StateConflict)?;
        if before.provider_instance_id != self.provider_instance_id
            || observed.provider_instance_id != before.provider_instance_id
            || observed.computer_id != before.computer_id
            || observed.replacement_instance_id != before.replacement_instance_id
            || observed.template_fingerprint != before.template_fingerprint
            || before.phase != ComputerPhase::Ready
            || observed.phase != ReachedPhase::Ready
            || before.provider_resource_id.as_deref() != Some(&observed.resource_id)
            || before.process_id.as_ref().is_none_or(String::is_empty)
            || observed.process_id.is_empty()
            || observed.process_id.len() > 256
            || !observed.process_id.bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(ComputerError::StateConflict);
        }
        if before.process_id.as_deref() == Some(&observed.process_id) {
            return Ok(());
        }
        if before.active_operation.is_some() {
            return Err(ComputerError::OperationBusy);
        }
        #[derive(serde::Serialize)]
        struct Payload<'a> {
            computer_id: uuid::Uuid,
            previous_process_id: &'a str,
            process_id: &'a str,
        }
        let event = OutboxDraft::now(
            Some(
                deterministic_tenant_id(before.owner.tenant_key())
                    .map_err(|_| ComputerError::InvalidInput)?
                    .record_id(),
            ),
            "computer",
            before.computer_id.to_string(),
            "computer.run_observed",
            1,
            crate::session_grants::object(&Payload {
                computer_id: before.computer_id,
                previous_process_id: before.process_id.as_deref().unwrap(),
                process_id: &observed.process_id,
            })?,
        );
        self.query(
            include_str!("../queries/observe_restart.surql"),
            vec![
                (
                    "computer",
                    crate::model::computer_record(before.computer_id).into_value(),
                ),
                ("computer_id", before.computer_id.into_value()),
                (
                    "slot",
                    crate::commands::slot(before.computer_id).into_value(),
                ),
                (
                    "owner_key",
                    crate::identity::owner_key(&before.owner)?.into_value(),
                ),
                (
                    "owner_context",
                    crate::session_grants::object(&before.owner)?.into_value(),
                ),
                ("provider", before.provider_instance_id.into_value()),
                ("instance", before.replacement_instance_id.into_value()),
                ("template", before.template_fingerprint.clone().into_value()),
                ("resource", observed.resource_id.into_value()),
                ("previous_process", before.process_id.clone().into_value()),
                ("process", observed.process_id.into_value()),
                ("updated_at", before.updated_at.into_value()),
                (
                    "expires_at",
                    (Utc::now()
                        + chrono::TimeDelta::from_std(remaining)
                            .map_err(|_| ComputerError::Unavailable)?)
                    .into_value(),
                ),
                ("event", event.into_value()),
            ],
        )
        .await?;
        Ok(())
    }
}
