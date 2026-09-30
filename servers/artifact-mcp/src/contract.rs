//! Typed MCP contract for artifact discovery, authorization, and sharing.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub use veoveo_artifact_contract::{
    ArtifactId, ArtifactMetadata, ArtifactReleaseState, ArtifactShareLink, ArtifactShareLinkId,
    Grant,
};

use veoveo_types::AccessLevel;
use veoveo_types::AccessSubject;

pub const INDEX_URI: &str = "artifact://index";
pub const LIBRARY_APP_URI: &str = "ui://artifact/library.html";
pub const ARTIFACT_TEMPLATE: &str = "artifact://{artifact_id}";
pub const METADATA_TEMPLATE: &str = "artifact://metadata/{artifact_id}";
pub const GRANTS_TEMPLATE: &str = "artifact://grants/{artifact_id}";

/// Well-known surface roots (contract C18, C19). These literals must match
/// the Artifact resource builders.
pub const DOCS_URI: &str = "artifact://docs";
pub const CONTRACT_URI: &str = "artifact://contract";
pub const DOC_TEMPLATE: &str = "artifact://docs/{doc_id}";

mod resources;
pub use resources::{
    ArtifactDocument, ArtifactResource, doc_uri, grants_uri, metadata_uri, parse_doc_uri,
    parse_grants_uri, parse_metadata_uri,
};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactReference {
    pub artifact_id: ArtifactId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GrantArtifactRequest {
    pub artifact_id: ArtifactId,
    pub subject: AccessSubject,
    pub level: AccessLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RevokeArtifactGrantRequest {
    pub artifact_id: ArtifactId,
    pub subject: AccessSubject,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SetArtifactReleaseRequest {
    pub artifact_id: ArtifactId,
    pub release_state: ArtifactReleaseState,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShareLinkOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_downloads: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateArtifactShareRequest {
    pub artifact_id: ArtifactId,
    #[serde(flatten)]
    pub options: ShareLinkOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RevokeArtifactShareRequest {
    pub artifact_id: ArtifactId,
    pub link_id: ArtifactShareLinkId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactMetadataOutput {
    pub artifact: ArtifactMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactGrantsOutput {
    pub artifact_id: ArtifactId,
    pub grants: Vec<Grant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactShareOutput {
    pub share_link: ArtifactShareLink,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactMutationOutput {
    pub artifact_id: ArtifactId,
    pub changed: bool,
}
