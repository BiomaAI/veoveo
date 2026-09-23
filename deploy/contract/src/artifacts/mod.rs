//! Immutable identities shared by deployment locks and build publication.
mod descriptor;
mod ids;
pub use descriptor::{
    ArtifactDescriptor, ArtifactKind, ArtifactPlatform, CpuArchitecture, OperatingSystem,
};
pub use ids::{ArtifactCoordinate, ArtifactDigest, ArtifactName, ReleaseVersion, SourceRevision};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArtifactError {
    #[error("invalid {kind} {value:?}: {reason}")]
    InvalidIdentifier {
        kind: &'static str,
        value: String,
        reason: &'static str,
    },
}
fn invalid_identifier(kind: &'static str, value: &str, reason: &'static str) -> ArtifactError {
    ArtifactError::InvalidIdentifier {
        kind,
        value: value.to_owned(),
        reason,
    }
}
