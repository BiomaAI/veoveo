use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Compliance and tenancy labels that travel with server-owned artifacts.
///
/// These fields are metadata for policy enforcement and audit routing. They are
/// intentionally optional because local/dev servers may not have tenant or data
/// classification context yet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComplianceMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_expires_at: Option<String>,
}

/// Canonical metadata for an artifact managed by a server-owned store.
///
/// `artifact_uri` is the protocol identity (`{scheme}://artifact/{sha256}`).
/// `download_url` is optional bulk-transfer plumbing for large artifacts and
/// must not become a discovery API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub sha256: String,
    pub byte_len: u64,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    pub artifact_uri: String,
    #[serde(default)]
    pub download_url: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub compliance: ComplianceMetadata,
    #[serde(default)]
    pub metadata: Value,
}

/// Bytes and optional presentation metadata for a new artifact.
#[derive(Debug, Clone)]
pub struct ArtifactPut {
    pub bytes: Vec<u8>,
    pub mime_type: Option<String>,
    pub filename: Option<String>,
    pub compliance: ComplianceMetadata,
    pub metadata: Value,
}

impl ArtifactPut {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            mime_type: None,
            filename: None,
            compliance: ComplianceMetadata::default(),
            metadata: Value::Null,
        }
    }
}

/// Stored artifact payload plus its canonical metadata.
#[derive(Debug, Clone)]
pub struct ArtifactObject {
    pub metadata: ArtifactMetadata,
    pub bytes: Vec<u8>,
}

/// External artifact storage port.
///
/// Implementations can target local disk, S3-compatible stores, or any other
/// content-addressed object store. MCP servers depend on this port, not on a
/// concrete storage service.
#[async_trait]
pub trait ArtifactStore: Clone + Send + Sync + 'static {
    async fn put(&self, artifact: ArtifactPut) -> Result<ArtifactMetadata>;
    async fn get(&self, sha256: &str) -> Result<Option<ArtifactObject>>;
    async fn head(&self, sha256: &str) -> Result<Option<ArtifactMetadata>>;
}
