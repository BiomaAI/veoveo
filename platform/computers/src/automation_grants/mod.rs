//! Durable named-principal permissions; no bearer secret and no dispatch authority.
mod admission;
mod authority;
mod management;
mod model;
mod policy;

use crate::{ComputerError, ComputersStore, Result};
pub use authority::AutomationAuthority;
pub use policy::AutomationGrantPolicy;
use surrealdb::types::{RecordId, SurrealValue};

pub(crate) fn record(id: crate::api::AutomationGrantId) -> RecordId {
    RecordId::new(
        "computer_automation_grant",
        surrealdb::types::Uuid::from(id.into_uuid()),
    )
}

impl ComputersStore {
    async fn automation_grant(&self, id: crate::api::AutomationGrantId) -> Result<model::Grant> {
        let mut response = self
            .query(
                "SELECT * FROM ONLY $grant;",
                vec![("grant", record(id).into_value())],
            )
            .await?;
        let row: Option<model::Record> =
            response.take(0).map_err(|_| ComputerError::Unavailable)?;
        row.ok_or(ComputerError::NotFound)?.try_into()
    }
}
