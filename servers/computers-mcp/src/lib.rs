//! Computers worker and public projections share one domain journal.
#[cfg(feature = "contract")]
pub use veoveo_computers_contract as contract;
#[cfg(feature = "runtime")]
mod application;
#[cfg(feature = "runtime")]
mod command_worker;
#[cfg(feature = "mcp")]
pub mod config;
#[cfg(feature = "runtime")]
mod file_worker;
#[cfg(feature = "runtime")]
mod io_guard;
#[cfg(feature = "runtime")]
mod maintenance_worker;
#[cfg(feature = "runtime")]
mod preflight;
#[cfg(feature = "mcp")]
pub mod protocol;
#[cfg(feature = "runtime")]
mod runtime_access;
#[cfg(feature = "mcp")]
pub mod server;
#[cfg(feature = "runtime")]
mod templates;
#[cfg(feature = "runtime")]
mod worker;
#[cfg(feature = "runtime")]
pub use application::{Application, ApplicationError, CapacityHealth};
#[cfg(feature = "runtime")]
pub use command_worker::{CommandWorker, CommandWorkerError};
#[cfg(feature = "runtime")]
pub use maintenance_worker::{MaintenanceProfiles, MaintenanceTransition, MaintenanceWorker};
#[cfg(feature = "runtime")]
pub use preflight::{Preflight, PreflightError, RetainedHomes};
#[cfg(feature = "runtime")]
pub use runtime_access::{RuntimeAccess, RuntimePublisher};
#[cfg(feature = "runtime")]
pub use templates::{NamedTemplate, Templates};
#[cfg(feature = "runtime")]
pub use worker::{LifecycleWorker, WorkerError, WorkerStep};

#[cfg(feature = "runtime")]
pub use file_worker::{FileWorker, FileWorkerError};
