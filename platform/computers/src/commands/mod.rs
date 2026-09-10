//! Durable command admission. A queued command cannot itself dispatch an effect.
mod admission;
mod model;
mod tasks;
pub use model::QueuedCommand;

use crate::{AcceptedAuthority, ComputerError, Result, identity::digest};
use surrealdb::types::RecordId;
use uuid::Uuid;

fn record(id: Uuid) -> RecordId {
    RecordId::new("computer_execution", surrealdb::types::Uuid::from(id))
}
pub(crate) fn slot(computer: Uuid) -> RecordId {
    RecordId::new(
        "computer_execution_slot",
        surrealdb::types::Uuid::from(computer),
    )
}
fn actor_key(actor: &AcceptedAuthority) -> Result<String> {
    digest(&(
        "veoveo.computer.execution-actor.v1",
        crate::identity::owner_key(&actor.task_owner())?,
        &actor.request_context.principal.id,
        &actor.request_context.access_token.oauth_client_id,
    ))
}
fn request(actor_key: &str, computer: Uuid, request: Uuid) -> Result<RecordId> {
    if request.is_nil() {
        return Err(ComputerError::InvalidInput);
    }
    Ok(RecordId::new(
        "computer_execution_request",
        digest(&(
            "veoveo.computer.execution-request.v1",
            actor_key,
            computer,
            request,
        ))?,
    ))
}
