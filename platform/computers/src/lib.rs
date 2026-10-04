//! Durable core Computers domain. Public facades do not own provider mutations.
#[cfg(feature = "runtime")]
mod active_execution;
#[cfg(feature = "runtime")]
mod admission;
#[cfg(feature = "runtime")]
mod authority;
#[cfg(feature = "runtime")]
mod authority_changes;
#[cfg(feature = "runtime")]
mod authority_snapshot;
#[cfg(feature = "runtime")]
pub mod automation_grants;
#[cfg(feature = "runtime")]
mod capacity;
#[cfg(feature = "runtime")]
pub mod cli_grants;
#[cfg(feature = "runtime")]
pub mod commands;
#[cfg(feature = "runtime")]
mod computer_access;
#[cfg(feature = "runtime")]
mod control_authority;
#[cfg(feature = "runtime")]
mod control_session;
#[cfg(feature = "runtime")]
mod current_authority;
#[cfg(feature = "runtime")]
pub mod files;
#[cfg(feature = "runtime")]
mod identity;
#[cfg(feature = "runtime")]
mod lifecycle;
#[cfg(feature = "runtime")]
pub mod maintenance;
#[cfg(feature = "runtime")]
mod model;
#[cfg(feature = "runtime")]
mod observed_restart;
#[cfg(feature = "runtime")]
mod operation;
#[cfg(feature = "runtime")]
mod operation_admission;
#[cfg(feature = "runtime")]
mod operation_authority;
#[cfg(feature = "runtime")]
mod operation_reads;
#[cfg(feature = "runtime")]
pub mod secrets;
#[cfg(feature = "runtime")]
pub mod session_grants;
#[cfg(feature = "runtime")]
mod store;
#[cfg(feature = "runtime")]
mod task_access;
#[cfg(feature = "runtime")]
mod task_references;
#[cfg(feature = "runtime")]
mod worker_journal;
#[cfg(feature = "runtime")]
mod worker_queue;

#[cfg(feature = "runtime")]
pub use admission::{CapacityPolicy, Reservation};
#[cfg(feature = "runtime")]
pub use authority::{AcceptedAuthority, ComputerActor};
#[cfg(feature = "runtime")]
pub use authority_changes::{AuthorityChanges, AuthorityInterest};
#[cfg(feature = "runtime")]
pub use computer_access::{ComputerReadAccess, ComputerReadPage};
#[cfg(feature = "runtime")]
pub use control_authority::ControlAuthority;
#[cfg(feature = "runtime")]
pub use current_authority::{AutomationLifecycleDecision, ExecutionDecision};
#[cfg(feature = "runtime")]
pub use lifecycle::{
    DispatchTicket, ObservationAdmission, ObservationTicket, ReachedPhase, ReachedState,
};
#[cfg(feature = "runtime")]
pub use model::{Computer, ComputerPage};
#[cfg(feature = "runtime")]
pub use operation::{Operation, OperationStage};
#[cfg(feature = "runtime")]
pub use operation_authority::OperationAccess;
#[cfg(feature = "runtime")]
pub use store::ComputersStore;
#[cfg(feature = "runtime")]
pub use veoveo_computers_contract as api;
#[cfg(feature = "runtime")]
pub use worker_queue::UndispatchedOutcome;

#[cfg(feature = "runtime")]
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
#[cfg(feature = "runtime")]
pub type Result<T> = std::result::Result<T, ComputerError>;
#[cfg(feature = "runtime")]
mod audit;

#[cfg(feature = "schema")]
pub mod schema;

#[cfg(feature = "gateway")]
pub mod gateway;

#[cfg(all(test, feature = "gateway"))]
#[path = "../../../testing/fixtures/catalog_admission.rs"]
mod test_catalog_admission;
#[cfg(all(test, feature = "gateway"))]
#[path = "../../../testing/fixtures/store.rs"]
mod test_store;
