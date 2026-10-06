use super::model;
use crate::{ComputerActor, ComputerError, ComputersStore, Result, api::*, identity::owner_key};
use std::time::Duration;
use surrealdb::types::SurrealValue;

impl ComputersStore {
    /// Outstanding owned grants. No token, authority envelope, provider or session
    /// identifier crosses this projection, and redemption does not imply liveness.
    pub async fn access_grants(
        &self,
        actor: &ComputerActor,
        computer_id: veoveo_computers_contract::ComputerId,
    ) -> Result<AccessGrantCollection> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let control = self.control_authority(actor).await?;
            control.require_read(Some(computer_id))?;
            let computer = self.get(actor.owner(), computer_id).await?;
            let owner = owner_key(&computer.owner)?;
            let mut params = crate::store::owner_query_bindings(actor.owner())?;
            params.extend([
                ("provider", self.provider_instance_id.as_uuid().into_value()),
                ("owner_key", owner.clone().into_value()),
                ("computer_id", computer_id.as_uuid().into_value()),
            ]);
            let mut response = self
                .query(
                    include_str!("../../queries/browser_access_grants.surql"),
                    params,
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
                    crate::identity::verify_retained_owner(
                        &computer.owner,
                        &row.owner_key,
                        &accepted.task_owner(),
                    )?;
                    if row.owner_key != owner || row.computer_id != computer_id.as_uuid() {
                        return Err(ComputerError::Unavailable);
                    }
                    veoveo_computers_contract::AccessGrantViewValue {
                        grant_id: crate::api::AccessGrantId::try_from(row.grant_id)
                            .map_err(|_| ComputerError::Unavailable)?,
                        kind: AccessGrantKind::Browser,
                        name: "Browser".into(),
                        redeemed: row.connection_id.is_some(),
                        current_session: current_family.is_some()
                            && current_family
                                == &accepted.request_context.access_token.session_family,
                        issued_at: row.issued_at,
                        expires_at: row.expires_at,
                        last_activity_at: row.last_activity_at,
                    }
                    .build()
                    .map_err(|_| crate::ComputerError::Unavailable)
                })
                .collect::<Result<Vec<_>>>()?;
            grants.extend(
                self.cli_access_grants(actor, computer_id)
                    .await?
                    .into_iter()
                    .map(|row| {
                        veoveo_computers_contract::AccessGrantViewValue {
                            grant_id: row.grant_id,
                            kind: AccessGrantKind::Cli,
                            name: row.name,
                            redeemed: true,
                            current_session: row.current_session,
                            issued_at: row.issued_at,
                            expires_at: row.expires_at,
                            last_activity_at: row.last_activity_at,
                        }
                        .build()
                        .map_err(|_| crate::ComputerError::Unavailable)
                    })
                    .collect::<Result<Vec<_>>>()?,
            );
            if grants.len() > 128 {
                return Err(ComputerError::Unavailable);
            }
            grants.sort_by_key(|grant| std::cmp::Reverse(grant.grant_id));
            control.require_read(Some(computer_id))?;
            veoveo_computers_contract::AccessGrantCollectionValue {
                computer_id,
                grants,
            }
            .build()
            .map_err(|_| crate::ComputerError::Unavailable)
        })
        .await
        .map_err(|_| ComputerError::Unavailable)?
    }

    /// Dispatch the canonical grant ID to its owning ledger. Both kinds retain
    /// exact owner/parent checks and permit reducing access without attach rights.
    pub async fn revoke_access(
        &self,
        actor: &ComputerActor,
        computer: veoveo_computers_contract::ComputerId,
        grant: crate::api::AccessGrantId,
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
