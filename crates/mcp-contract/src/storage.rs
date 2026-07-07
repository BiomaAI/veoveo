use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::gateway::{DataLabelId, PrincipalId, TenantId};

/// Compliance and tenancy labels that travel with server-owned artifacts.
///
/// Object metadata is not the only authorization source; servers should keep
/// owner rows for task/artifact access checks. These typed fields keep exported
/// artifact metadata aligned with the gateway principal and policy model.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ComplianceMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<DataLabelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<TenantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<PrincipalId>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub data_labels: BTreeSet<DataLabelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_expires_at: Option<DateTime<Utc>>,
}

/// Canonical metadata for an artifact managed by a server-owned store.
///
/// `artifact_uri` is the protocol identity (`{scheme}://artifact/{sha256}`).
/// `download_url` is optional bulk-transfer plumbing for large artifacts and
/// must not become a discovery API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactMetadata {
    pub sha256: String,
    pub byte_len: u64,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    pub artifact_uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub compliance: ComplianceMetadata,
    #[serde(default)]
    pub metadata: Value,
}

impl ArtifactMetadata {
    pub fn without_download_url(mut self) -> Self {
        self.download_url = None;
        self
    }
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
#[derive(Debug, Clone, PartialEq)]
pub struct ArtifactObject {
    pub metadata: ArtifactMetadata,
    pub bytes: Vec<u8>,
}
