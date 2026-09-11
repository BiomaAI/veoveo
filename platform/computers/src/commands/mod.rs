//! Durable command admission. A queued command cannot itself dispatch an effect.
mod access;
mod admission;
mod completion;
mod containment;
mod continuation;
mod dispatch;
mod journal;
mod model;
mod outcome;
mod output_access;
mod settlement;
mod tasks;
pub use access::{CommandTaskAccess, CommandTaskAction};
pub use completion::CommandExitTicket;
pub use containment::{CommandContainmentRead, CommandContainmentStop, ContainmentReadAdmission};
pub use continuation::{CommandContinuation, CommandRunAuthority};
pub use dispatch::{CommandDispatchDecision, CommandDispatchTicket};
pub use model::{CommandOperation, CommandStage};
pub use outcome::{CommandInterruption, CommandOutcome, CommandRefusal};

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
