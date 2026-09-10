//! Private durable metadata for the privileged retained-storage host.
mod command;
mod docker;
mod filesystem;
mod handoff;
mod identity;
mod journal;
pub mod plugin;
mod service;
pub mod transport;

pub use docker::Docker;
pub use filesystem::Filesystem;
pub use handoff::{Handoff, PhysicalWriter, WriterState};
pub use identity::{HomeIdentity, HostIdentity};
pub use journal::{AllocationRecord, AllocationState, BackingIdentity, Journal, Reservation};
pub use service::{Service, Template};

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
    #[error("retained storage free-space reserve would be exceeded")]
    CapacityExceeded,
    #[error("retained storage physical backend is unavailable")]
    BackendUnavailable,
    #[error("mount requires the current registered Computer writer")]
    WriterDenied,
    #[error("retained home requires governed deletion")]
    PurgeRequired,
}
pub type Result<T> = std::result::Result<T, StorageError>;
