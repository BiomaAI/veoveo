//! Private durable metadata for the privileged retained-storage host.
mod identity;
mod journal;

pub use identity::{HomeIdentity, HostIdentity};
pub use journal::{AllocationRecord, AllocationState, BackingIdentity, Journal, Reservation};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("invalid retained storage configuration or identity")]
    InvalidIdentity,
    #[error("retained storage is already owned by another helper")]
    Busy,
    #[error("retained storage identity does not match this request")]
    IdentityMismatch,
    #[error("retained storage metadata requires recovery")]
    RecoveryRequired,
    #[error("retained storage metadata is unavailable")]
    Unavailable,
}
pub type Result<T> = std::result::Result<T, StorageError>;
