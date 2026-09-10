//! Durable core Computers domain. Public facades do not own provider mutations.
mod admission;
mod capacity;
mod identity;
mod model;
mod store;

pub use admission::{CapacityPolicy, Reservation};
pub use model::{Computer, ComputerPage};
pub use store::ComputersStore;
pub use veoveo_computers_contract as api;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ComputerError {
    #[error("Computer not found")]
    NotFound,
    #[error("Computer input is invalid")]
    InvalidInput,
    #[error("Computer capacity is full")]
    CapacityFull,
    #[error("Request ID was already used with different inputs")]
    RequestConflict,
    #[error("Computer capacity policy changed")]
    PolicyConflict,
    #[error("Computer state is unavailable")]
    Unavailable,
}
pub type Result<T> = std::result::Result<T, ComputerError>;
