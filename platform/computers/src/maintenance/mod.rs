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

use crate::{ComputerError, Result};
use serde::Serialize;
use surrealdb::types::RecordId;
use veoveo_platform_store::OpenObject;

fn record(id: veoveo_types::TaskId) -> RecordId {
    RecordId::new(
        "computer_maintenance",
        surrealdb::types::Uuid::from(id.as_uuid()),
    )
}
fn object(value: &impl Serialize) -> Result<OpenObject> {
    serde_json::from_value(serde_json::to_value(value).map_err(|_| ComputerError::Unavailable)?)
        .map_err(|_| ComputerError::Unavailable)
}
