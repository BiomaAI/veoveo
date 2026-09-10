use super::{CliConnectionHandle, CliGrantCredential, CliGrantView, model, secret};
use crate::{
    ComputerActor, ComputerError, ComputersStore, Result, identity::owner_key,
    session_grants::authority,
};
use chrono::Utc;
use std::time::Duration;
use surrealdb::types::SurrealValue;
use uuid::Uuid;

impl ComputersStore {
    pub async fn open_cli_connection(
        &self,
        expected_computer: Option<Uuid>,
        expected_profile: veoveo_mcp_contract::GatewayProfileId,
        credential: &CliGrantCredential,
    ) -> Result<CliConnectionHandle> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (grant_id, hash) = secret::parse(credential)?;
            let grant = self.cli_grant(grant_id).await?;
            if expected_computer.is_some_and(|id| id != grant.computer_id) {
                return Err(ComputerError::Forbidden);
            }
            let computer_id = grant.computer_id;
            let accepted = grant.accepted()?;
            if accepted.profile != expected_profile {
                return Err(ComputerError::Forbidden);
            }
            let snapshot = self.read_authority(&accepted).await?;
            let family_end = self
                .check_control_session(&snapshot)
                .await?
                .ok_or(ComputerError::Forbidden)?;
            authority::require_attach(&snapshot, computer_id)?;
            let computer = self.get(&accepted.task_owner(), computer_id).await?;
            authority::ready(&computer, self.provider_instance_id)?;
            let connection_id = Uuid::now_v7();
            let end = (snapshot.checked_at + chrono::TimeDelta::seconds(30))
                .min(family_end)
                .min(grant.expires_at)
                .min(grant.idle_expires_at);
            snapshot.check_fresh()?;
            self.query(
                include_str!("../../queries/open_cli_connection.surql"),
                vec![
                    ("grant", super::grant_record(grant_id).into_value()),
                    ("grant_id", grant_id.into_value()),
                    (
                        "computer",
                        crate::model::computer_record(computer_id).into_value(),
                    ),
                    ("computer_id", computer_id.into_value()),
                    ("provider", self.provider_instance_id.into_value()),
                    ("credential_hash", hash.into_value()),
                    (
                        "connection",
                        super::connection_record(connection_id).into_value(),
                    ),
                    ("connection_id", connection_id.into_value()),
                    ("policy", self.session_policy_record().into_value()),
                    ("resource", computer.provider_resource_id.into_value()),
                    ("process", computer.process_id.into_value()),
                    ("authority_expires_at", end.into_value()),
                    (
                        "event",
                        authority::event(&accepted, computer_id, grant_id, "access_connected")?
                            .into_value(),
                    ),
                ],
            )
            .await?;
            let handle = CliConnectionHandle {
                grant_id,
                connection_id,
            };
            // No provider I/O may start from admission alone. Tightened limits,
            // current policy, exact process and database-relative expiry apply now.
            if let Err(error) = self.renew_cli_grant(&handle, false).await {
                let _ = self.close_cli_connection(&handle).await;
                return Err(error);
            }
            Ok(handle)
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    pub(super) async fn cli_grant(&self, id: Uuid) -> Result<model::Grant> {
        let mut read = self
            .query(
                "SELECT * FROM ONLY $grant;",
                vec![("grant", super::grant_record(id).into_value())],
            )
            .await?;
        let grant: Option<model::Grant> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        grant.ok_or(ComputerError::NotFound)
    }
    pub async fn cli_access_grants(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
    ) -> Result<Vec<CliGrantView>> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let control = self.control_authority(actor).await?;
            control.require_read(Some(computer_id))?;
            self.get(actor.owner(), computer_id).await?;
            let owner = owner_key(actor.owner())?;
            let mut read = self.query("SELECT * FROM computer_cli_grant WHERE owner_key = $owner_key AND computer_id = $computer_id \
                AND revoked_at = NONE AND expires_at > time::now() AND idle_expires_at > time::now() \
                AND family.revoked_at = NONE AND family.expires_at > time::now() ORDER BY grant_id DESC LIMIT 129;", vec![
                ("owner_key", owner.clone().into_value()), ("computer_id", computer_id.into_value()),
            ]).await?;
            let rows: Vec<model::Grant> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
            if rows.len() > 128 { return Err(ComputerError::Unavailable); }
            let family = actor.accepted().request_context.access_token.session_family.as_ref();
            let result = rows.into_iter().map(|row| {
                let accepted = row.accepted()?;
                if row.owner_key != owner || row.computer_id != computer_id { return Err(ComputerError::Unavailable); }
                Ok(CliGrantView { grant_id:row.grant_id, name:row.name,
                    current_session:family.is_some() && family == accepted.request_context.access_token.session_family.as_ref(),
                    issued_at:row.issued_at, expires_at:row.expires_at, last_activity_at:row.last_activity_at })
            }).collect::<Result<Vec<_>>>()?;
            control.require_read(Some(computer_id))?;
            Ok(result)
        }).await.map_err(|_| ComputerError::Unavailable)?
    }
    pub async fn revoke_cli_grant(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
        grant_id: Uuid,
    ) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let row = self.cli_grant(grant_id).await?;
            if row.computer_id != computer_id || row.owner_key != owner_key(actor.owner())? {
                return Err(ComputerError::NotFound);
            }
            let control = self.control_authority(actor).await?;
            control.require_read(Some(computer_id))?;
            self.get(actor.owner(), computer_id).await?;
            self.query(
                include_str!("../../queries/revoke_cli_grant.surql"),
                vec![
                    ("grant", super::grant_record(grant_id).into_value()),
                    ("computer_id", computer_id.into_value()),
                    ("owner_key", row.owner_key.into_value()),
                    (
                        "admission_expires_at",
                        actor.admission_expires_at().into_value(),
                    ),
                    (
                        "event",
                        authority::event(
                            actor.accepted(),
                            computer_id,
                            grant_id,
                            "access_revoked",
                        )?
                        .into_value(),
                    ),
                ],
            )
            .await?;
            Ok(())
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
    /// Closing one TCP/gRPC connection never revokes the paired client.
    pub async fn close_cli_connection(&self, handle: &CliConnectionHandle) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            self.query("UPDATE ONLY $connection SET closed_at = $now WHERE connection_id = $connection_id AND grant_id = $grant_id AND closed_at = NONE;", vec![
                ("connection", super::connection_record(handle.connection_id).into_value()),
                ("connection_id", handle.connection_id.into_value()), ("grant_id", handle.grant_id.into_value()),
                ("now", Utc::now().into_value()),
            ]).await?;
            Ok(())
        }).await.map_err(|_| ComputerError::Unavailable)?
    }
}
