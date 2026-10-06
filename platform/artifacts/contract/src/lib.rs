//! Artifact-plane identities, metadata, and byte handoff values.
//! Authorization and transport are owned by the Artifact service and its adapters.

mod access;
pub use access::{
    ArtifactGrantSubjectKind, ArtifactShareLink, ArtifactShareLinkId, ArtifactShareLinkIdError,
    Grant,
};
mod identity;
mod ledger;
mod metadata;
mod provenance;
mod snapshot;
mod uri;

pub use identity::{ARTIFACT_PLANE_SCHEME, ArtifactId, ArtifactIdError, parse_artifact_plane_uri};
pub use ledger::{
    ArtifactAccessRequestId, ArtifactLedgerAddress, ArtifactLedgerIdError,
    ArtifactReadCapabilityId, ArtifactTaskId, ArtifactUploadId, ArtifactUploadRequestId,
    ArtifactWriteCapabilityId,
};
pub use metadata::{
    ArtifactMetadata, ArtifactMetadataError, ArtifactObject, ArtifactPut, ArtifactReleaseState,
    ComplianceMetadata,
};
pub use provenance::ArtifactProvenance;
pub use snapshot::{ArtifactMetadataSnapshot, ArtifactReadGrant, ArtifactSnapshotError};
pub use uri::{ArtifactAddress, ArtifactUri, ArtifactUriError};
