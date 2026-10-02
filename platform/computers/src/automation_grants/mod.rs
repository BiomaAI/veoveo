//! Durable named-principal permissions; no bearer secret and no dispatch authority.
mod admission;
mod authority;
mod management;
mod model;
mod policy;

use crate::{AcceptedAuthority, Computer, ComputerError, ComputersStore, Result};
pub use authority::AutomationAuthority;
pub(crate) use authority::GrantReadPermit;
pub use policy::AutomationGrantPolicy;
use surrealdb::types::{RecordId, SurrealValue};

pub(crate) fn record(id: crate::api::AutomationGrantId) -> RecordId {
    RecordId::new(
        "computer_automation_grant",
        surrealdb::types::Uuid::from(id.into_uuid()),
    )
}

impl ComputersStore {
    async fn owned_automation_grant(
        &self,
        computer: &Computer,
        id: crate::api::AutomationGrantId,
    ) -> Result<model::Grant> {
        let mut response = self
            .query(
                include_str!("../../queries/owned_automation_grant.surql"),
                vec![
                    ("grant", record(id).into_value()),
                    ("computer", computer.computer_id.into_uuid().into_value()),
                    (
                        "owner_key",
                        crate::identity::owner_key(&computer.owner)?.into_value(),
                    ),
                    (
                        "provider",
                        self.provider_instance_id.into_uuid().into_value(),
                    ),
                ],
            )
            .await?;
        let row: Option<model::Record> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        row.ok_or(ComputerError::NotFound)?.try_into()
    }

    /// Private authorization metadata is scoped to the verified grantee before decoding.
    /// Current policy still has to admit the Computer before its row can be read.
    async fn grantee_automation_grant(
        &self,
        accepted: &AcceptedAuthority,
        computer: crate::api::ComputerId,
        id: crate::api::AutomationGrantId,
    ) -> Result<model::Grant> {
        let mut params = crate::computer_access::scope(accepted)?;
        params.extend([
            ("grant", record(id).into_value()),
            ("computer", computer.into_uuid().into_value()),
            (
                "provider",
                self.provider_instance_id.into_uuid().into_value(),
            ),
        ]);
        let mut read = self
            .query(
                include_str!("../../queries/grantee_automation_grant.surql"),
                params,
            )
            .await?;
        let row: Option<model::Record> = read.take(0).map_err(|_| ComputerError::Unavailable)?;
        row.ok_or(ComputerError::NotFound)?.try_into()
    }
}
