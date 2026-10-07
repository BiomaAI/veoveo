//! Recording-owned public Artifact metadata; the generic Artifact map stays open.
use crate::{RecordingDatasetId, RecordingId, RecordingLayerId, RecordingLayerKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{Sha256Digest, sha256_hex};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecordingCaptureMetadata {
    pub recording_id: RecordingId,
    pub dataset_id: RecordingDatasetId,
    pub layer_kind: RecordingLayerKind,
    #[serde(with = "sha256_hex")]
    #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
    pub schema_digest: Sha256Digest,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecordingArtifactMetadata {
    pub provenance: RecordingArtifactProvenance,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RecordingArtifactProvenance {
    RecordingLayer {
        layer_kind: RecordingLayerKind,
        dataset_id: RecordingDatasetId,
        recording_id: RecordingId,
        layer_id: RecordingLayerId,
        #[serde(with = "sha256_hex")]
        #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
        sha256: Sha256Digest,
    },
    RecordingBlueprint {
        dataset_id: RecordingDatasetId,
        recording_id: RecordingId,
        blueprint_id: String,
        revision: std::num::NonZeroU64,
        #[serde(with = "sha256_hex")]
        #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
        sha256: Sha256Digest,
    },
    RecordingManifest {
        recording_id: RecordingId,
        dataset_id: RecordingDatasetId,
        catalog_revision: String,
        dataset_revision: u64,
        recording_revision: u64,
        #[schemars(with = "String", extend("format" = "date-time"))]
        sealed_at: chrono::DateTime<chrono::Utc>,
        #[serde(with = "sha256_hex")]
        #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
        sha256: Sha256Digest,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum RecordingArtifactProvenanceKind {
    #[vocabulary(rename = "recording_layer")]
    RecordingLayer,
    #[vocabulary(rename = "recording_blueprint")]
    RecordingBlueprint,
    #[vocabulary(rename = "recording_manifest")]
    RecordingManifest,
}
impl RecordingArtifactProvenance {
    pub fn kind(&self) -> RecordingArtifactProvenanceKind {
        match self {
            Self::RecordingLayer { .. } => RecordingArtifactProvenanceKind::RecordingLayer,
            Self::RecordingBlueprint { .. } => RecordingArtifactProvenanceKind::RecordingBlueprint,
            Self::RecordingManifest { .. } => RecordingArtifactProvenanceKind::RecordingManifest,
        }
    }
}

/// Known native Recording-row metadata producers. Native source labels keep their
/// declared spelling; installation names retain their admitted owner identities.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "source", rename_all_fields = "camelCase", deny_unknown_fields)]
pub enum RecordingOriginMetadata {
    #[serde(rename = "authenticated-recording-ingest")]
    AuthenticatedIngest {
        producer_id: crate::RecordingProducerId,
    },
    #[serde(rename = "recording-hub")]
    Hub {
        dataset: crate::RecordingDatasetName,
    },
}

/// Validated publisher configuration that identifies one create-only publication profile.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecordingPublisherContext {
    pub client_id: String,
    pub profile: String,
    pub protected_resource: String,
}
