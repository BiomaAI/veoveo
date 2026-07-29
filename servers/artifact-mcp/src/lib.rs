//! Typed MCP contract for artifact discovery, authorization, and sharing.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{
    AccessLevel, AccessSubject, ArtifactId, ArtifactMetadata, ArtifactReleaseState,
    ArtifactShareLink, ArtifactShareLinkId, Grant,
};

pub const INDEX_URI: &str = "artifact://index";
pub const ARTIFACT_TEMPLATE: &str = "artifact://{artifact_id}";
pub const METADATA_TEMPLATE: &str = "artifact://metadata/{artifact_id}";
pub const GRANTS_TEMPLATE: &str = "artifact://grants/{artifact_id}";

/// Well-known surface roots (contract C18, C19). These literals must match
/// `veoveo_mcp_contract::ServerResourceUris::new("artifact")`; a unit test
/// below pins the equivalence.
pub const DOCS_URI: &str = "artifact://docs";
pub const CONTRACT_URI: &str = "artifact://contract";
pub const DOC_TEMPLATE: &str = "artifact://docs/{doc_id}";

pub fn doc_uri(doc_id: &str) -> String {
    format!("artifact://docs/{doc_id}")
}

pub fn parse_doc_uri(uri: &str) -> Option<&str> {
    veoveo_mcp_contract::parse_server_doc_uri("artifact", uri)
}

pub fn artifact_uri(id: ArtifactId) -> String {
    id.plane_uri()
}

pub fn metadata_uri(id: ArtifactId) -> String {
    format!("artifact://metadata/{id}")
}

pub fn grants_uri(id: ArtifactId) -> String {
    format!("artifact://grants/{id}")
}

pub fn parse_metadata_uri(uri: &str) -> Option<ArtifactId> {
    ArtifactId::parse(uri.strip_prefix("artifact://metadata/")?).ok()
}

pub fn parse_grants_uri(uri: &str) -> Option<ArtifactId> {
    ArtifactId::parse(uri.strip_prefix("artifact://grants/")?).ok()
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_uris_match_the_shared_contract_conventions() {
        let conventions = veoveo_mcp_contract::ServerResourceUris::new("artifact");
        assert_eq!(DOCS_URI, conventions.docs_root_uri());
        assert_eq!(CONTRACT_URI, conventions.contract_uri());
        assert_eq!(DOC_TEMPLATE, conventions.doc_template());
        assert_eq!(doc_uri("agents"), conventions.doc_uri("agents"));
        assert_eq!(parse_doc_uri("artifact://docs/agents"), Some("agents"));
        assert_eq!(parse_doc_uri("artifact://docs"), None);
        assert_eq!(parse_doc_uri("artifact://docs/agents/extra"), None);
    }

    #[test]
    fn uri_shapes_are_strict_and_uuid_v7_based() {
        let id = ArtifactId::new();
        assert_eq!(parse_metadata_uri(&metadata_uri(id)), Some(id));
        assert_eq!(parse_grants_uri(&grants_uri(id)), Some(id));
        assert!(parse_metadata_uri("artifact://metadata/not-a-uuid").is_none());
        assert!(parse_grants_uri(&format!("artifact://grants/{id}/extra")).is_none());
    }
}
