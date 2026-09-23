//! Durable core Computers domain. Public facades do not own provider mutations.
mod active_execution;
mod admission;
mod authority;
mod authority_snapshot;
pub mod automation_grants;
mod capacity;
pub mod cli_grants;
pub mod commands;
mod computer_access;
mod control_authority;
mod control_session;
mod current_authority;
pub mod files;
mod identity;
mod lifecycle;
pub mod maintenance;
mod model;
mod operation;
mod operation_admission;
mod operation_authority;
pub mod secrets;
pub mod session_grants;
mod store;
mod worker_journal;
mod worker_queue;

pub use admission::{CapacityPolicy, Reservation};
pub use authority::{AcceptedAuthority, ComputerActor};
pub use computer_access::{ComputerReadAccess, ComputerReadPage};
pub use control_authority::ControlAuthority;
pub use current_authority::{AutomationLifecycleDecision, ExecutionDecision};
pub use lifecycle::{
    DispatchTicket, ObservationAdmission, ObservationTicket, ReachedPhase, ReachedState,
};
pub use model::{Computer, ComputerPage};
pub use operation::{Operation, OperationStage};
pub use operation_authority::OperationAccess;
pub use store::ComputersStore;
pub use veoveo_computers_contract as api;
pub use worker_queue::UndispatchedOutcome;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ComputerError {
    #[error("Computer was not found")]
    NotFound,
    #[error(
        "The Computer request has a missing or invalid field; check it against the tool's input schema"
    )]
    InvalidInput,
    #[error("You don't have permission to perform this Computer action")]
    Forbidden,
    #[error(
        "No Computer capacity is available right now; try again later or ask an operator to add capacity"
    )]
    CapacityFull,
    #[error("Computer access limit reached; close an existing connection before connecting again")]
    AccessLimit,
    #[error("Request ID was already used with different inputs")]
    RequestConflict,
    #[error("The installation's Computer capacity settings changed during this request; retry it")]
    PolicyConflict,
    #[error(
        "This Computer is already running another operation; wait for it to finish, then retry"
    )]
    OperationBusy,
    #[error("The Computer changed during this request; read its current state and retry")]
    StateConflict,
    #[error(
        "The Computer isn't in a state that allows this action; read its current state to see which actions are available"
    )]
    InvalidState,
    #[error("Computers is temporarily unavailable. Try again shortly")]
    Unavailable,
}
pub type Result<T> = std::result::Result<T, ComputerError>;
