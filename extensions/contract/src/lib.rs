//! Typed contracts for independently owned Veoveo extension releases.

/// Private extension Helm library API selected by compatibility releases.
pub const EXTENSION_HELM_LIBRARY_API: &str = "veoveo.io/extension-helm-library/v1";

mod artifact;
mod compatibility;
mod ids;
mod release;
mod schema;
mod simulation;

pub use artifact::{
    ArtifactDescriptor, ArtifactKind, ArtifactPlatform, CpuArchitecture, OperatingSystem,
};
pub use compatibility::{
    COMPATIBILITY_MANIFEST_SCHEMA, CompatibilityManifest, CompatibilityManifestSchema,
    ContractCompatibility, ContractKind, GpuRuntimeRequirement, RuntimeComponent,
    RuntimeComponentVersion, SdkCompatibility, SdkLanguage, SimulationRuntimeCompatibility,
};
pub use ids::{
    ArtifactCoordinate, ArtifactDigest, ArtifactName, CompatibilityReleaseId, ExtensionId,
    ReleaseVersion, SourceRevision, VersionRequirement,
};
pub use release::{
    EXTENSION_RELEASE_SCHEMA, ExtensionReleaseManifest, ExtensionReleaseSchema, ExtensionSource,
    SimulationOverlayRequirement,
};
pub use schema::{
    compatibility_manifest_schema, extension_release_schema, simulation_conformance_result_schema,
    simulation_runtime_build_lock_schema, simulation_runtime_release_evidence_schema,
};
pub use simulation::{
    NvidiaDriverCapability, PythonDistributionInput, SIMULATION_CONFORMANCE_RESULT_SCHEMA,
    SIMULATION_RUNTIME_BUILD_LOCK_SCHEMA, SIMULATION_RUNTIME_RELEASE_EVIDENCE_SCHEMA,
    SimulationAttestationEvidence, SimulationConformanceResult, SimulationConformanceResultSchema,
    SimulationGpuRequirement, SimulationHardwareEvidence, SimulationNewtonDynamicsEvidence,
    SimulationOverlayKind, SimulationProbeKind, SimulationProbeResult, SimulationRuntimeBuildLock,
    SimulationRuntimeBuildLockSchema, SimulationRuntimeReleaseEvidence,
    SimulationRuntimeReleaseEvidenceSchema, SimulationSourceInput,
};

use thiserror::Error;

/// Validation failure for a controlled extension document.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExtensionContractError {
    /// A typed identifier failed its construction rule.
    #[error("invalid {kind} {value:?}: {reason}")]
    InvalidIdentifier {
        /// Identifier category.
        kind: &'static str,
        /// Rejected value.
        value: String,
        /// Stable validation reason.
        reason: &'static str,
    },
    /// A document contains a duplicate controlled identity.
    #[error("duplicate {kind}: {identity}")]
    Duplicate {
        /// Duplicate category.
        kind: &'static str,
        /// Repeated identity.
        identity: String,
    },
    /// An artifact kind does not match the field that contains it.
    #[error("{field} requires artifact kind {expected:?}, received {actual:?}")]
    ArtifactKind {
        /// Owning field.
        field: &'static str,
        /// Required kind.
        expected: ArtifactKind,
        /// Actual kind.
        actual: ArtifactKind,
    },
    /// A controlled collection is empty or incomplete.
    #[error("{field} cannot be empty")]
    Empty {
        /// Empty field.
        field: &'static str,
    },
    /// A canonical runtime tuple differs from the required component set.
    #[error("simulation runtime component set differs: expected {expected:?}, received {actual:?}")]
    RuntimeComponents {
        /// Required components.
        expected: std::collections::BTreeSet<RuntimeComponent>,
        /// Supplied components.
        actual: std::collections::BTreeSet<RuntimeComponent>,
    },
}

pub(crate) fn invalid_identifier(
    kind: &'static str,
    value: &str,
    reason: &'static str,
) -> ExtensionContractError {
    ExtensionContractError::InvalidIdentifier {
        kind,
        value: value.to_owned(),
        reason,
    }
}
