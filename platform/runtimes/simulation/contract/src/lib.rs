//! Build inputs and hardware qualification for the shared simulation runtime.
mod components;
mod schema;
mod simulation;
pub use components::{GpuRuntimeRequirement, RuntimeComponent, RuntimeComponentVersion};
pub use schema::*;
pub use simulation::*;
use veoveo_deploy_contract::ArtifactKind;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SimulationContractError {
    #[error("duplicate {kind}: {identity}")]
    Duplicate {
        kind: &'static str,
        identity: String,
    },
    #[error("{field} requires artifact kind {expected:?}, received {actual:?}")]
    ArtifactKind {
        field: &'static str,
        expected: ArtifactKind,
        actual: ArtifactKind,
    },
    #[error("{field} cannot be empty or inconsistent")]
    Empty { field: &'static str },
    #[error("simulation runtime component set differs: expected {expected:?}, received {actual:?}")]
    RuntimeComponents {
        expected: std::collections::BTreeSet<RuntimeComponent>,
        actual: std::collections::BTreeSet<RuntimeComponent>,
    },
}
