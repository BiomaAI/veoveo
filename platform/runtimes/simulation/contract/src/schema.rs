use crate::{
    SimulationConformanceResult, SimulationRuntimeBuildLock, SimulationRuntimeReleaseEvidence,
};
use schemars::{Schema, schema_for};

/// Generates the canonical simulation-runtime build-lock JSON Schema.
#[must_use]
pub fn simulation_runtime_build_lock_schema() -> Schema {
    schema_for!(SimulationRuntimeBuildLock)
}

/// Generates the canonical simulation-conformance result JSON Schema.
#[must_use]
pub fn simulation_conformance_result_schema() -> Schema {
    schema_for!(SimulationConformanceResult)
}

/// Generates the canonical simulation-runtime release-evidence JSON Schema.
#[must_use]
pub fn simulation_runtime_release_evidence_schema() -> Schema {
    schema_for!(SimulationRuntimeReleaseEvidence)
}
