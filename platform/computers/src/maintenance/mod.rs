//! Durable retained maintenance identity. Provider and allocator proofs belong to
//! the owning worker; admission alone never replaces an instance or frees capacity.
mod admission;
mod authority;
pub(crate) use authority::{resume_target, target};
mod checkpoint;
mod journal;
mod model;
mod progress;
mod queue;
mod resume;
mod steps;
pub use model::{MaintenanceOperation, MaintenanceSource, MaintenanceStage, MaintenanceTarget};
pub use progress::{
    MaintenanceEvidence, MaintenanceRecovery, MaintenanceStep, MaintenanceStepRecord,
};
pub use steps::{MaintenanceObservationAdmission, MaintenanceTicket};

use surrealdb::types::RecordId;

fn record(id: veoveo_types::TaskId) -> RecordId {
    RecordId::new(
        "computer_maintenance",
        surrealdb::types::Uuid::from(id.as_uuid()),
    )
}
