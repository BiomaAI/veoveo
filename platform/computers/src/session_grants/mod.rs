//! Retained browser access. Only a successful one-use ticket redemption creates a
//! handle; current family, policy, Computer run and grant state bound every renewal.
mod admission;
mod authority;
mod inventory;
mod model;
mod policy;
mod renewal;
mod secret;
pub(crate) use authority::require_attach;
pub use model::{SessionGrantHandle, SessionGrantLease, SessionGrantTicket};
pub use policy::SessionGrantPolicy;

use crate::{ComputerError, Result};
use serde::Serialize;
use surrealdb::types::RecordId;
use uuid::Uuid;
use veoveo_platform_store::OpenObject;

fn record(id: Uuid) -> RecordId {
    RecordId::new("computer_session_grant", surrealdb::types::Uuid::from(id))
}
fn object(value: &impl Serialize) -> Result<OpenObject> {
    serde_json::from_value(serde_json::to_value(value).map_err(|_| ComputerError::InvalidInput)?)
        .map_err(|_| ComputerError::InvalidInput)
}
