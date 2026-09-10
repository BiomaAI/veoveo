use super::{
    SessionGrantHandle, SessionGrantTicket, authority, model, policy::StoredPolicy, secret,
};
use crate::{
    ComputerActor, ComputerError, ComputersStore, Result, api::TerminalToken, identity::owner_key,
    model::computer_record,
};
use chrono::{DateTime, Utc};
use std::time::Duration;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::gateway_refresh_family_record_id;

impl ComputersStore {
    pub async fn issue_browser_grant(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
    ) -> Result<SessionGrantTicket> {
        tokio::time::timeout(
            Duration::from_secs(5),
            self.issue_browser(actor, computer_id),
        )
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    async fn issue_browser(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
    ) -> Result<SessionGrantTicket> {
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
                "SELECT * FROM ONLY $policy;",
                vec![("policy", self.session_policy_record().into_value())],
            )
            .await?;
        let policy: Option<StoredPolicy> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        let policy = policy.ok_or(ComputerError::Unavailable)?;
        let limits = policy.checked()?;
        let grant_id = Uuid::now_v7();
        let (token, hash) = secret::issue(grant_id)?;
        let family = gateway_refresh_family_record_id(
            actor
                .accepted()
                .request_context
                .access_token
                .session_family
                .as_ref()
                .ok_or(ComputerError::Forbidden)?
                .as_str()
                .parse()
                .map_err(|_| ComputerError::Forbidden)?,
        );
        let mut params = authority::bindings(&snapshot);
        params.extend([
            ("computer", computer_record(computer_id).into_value()),
            ("computer_id", computer_id.into_value()),
            ("grant", super::record(grant_id).into_value()),
            ("grant_id", grant_id.into_value()),
            ("owner_key", owner_key(actor.owner())?.into_value()),
            ("provider", self.provider_instance_id.into_value()),
            ("authority", super::object(actor.accepted())?.into_value()),
            ("family", family.into_value()),
            ("ticket_hash", hash.into_value()),
            ("resource", computer.provider_resource_id.into_value()),
            ("process", computer.process_id.into_value()),
            ("policy", self.session_policy_record().into_value()),
            ("policy_fingerprint", policy.fingerprint.into_value()),
            (
                "absolute",
                surrealdb::types::Duration::from_secs(u64::from(limits.absolute_seconds))
                    .into_value(),
            ),
            (
                "idle",
                surrealdb::types::Duration::from_secs(u64::from(limits.idle_seconds)).into_value(),
            ),
            (
                "guard",
                RecordId::new(
                    "computer_session_grant_guard",
                    surrealdb::types::Uuid::from(computer_id),
                )
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
                include_str!("../../queries/issue_session_grant.surql"),
                params,
            )
            .await?;
        let result_index = response
            .num_statements()
            .checked_sub(1)
            .ok_or(ComputerError::Unavailable)?;
        let expires_at: Option<DateTime<Utc>> = response
            .take(result_index)
            .map_err(|_| ComputerError::Unavailable)?;
        Ok(SessionGrantTicket {
            computer_id,
            token,
            expires_at: expires_at.ok_or(ComputerError::Unavailable)?,
        })
    }
    pub async fn redeem_browser_grant(
        &self,
        actor: &ComputerActor,
        token: &TerminalToken,
    ) -> Result<SessionGrantHandle> {
        tokio::time::timeout(Duration::from_secs(5), self.redeem_browser(actor, token))
            .await
            .map_err(|_| ComputerError::Unavailable)?
    }
    async fn redeem_browser(
        &self,
        actor: &ComputerActor,
        token: &TerminalToken,
    ) -> Result<SessionGrantHandle> {
        actor.check_admission()?;
        let (grant_id, hash) = secret::parse(token)?;
        let row = self.session_grant(grant_id).await?;
        if row.ticket_expires_at <= Utc::now()
            || row.connection_id.is_some()
            || row.revoked_at.is_some()
        {
            return Err(ComputerError::Forbidden);
        }
        let accepted = row.accepted()?;
        let control = self.control_authority(actor).await?;
        control.require_attach(row.computer_id)?;
        if accepted.profile != actor.accepted().profile
            || accepted.actor.id != actor.accepted().actor.id
            || accepted.request_context.access_token.session_family
                != actor.accepted().request_context.access_token.session_family
            || accepted.request_context.access_token.oauth_client_id
                != actor
                    .accepted()
                    .request_context
                    .access_token
                    .oauth_client_id
            || accepted.invocation.work_context != actor.accepted().invocation.work_context
            || row.provider_instance_id != self.provider_instance_id
        {
            return Err(ComputerError::Forbidden);
        }
        let computer = self.get(actor.owner(), row.computer_id).await?;
        authority::ready(&computer, self.provider_instance_id)?;
        let connection_id = Uuid::now_v7();
        let mut result = self
            .query(
                include_str!("../../queries/redeem_session_grant.surql"),
                vec![
                    ("grant", super::record(grant_id).into_value()),
                    ("ticket_hash", hash.into_value()),
                    ("policy", self.session_policy_record().into_value()),
                    ("owner_key", owner_key(actor.owner())?.into_value()),
                    ("connection", connection_id.into_value()),
                    ("computer", computer_record(row.computer_id).into_value()),
                    ("provider", self.provider_instance_id.into_value()),
                    (
                        "admission_expires_at",
                        actor.admission_expires_at().into_value(),
                    ),
                    (
                        "event",
                        authority::event(
                            actor.accepted(),
                            row.computer_id,
                            grant_id,
                            "access_connected",
                        )?
                        .into_value(),
                    ),
                ],
            )
            .await?;
        let result_index = result
            .num_statements()
            .checked_sub(1)
            .ok_or(ComputerError::Unavailable)?;
        let redeemed: Option<Uuid> = result
            .take(result_index)
            .map_err(|_| ComputerError::Unavailable)?;
        if redeemed != Some(connection_id) {
            return Err(ComputerError::Forbidden);
        }
        Ok(SessionGrantHandle {
            grant_id,
            connection_id,
        })
    }
    pub(super) async fn session_grant(&self, grant_id: Uuid) -> Result<model::Record> {
        let mut result = self
            .query(
                "SELECT * FROM ONLY $grant;",
                vec![("grant", super::record(grant_id).into_value())],
            )
            .await?;
        let row: Option<model::Record> = result.take(0).map_err(|_| ComputerError::Unavailable)?;
        row.ok_or(ComputerError::NotFound)
    }
}
