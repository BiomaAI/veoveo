//! Retained browser access. Only a successful one-use ticket redemption creates a
//! handle; current family, policy, Computer run and grant state bound every renewal.
mod admission;
pub(crate) mod authority;
mod inventory;
mod model;
pub(crate) mod policy;
mod renewal;
mod secret;
pub(crate) use authority::require_attach;
pub use model::{SessionGrantHandle, SessionGrantLease, SessionGrantTicket};
pub use policy::SessionGrantPolicy;

use surrealdb::types::RecordId;

fn record(id: crate::api::AccessGrantId) -> RecordId {
    RecordId::new(
        "computer_session_grant",
        surrealdb::types::Uuid::from(id.as_uuid()),
    )
}
