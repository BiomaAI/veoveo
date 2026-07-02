use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Canonical metadata for an artifact managed by a server-owned store.
///
/// `artifact_uri` is the protocol identity (`artifact://{sha256}`). `download_url`
/// is optional bulk-transfer plumbing for large artifacts and must not become a
/// discovery API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
}

/// Bytes and optional presentation metadata for a new artifact.
#[derive(Debug, Clone)]
pub struct ArtifactPut {
    pub bytes: Vec<u8>,
    pub mime_type: Option<String>,
    pub filename: Option<String>,
}

/// Stored artifact payload plus its canonical metadata.
#[derive(Debug, Clone)]
pub struct ArtifactObject {
    pub metadata: ArtifactMetadata,
    pub bytes: Vec<u8>,
}

/// External artifact storage port.
///
/// Implementations can target local disk, S3, MinIO, R2, or any other
/// content-addressed object store. MCP servers depend on this port, not on a
/// concrete storage service.
#[async_trait]
pub trait ArtifactStore: Clone + Send + Sync + 'static {
    async fn put(&self, artifact: ArtifactPut) -> Result<ArtifactMetadata>;
    async fn get(&self, sha256: &str) -> Result<Option<ArtifactObject>>;
    async fn head(&self, sha256: &str) -> Result<Option<ArtifactMetadata>>;
}
