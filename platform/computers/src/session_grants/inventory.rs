use super::model;
use crate::{ComputerActor, ComputerError, ComputersStore, Result, api::*, identity::owner_key};
use std::time::Duration;
use surrealdb::types::SurrealValue;
use uuid::Uuid;

impl ComputersStore {
    /// Outstanding owned grants. No token, authority envelope, provider or session
    /// identifier crosses this projection, and redemption does not imply liveness.
    pub async fn access_grants(
        &self,
        actor: &ComputerActor,
        computer_id: Uuid,
    ) -> Result<AccessGrantCollection> {
        tokio::time::timeout(Duration::from_secs(5), async {
            if computer_id.is_nil() {
                return Err(ComputerError::InvalidInput);
            }
            let control = self.control_authority(actor).await?;
            control.require_read(Some(computer_id))?;
            self.get(actor.owner(), computer_id).await?;
            let owner = owner_key(actor.owner())?;
            let mut response = self
                .query(
                    "SELECT * FROM computer_session_grant WHERE owner_key = $owner_key \
                     AND computer_id = $computer_id AND revoked_at = NONE \
                     AND expires_at > time::now() AND idle_expires_at > time::now() \
                     AND family.revoked_at = NONE AND family.expires_at > time::now() \
                     AND (connection_id != NONE OR ticket_expires_at > time::now()) \
                     ORDER BY grant_id DESC LIMIT 129;",
                    vec![
                        ("owner_key", owner.clone().into_value()),
                        ("computer_id", computer_id.into_value()),
                    ],
                )
                .await?;
            let rows: Vec<model::Record> =
                response.take(0).map_err(|_| ComputerError::Unavailable)?;
            if rows.len() > 128 {
                return Err(ComputerError::Unavailable);
            }
            let current_family = &actor.accepted().request_context.access_token.session_family;
            let mut grants = rows
                .into_iter()
                .map(|row| {
                    let accepted = row.accepted()?;
                    if row.owner_key != owner || row.computer_id != computer_id {
                        return Err(ComputerError::Unavailable);
                    }
                    Ok(AccessGrantView {
                        grant_id: row.grant_id,
                        kind: AccessGrantKind::Browser,
                        name: "Browser".into(),
                        redeemed: row.connection_id.is_some(),
                        current_session: current_family.is_some()
                            && current_family
                                == &accepted.request_context.access_token.session_family,
                        issued_at: row.issued_at,
                        expires_at: row.expires_at,
                        last_activity_at: row.last_activity_at,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            grants.extend(
                self.cli_access_grants(actor, computer_id)
                    .await?
                    .into_iter()
                    .map(|row| AccessGrantView {
                        grant_id: row.grant_id,
                        kind: AccessGrantKind::Cli,
                        name: row.name,
                        redeemed: true,
                        current_session: row.current_session,
                        issued_at: row.issued_at,
                        expires_at: row.expires_at,
                        last_activity_at: row.last_activity_at,
                    }),
            );
            if grants.len() > 128 {
                return Err(ComputerError::Unavailable);
            }
            grants.sort_by_key(|grant| std::cmp::Reverse(grant.grant_id));
            control.require_read(Some(computer_id))?;
            Ok(AccessGrantCollection {
                computer_id,
                grants,
            })
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    /// Dispatch the canonical grant ID to its owning ledger. Both kinds retain
    /// exact owner/parent checks and permit reducing access without attach rights.
    pub async fn revoke_access(
        &self,
        actor: &ComputerActor,
        computer: Uuid,
        grant: Uuid,
    ) -> Result<()> {
        tokio::time::timeout(Duration::from_secs(5), async {
            match self.revoke_browser_grant(actor, computer, grant).await {
                Err(ComputerError::NotFound) => self.revoke_cli_grant(actor, computer, grant).await,
                result => result,
            }
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }
}
