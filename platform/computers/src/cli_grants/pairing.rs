use super::{CliPairing, CliPairingRequest, PairedCliGrant, model, secret};
use crate::{
    ComputerActor, ComputerError, ComputersStore, Result,
    identity::owner_key,
    session_grants::{authority, object, policy::StoredPolicy},
};
use chrono::{DateTime, Utc};
use std::time::Duration;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

impl ComputersStore {
    pub async fn begin_cli_pairing(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
        request: &CliPairingRequest,
    ) -> Result<CliPairing> {
        tokio::time::timeout(Duration::from_secs(5), async {
            request.validate()?;
            let control = self.control_authority(actor).await?;
            control.require_attach(computer_id)?;
            let computer = self.get(actor.owner(), computer_id).await?;
            authority::ready(&computer, self.provider_instance_id)?;
            let owner = owner_key(actor.owner())?;
            let id = Uuid::now_v7();
            let mut response = self
                .query(
                    include_str!("../../queries/begin_cli_pairing.surql"),
                    vec![
                        ("pairing", super::pairing_record(id).into_value()),
                        ("pairing_id", id.into_value()),
                        (
                            "computer",
                            crate::model::computer_record(computer_id).into_value(),
                        ),
                        ("computer_id", computer_id.into_value()),
                        (
                            "guard",
                            RecordId::new("computer_cli_pairing_guard", owner.clone()).into_value(),
                        ),
                        ("owner_key", owner.into_value()),
                        ("family", model::family(actor.accepted())?.into_value()),
                        (
                            "binding_hash",
                            model::binding_hash(actor.accepted())?.into_value(),
                        ),
                        (
                            "code_hash",
                            crate::identity::digest(&("veoveo.cli-pairing.v1", &request.code))?
                                .into_value(),
                        ),
                        ("name", request.name.clone().into_value()),
                        ("callback_port", request.callback_port.into_value()),
                        (
                            "admission_expires_at",
                            actor.admission_expires_at().into_value(),
                        ),
                    ],
                )
                .await?;
            let index = response
                .num_statements()
                .checked_sub(1)
                .ok_or(ComputerError::Unavailable)?;
            let expires: Option<DateTime<Utc>> = response
                .take(index)
                .map_err(|_| ComputerError::Unavailable)?;
            Ok(CliPairing {
                pairing_id: id,
                computer_id,
                expires_at: expires.ok_or(ComputerError::Unavailable)?,
            })
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    /// Called only by the authenticated explicit-confirmation surface. A retry
    /// cannot recover a lost bearer or consume the challenge a second time.
    pub async fn confirm_cli_pairing(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
        pairing_id: Uuid,
    ) -> Result<PairedCliGrant> {
        tokio::time::timeout(Duration::from_secs(5), async {
            actor.check_admission()?;
            let snapshot = self.read_authority(actor.accepted()).await?;
            self.check_control_session(&snapshot)
                .await?
                .ok_or(ComputerError::Forbidden)?;
            authority::require_attach(&snapshot, computer_id)?;
            let computer = self.get(actor.owner(), computer_id).await?;
            authority::ready(&computer, self.provider_instance_id)?;
            let mut read = self
                .query(
                    "SELECT * FROM ONLY $pairing; SELECT * FROM ONLY $policy;",
                    vec![
                        ("pairing", super::pairing_record(pairing_id).into_value()),
                        ("policy", self.session_policy_record().into_value()),
                    ],
                )
                .await?;
            let pairing: Option<model::Pairing> =
                read.take(0).map_err(|_| ComputerError::Unavailable)?;
            let pairing = pairing.ok_or(ComputerError::NotFound)?;
            let policy: Option<StoredPolicy> =
                read.take(1).map_err(|_| ComputerError::Unavailable)?;
            let policy = policy.ok_or(ComputerError::Unavailable)?;
            let limits = policy.checked()?;
            let owner = owner_key(actor.owner())?;
            let family = model::family(actor.accepted())?;
            let binding = model::binding_hash(actor.accepted())?;
            if pairing.pairing_id != pairing_id
                || pairing.computer_id != computer_id
                || pairing.owner_key != owner
                || pairing.family != family
                || pairing.binding_hash != binding
                || pairing.expires_at <= Utc::now()
                || pairing.consumed_at.is_some()
            {
                return Err(ComputerError::Forbidden);
            }
            let grant_id = Uuid::now_v7();
            let (credential, hash) = secret::issue(grant_id)?;
            let mut params = authority::bindings(&snapshot);
            params.extend([
                ("pairing", super::pairing_record(pairing_id).into_value()),
                (
                    "computer",
                    crate::model::computer_record(computer_id).into_value(),
                ),
                ("computer_id", computer_id.into_value()),
                ("grant", super::grant_record(grant_id).into_value()),
                ("grant_id", grant_id.into_value()),
                (
                    "guard",
                    super::record("computer_session_grant_guard", computer_id).into_value(),
                ),
                ("owner_key", owner.into_value()),
                ("provider", self.provider_instance_id.into_value()),
                ("authority", object(actor.accepted())?.into_value()),
                ("family", family.into_value()),
                ("binding_hash", binding.into_value()),
                ("credential_hash", hash.into_value()),
                ("policy", self.session_policy_record().into_value()),
                ("policy_fingerprint", policy.fingerprint.into_value()),
                (
                    "absolute",
                    surrealdb::types::Duration::from_secs(u64::from(limits.absolute_seconds))
                        .into_value(),
                ),
                (
                    "idle",
                    surrealdb::types::Duration::from_secs(u64::from(limits.idle_seconds))
                        .into_value(),
                ),
                (
                    "authority_expires_at",
                    actor
                        .admission_expires_at()
                        .min(snapshot.checked_at + chrono::TimeDelta::seconds(30))
                        .into_value(),
                ),
                (
                    "event",
                    authority::event(actor.accepted(), computer_id, grant_id, "access_issued")?
                        .into_value(),
                ),
            ]);
            actor.check_admission()?;
            snapshot.check_fresh()?;
            let mut response = self
                .query(
                    include_str!("../../queries/confirm_cli_pairing.surql"),
                    params,
                )
                .await?;
            let index = response
                .num_statements()
                .checked_sub(1)
                .ok_or(ComputerError::Unavailable)?;
            let expires: Option<DateTime<Utc>> = response
                .take(index)
                .map_err(|_| ComputerError::Unavailable)?;
            Ok(PairedCliGrant {
                grant_id,
                computer_id,
                credential,
                callback_port: pairing.callback_port,
                expires_at: expires.ok_or(ComputerError::Unavailable)?,
            })
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
}
