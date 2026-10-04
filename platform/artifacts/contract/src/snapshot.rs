//! Service-owned metadata and access state read from one repository snapshot.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ArtifactMetadata, Grant};
use veoveo_types::AccessSubject;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "SnapshotWire", into = "SnapshotWire")]
pub struct ArtifactMetadataSnapshot(SnapshotWire);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SnapshotWire {
    metadata: ArtifactMetadata,
    read_grants: Vec<ArtifactReadGrant>,
    metadata_updated_at: DateTime<Utc>,
}

/// One subject allowed to read, with the deadline recorded by the Artifact service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReadGrant {
    pub subject: AccessSubject,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "artifact snapshot requires neutral metadata, stored attribution, matching grants and a valid metadata timestamp"
)]
pub struct ArtifactSnapshotError;

impl ArtifactMetadataSnapshot {
    pub fn new(
        metadata: ArtifactMetadata,
        grants: Vec<Grant>,
        metadata_updated_at: DateTime<Utc>,
    ) -> Result<Self, ArtifactSnapshotError> {
        if grants.iter().any(|grant| {
            grant.artifact != metadata.artifact_id()
                || Some(&grant.tenant) != metadata.compliance.tenant_id.as_ref()
        }) {
            return Err(ArtifactSnapshotError);
        }
        SnapshotWire {
            metadata,
            read_grants: grants
                .into_iter()
                .map(|grant| ArtifactReadGrant {
                    subject: grant.subject,
                    expires_at: grant.retention_expires_at,
                })
                .collect(),
            metadata_updated_at,
        }
        .try_into()
    }

    pub fn metadata(&self) -> &ArtifactMetadata {
        &self.0.metadata
    }
    pub fn read_grants(&self) -> &[ArtifactReadGrant] {
        &self.0.read_grants
    }
    /// Last metadata mutation. Grant changes have their own revision inputs.
    pub fn metadata_updated_at(&self) -> DateTime<Utc> {
        self.0.metadata_updated_at
    }
    pub fn into_metadata(self) -> ArtifactMetadata {
        self.0.metadata
    }
}

impl TryFrom<SnapshotWire> for ArtifactMetadataSnapshot {
    type Error = ArtifactSnapshotError;
    fn try_from(mut value: SnapshotWire) -> Result<Self, Self::Error> {
        let metadata = &value.metadata;
        let compliance = &metadata.compliance;
        if metadata.download_url.is_some()
            || metadata.artifact_uri != metadata.artifact_id().plane_uri()
            || compliance.tenant_id.is_none()
            || compliance.owner.is_none()
            || compliance.work_context.is_none()
            || compliance.provenance.is_none()
            || value.metadata_updated_at < metadata.created_at
        {
            return Err(ArtifactSnapshotError);
        }
        value.read_grants.sort_by(|a, b| a.subject.cmp(&b.subject));
        if value
            .read_grants
            .windows(2)
            .any(|pair| pair[0].subject == pair[1].subject)
        {
            return Err(ArtifactSnapshotError);
        }
        Ok(Self(value))
    }
}
impl From<ArtifactMetadataSnapshot> for SnapshotWire {
    fn from(value: ArtifactMetadataSnapshot) -> Self {
        value.0
    }
}
