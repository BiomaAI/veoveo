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
use uuid::Uuid;

pub(crate) fn record(id: Uuid) -> RecordId {
    RecordId::new(
        "computer_automation_grant",
        surrealdb::types::Uuid::from(id),
    )
}

impl ComputersStore {
    async fn automation_grant(&self, id: Uuid) -> Result<model::Grant> {
        if id.is_nil() {
            return Err(ComputerError::InvalidInput);
        }
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
