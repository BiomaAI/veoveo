//! Durable core Computers domain. Public facades do not own provider mutations.
mod admission;
mod authority;
mod capacity;
mod current_authority;
mod identity;
mod lifecycle;
mod model;
mod operation;
mod operation_admission;
mod store;
mod worker_journal;
mod worker_queue;

pub use admission::{CapacityPolicy, Reservation};
pub use authority::{AcceptedAuthority, ComputerActor};
pub use current_authority::ExecutionDecision;
pub use lifecycle::{
    DispatchTicket, ObservationAdmission, ObservationTicket, ReachedPhase, ReachedState,
};
pub use model::{Computer, ComputerPage};
pub use operation::{Operation, OperationStage};
pub use store::ComputersStore;
pub use veoveo_computers_contract as api;
pub use worker_queue::UndispatchedOutcome;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ComputerError {
    #[error("Computer not found")]
    NotFound,
    #[error("Computer input is invalid")]
    InvalidInput,
    #[error("Current Work Context authority does not permit this Computer action")]
    Forbidden,
    #[error("Computer capacity is full")]
    CapacityFull,
    #[error("Request ID was already used with different inputs")]
    RequestConflict,
    #[error("Computer capacity policy changed")]
    PolicyConflict,
    #[error("Computer already has an active operation")]
    OperationBusy,
    #[error("Computer state changed")]
    StateConflict,
    #[error("Computer action requires a different lifecycle state")]
    InvalidState,
    #[error("Computer state is unavailable")]
    Unavailable,
}
pub type Result<T> = std::result::Result<T, ComputerError>;
