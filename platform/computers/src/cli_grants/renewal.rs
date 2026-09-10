use super::{CliConnectionHandle, CliGrantLease, model};
use crate::{
    ComputerError, ComputersStore, Result,
    session_grants::{authority, policy::StoredPolicy},
};
use chrono::{DateTime, TimeDelta, Utc};
use std::time::{Duration, Instant};
use surrealdb::types::SurrealValue;

impl ComputersStore {
    /// Only accepted client activity extends idle access. Output, relay keepalives and
    /// renewal ticks do not. A failed or expired connection never revives.
    pub async fn renew_cli_grant(
        &self,
        handle: &CliConnectionHandle,
        activity: bool,
    ) -> Result<CliGrantLease> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let started = Instant::now();
            let mut read = self.query("SELECT * FROM ONLY $grant; SELECT * FROM ONLY $connection; SELECT * FROM ONLY $policy; RETURN time::now();", vec![
                ("grant", super::grant_record(handle.grant_id).into_value()),
                ("connection", super::connection_record(handle.connection_id).into_value()),
                ("policy", self.session_policy_record().into_value()),
            ]).await?;
            let grant: Option<model::Grant> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
            let grant = grant.ok_or(ComputerError::Forbidden)?;
            let connection: Option<model::Connection> = read.take(1).map_err(|_| ComputerError::Unavailable)?;
            let connection = connection.ok_or(ComputerError::Forbidden)?;
            let policy: Option<StoredPolicy> = read.take(2).map_err(|_| ComputerError::Unavailable)?;
            let policy = policy.ok_or(ComputerError::Unavailable)?;
            let limits = policy.checked()?;
            let now: Option<DateTime<Utc>> = read.take(3).map_err(|_| ComputerError::Unavailable)?;
            let now = now.ok_or(ComputerError::Unavailable)?;
            let absolute = grant.expires_at.min(grant.issued_at + TimeDelta::seconds(i64::from(limits.absolute_seconds)));
            let idle = grant.idle_expires_at.min(grant.last_activity_at + TimeDelta::seconds(i64::from(limits.idle_seconds)));
            if grant.provider_instance_id != self.provider_instance_id || limits.max_grants == 0 || grant.revoked_at.is_some()
                || absolute <= now || idle <= now || grant.issued_at > now || grant.last_activity_at < grant.issued_at
                || grant.last_activity_at > now || connection.connection_id != handle.connection_id
                || connection.grant_id != handle.grant_id || connection.closed_at.is_some() || connection.expires_at <= now
            { return Err(ComputerError::Forbidden); }
            let accepted = grant.accepted()?;
            let snapshot = self.read_authority(&accepted).await?;
            let family_end = self.check_control_session(&snapshot).await?.ok_or(ComputerError::Forbidden)?;
            authority::require_attach(&snapshot, grant.computer_id)?;
            let computer = self.get(&accepted.task_owner(), grant.computer_id).await?;
            authority::ready(&computer, self.provider_instance_id)?;
            if computer.provider_resource_id.as_deref() != Some(connection.provider_resource_id.as_str())
                || computer.process_id.as_deref() != Some(connection.process_id.as_str())
            { return Err(ComputerError::Forbidden); }
            let effective_end = absolute.min(family_end).min(if activity {
                now + TimeDelta::seconds(i64::from(limits.idle_seconds))
            } else { idle });
            let remaining = (effective_end - now).to_std().map_err(|_| ComputerError::Forbidden)?
                .min(crate::current_authority::AUTHORITY_LIFETIME);
            let valid_until = started + remaining;
            let lease_end = now + TimeDelta::from_std(remaining).map_err(|_| ComputerError::Unavailable)?;
            self.query(include_str!("../../queries/renew_cli_connection.surql"), vec![
                ("grant", super::grant_record(handle.grant_id).into_value()),
                ("grant_id", handle.grant_id.into_value()),
                ("connection", super::connection_record(handle.connection_id).into_value()),
                ("connection_id", handle.connection_id.into_value()),
                ("policy", self.session_policy_record().into_value()),
                ("policy_fingerprint", policy.fingerprint.into_value()),
                ("activity", activity.into_value()),
                ("idle", surrealdb::types::Duration::from_secs(u64::from(limits.idle_seconds)).into_value()),
                ("previous_idle_end", idle.into_value()), ("absolute_end", absolute.min(family_end).into_value()),
                ("lease_end", lease_end.into_value()),
            ]).await?;
            if valid_until <= Instant::now() { return Err(ComputerError::Forbidden); }
            Ok(CliGrantLease { session_family_id:accepted.request_context.access_token.session_family.as_ref()
                .ok_or(ComputerError::Forbidden)?.as_str().parse().map_err(|_| ComputerError::Forbidden)?,
                computer, checked_at:started, valid_until })
        }).await.map_err(|_| ComputerError::Unavailable)?
    }
}
