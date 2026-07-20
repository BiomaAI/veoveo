use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ArtifactId;
use crate::gateway::{
    DataLabelId, DelegationId, PolicyVersion, PrincipalId, TenantId, WorkContextId,
};
use crate::{AccessSubject, InvocationMode};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactReleaseState {
    #[default]
    Private,
    Releasable,
    Released,
}

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
    pub owner: Option<AccessSubject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_context: Option<WorkContextId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ArtifactProvenance>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub data_labels: BTreeSet<DataLabelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_expires_at: Option<DateTime<Utc>>,
}

/// Immutable explanation of how an artifact came into being.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactProvenance {
    pub producer: PrincipalId,
    pub invocation_mode: InvocationMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initiator: Option<PrincipalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delegation_id: Option<DelegationId>,
    pub policy_revision: PolicyVersion,
}

/// Canonical metadata for an artifact managed by a server-owned store.
///
/// `artifact_uri` is the protocol identity (`{scheme}://artifact/{artifact_id}`).
/// `download_url` is optional bulk-transfer plumbing for large artifacts and
/// must not become a discovery API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactMetadata {
    pub artifact_id: ArtifactId,
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
    pub release_state: ArtifactReleaseState,
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

    /// Rewrite `artifact_uri` into the given server's scheme
    /// (`{scheme}://artifact/{artifact_id}`) for client-facing presentation.
    ///
    /// The artifact service stamps the neutral plane identity `artifact://{id}`;
    /// each domain server presents artifacts under its own scheme (matching its
    /// resource templates and read paths), so callers get a URI the same server
    /// can resolve back. The neutral form remains valid cross-server input.
    pub fn presented_under_scheme(mut self, scheme: &str) -> Self {
        self.artifact_uri = format!("{scheme}://artifact/{}", self.artifact_id);
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

#[cfg(test)]
mod presentation_tests {
    use super::*;

    #[test]
    fn presented_under_scheme_rewrites_uri_to_server_scheme() {
        let artifact_id = ArtifactId::new();
        let meta = ArtifactMetadata {
            artifact_id,
            byte_len: 3,
            mime_type: None,
            filename: None,
            // Plane stamps the neutral identity...
            artifact_uri: artifact_id.plane_uri(),
            download_url: None,
            created_at: Utc::now(),
            release_state: ArtifactReleaseState::Private,
            compliance: ComplianceMetadata::default(),
            metadata: Value::Null,
        };
        // ...and each server re-presents it under its own scheme.
        let presented = meta.presented_under_scheme("media");
        assert_eq!(
            presented.artifact_uri,
            format!("media://artifact/{artifact_id}")
        );
        assert_eq!(
            crate::access::parse_artifact_plane_uri(&presented.artifact_uri)
                .unwrap()
                .to_string(),
            artifact_id.to_string()
        );
    }
}
