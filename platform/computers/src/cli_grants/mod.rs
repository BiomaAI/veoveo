//! Session-bound stock CLI authority. Pairing consumes one browser-confirmed
//! challenge; each connection obtains a separately fenced, renewable handle.
mod access;
mod model;
mod pairing;
mod renewal;
mod secret;
pub use model::{
    CliConnectionHandle, CliGrantCredential, CliGrantLease, CliGrantView, CliPairing,
    CliPairingRequest, PairedCliGrant,
};

use surrealdb::types::RecordId;
use uuid::Uuid;

fn record(table: &str, id: Uuid) -> RecordId {
    RecordId::new(table, surrealdb::types::Uuid::from(id))
}
fn grant_record(id: Uuid) -> RecordId {
    record("computer_cli_grant", id)
}
fn connection_record(id: Uuid) -> RecordId {
    record("computer_cli_connection", id)
}
fn pairing_record(id: Uuid) -> RecordId {
    record("computer_cli_pairing", id)
}
