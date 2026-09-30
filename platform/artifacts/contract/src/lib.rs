//! Artifact-plane identities, metadata, and byte handoff values.
//! Authorization and transport are owned by the Artifact service and its adapters.

mod access;
pub use access::{ArtifactShareLink, ArtifactShareLinkId, ArtifactShareLinkIdError, Grant};
mod identity;
mod metadata;
mod provenance;
mod uri;

pub use identity::{ARTIFACT_PLANE_SCHEME, ArtifactId, ArtifactIdError, parse_artifact_plane_uri};
pub use metadata::{
    ArtifactMetadata, ArtifactMetadataError, ArtifactObject, ArtifactPut, ArtifactReleaseState,
    ComplianceMetadata,
};
pub use provenance::ArtifactProvenance;
pub use uri::{ArtifactAddress, ArtifactUri, ArtifactUriError};
