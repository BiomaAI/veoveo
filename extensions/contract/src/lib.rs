//! Typed contracts for independently owned Veoveo extension releases.

mod artifact;
mod compatibility;
mod ids;
mod release;
mod schema;

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
pub use schema::{compatibility_manifest_schema, extension_release_schema};

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
