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

use crate::{AcceptedAuthority, Result, identity::digest};
use surrealdb::types::RecordId;

fn record(id: crate::api::ExecutionId) -> RecordId {
    RecordId::new(
        "computer_execution",
        surrealdb::types::Uuid::from(id.as_uuid()),
    )
}
fn payload_record(id: crate::api::ExecutionId) -> RecordId {
    RecordId::new(
        "computer_execution_payload",
        surrealdb::types::Uuid::from(id.as_uuid()),
    )
}
pub(crate) fn slot(computer: veoveo_computers_contract::ComputerId) -> RecordId {
    RecordId::new(
        "computer_execution_slot",
        surrealdb::types::Uuid::from(computer.as_uuid()),
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
fn request(
    actor_key: &str,
    computer: veoveo_computers_contract::ComputerId,
    request: crate::api::RequestId,
) -> Result<RecordId> {
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
