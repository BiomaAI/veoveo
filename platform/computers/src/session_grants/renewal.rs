use super::{SessionGrantHandle, SessionGrantLease, authority, model, policy::StoredPolicy};
use crate::{
    ComputerActor, ComputerError, ComputersStore, Result, identity::verify_retained_owner,
};
use chrono::{DateTime, TimeDelta, Utc};
use std::time::{Duration, Instant};
use surrealdb::types::SurrealValue;
use uuid::Uuid;

impl ComputersStore {
    /// `activity` means accepted terminal input since the previous check. Timer
    /// ticks and output do not extend idle access. Failed renewal ends attachment.
    pub async fn renew_browser_grant(
        &self,
        handle: &SessionGrantHandle,
        activity: bool,
    ) -> Result<SessionGrantLease> {
        tokio::time::timeout(Duration::from_secs(5), self.renew_browser(handle, activity))
            .await
            .map_err(|_| ComputerError::Unavailable)?
    }
    async fn renew_browser(
        &self,
        handle: &SessionGrantHandle,
        activity: bool,
    ) -> Result<SessionGrantLease> {
        let started = Instant::now();
        let mut read = self
            .query(
                "SELECT * FROM ONLY $grant WHERE connection_id = $connection AND provider_instance_id = $provider
                 AND revoked_at = NONE AND expires_at > time::now() AND idle_expires_at > time::now()
                 AND family.revoked_at = NONE AND family.expires_at > time::now();
                 SELECT * FROM ONLY $policy; RETURN time::now();",
                vec![
                    ("grant", super::record(handle.grant_id).into_value()),
                    ("connection", handle.connection_id.as_uuid().into_value()),
                    ("provider", self.provider_instance_id.as_uuid().into_value()),
                    ("policy", self.session_policy_record().into_value()),
                ],
            )
            .await?;
        let row: Option<model::Record> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let row = row.ok_or(ComputerError::Forbidden)?;
        let policy: Option<StoredPolicy> = read.take(1).map_err(|_| ComputerError::Unavailable)?;
        let policy = policy.ok_or(ComputerError::Unavailable)?;
        let limits = policy.checked()?;
        let now: Option<DateTime<Utc>> = read.take(2).map_err(|_| ComputerError::Unavailable)?;
        let now = now.ok_or(ComputerError::Unavailable)?;
        let absolute = row
            .expires_at
            .min(row.issued_at + TimeDelta::seconds(i64::from(limits.absolute_seconds)));
        let idle = row
            .idle_expires_at
            .min(row.last_activity_at + TimeDelta::seconds(i64::from(limits.idle_seconds)));
        if row.connection_id != Some(handle.connection_id.as_uuid())
            || row.provider_instance_id != self.provider_instance_id.as_uuid()
            || limits.max_grants == 0
            || row.revoked_at.is_some()
            || absolute <= now
            || idle <= now
            || row.issued_at > now
            || row.last_activity_at < row.issued_at
            || row.last_activity_at > now
        {
            return Err(ComputerError::Forbidden);
        }
        let accepted = row.accepted()?;
        let snapshot = self.read_authority(&accepted).await?;
        let family_end = self
            .check_control_session(&snapshot)
            .await?
            .ok_or(ComputerError::Forbidden)?;
        authority::require_attach(&snapshot, row.computer_id()?)?;
        let computer = self.get(&accepted.task_owner(), row.computer_id()?).await?;
        verify_retained_owner(&computer.owner, &row.owner_key, &accepted.task_owner())?;
        authority::ready(&computer, self.provider_instance_id)?;
        if computer.provider_resource_id.as_deref() != Some(row.provider_resource_id.as_str())
            || computer.process_id.as_deref() != Some(row.process_id.as_str())
        {
            return Err(ComputerError::Forbidden);
        }
        let effective_end = if activity {
            // The transaction rechecks the old idle limit before extending it.
            self.query(
                include_str!("../../queries/touch_session_grant.surql"),
                vec![
                    ("grant", super::record(handle.grant_id).into_value()),
                    ("connection", handle.connection_id.as_uuid().into_value()),
                    ("policy", self.session_policy_record().into_value()),
                    ("policy_fingerprint", policy.fingerprint.into_value()),
                    (
                        "idle",
                        surrealdb::types::Duration::from_secs(u64::from(limits.idle_seconds))
                            .into_value(),
                    ),
                    ("previous_idle_end", idle.into_value()),
                    ("absolute_end", absolute.min(family_end).into_value()),
                ],
            )
            .await?;
            // Conservatively charge all read/write latency against this earlier clock.
            absolute
                .min(family_end)
                .min(now + TimeDelta::seconds(i64::from(limits.idle_seconds)))
        } else {
            absolute.min(family_end).min(idle)
        };
        let remaining = (effective_end - now)
            .to_std()
            .map_err(|_| ComputerError::Forbidden)?
            .min(crate::current_authority::AUTHORITY_LIFETIME);
        let valid_until = started + remaining;
        if valid_until <= Instant::now() {
            return Err(ComputerError::Forbidden);
        }
        Ok(SessionGrantLease {
            session_family_id: accepted
                .request_context
                .access_token
                .session_family
                .as_ref()
                .ok_or(ComputerError::Forbidden)?
                .clone(),
            computer,
            checked_at: started,
            valid_until,
        })
    }
    /// An authenticated owner can revoke their grant from another browser family.
    pub async fn revoke_browser_grant(
        &self,
        actor: &ComputerActor,
        computer_id: veoveo_computers_contract::ComputerId,
        grant_id: crate::api::AccessGrantId,
    ) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let control = self.control_authority(actor).await?;
            control.require_read(Some(computer_id))?;
            let computer = self.get(actor.owner(), computer_id).await?;
            let mut params = crate::store::owner_query_bindings(actor.owner())?;
            params.extend([
                ("grant", super::record(grant_id).into_value()),
                ("computer_id", computer_id.as_uuid().into_value()),
                (
                    "owner_key",
                    crate::identity::owner_key(&computer.owner)?.into_value(),
                ),
                ("provider", self.provider_instance_id.as_uuid().into_value()),
            ]);
            let mut read = self
                .query(
                    include_str!("../../queries/owned_access_grant.surql"),
                    params,
                )
                .await?;
            let row: Option<model::Record> =
                read.take(0).map_err(|_| ComputerError::Unavailable)?;
            let row = row.ok_or(ComputerError::NotFound)?;
            verify_retained_owner(
                &computer.owner,
                &row.owner_key,
                &row.accepted()?.task_owner(),
            )?;
            self.query(
                include_str!("../../queries/revoke_session_grant.surql"),
                vec![
                    ("grant", super::record(grant_id).into_value()),
                    ("owner_key", row.owner_key.clone().into_value()),
                    ("connection", Option::<Uuid>::None.into_value()),
                    (
                        "admission_expires_at",
                        actor.admission_expires_at().into_value(),
                    ),
                    crate::audit::binding(
                        actor.accepted(),
                        row.computer_id()?,
                        crate::audit::Transition::accepted(
                            veoveo_audit_contract::ComputerActivity::Revoke,
                            veoveo_audit_contract::ComputerAuditStage::GrantRevoked,
                        ),
                    )?,
                ],
            )
            .await?;
            Ok(())
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    /// Transport cleanup uses only the exact successfully redeemed connection.
    pub async fn close_browser_grant(&self, handle: &SessionGrantHandle) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let mut read = self.query("SELECT * FROM ONLY $grant WHERE connection_id = $connection AND provider_instance_id = $provider;", vec![
                ("grant", super::record(handle.grant_id).into_value()),
                ("connection", handle.connection_id.as_uuid().into_value()),
                ("provider", self.provider_instance_id.as_uuid().into_value()),
            ]).await?;
            let row: Option<model::Record> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
            let row = row.ok_or(ComputerError::Forbidden)?;
            if row.connection_id != Some(handle.connection_id.as_uuid()) {
                return Err(ComputerError::Forbidden);
            }
            let accepted = row.accepted()?;
            self.query(
                include_str!("../../queries/revoke_session_grant.surql"),
                vec![
                    ("grant", super::record(handle.grant_id).into_value()),
                    ("owner_key", row.owner_key.clone().into_value()),
                    ("connection", Some(handle.connection_id.as_uuid()).into_value()),
                    (
                        "admission_expires_at",
                        Option::<DateTime<Utc>>::None.into_value(),
                    ),
                    crate::audit::binding(&accepted, row.computer_id()?, crate::audit::Transition::accepted(veoveo_audit_contract::ComputerActivity::Close, veoveo_audit_contract::ComputerAuditStage::Closed))?,

                ],
            )
            .await?;
            Ok(())
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
}
