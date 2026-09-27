//! Artifact-plane identities, metadata, and byte handoff values.
//! Authorization and transport are owned by the Artifact service and its adapters.

mod identity;
mod metadata;
mod provenance;

pub use identity::{ARTIFACT_PLANE_SCHEME, ArtifactId, ArtifactIdError, parse_artifact_plane_uri};
pub use metadata::{
    ArtifactMetadata, ArtifactObject, ArtifactPut, ArtifactReleaseState, ComplianceMetadata,
};
pub use provenance::ArtifactProvenance;
