//! Published seal results and manifests with checked immutable occurrence references.
use crate::{
    ManifestLayer, RecordingContractError, RecordingDatasetId, RecordingId,
    checked::{checked_model, text},
    hex_digest,
};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, num::NonZeroU64};
use veoveo_artifact_contract::ArtifactUri;
use veoveo_types::Sha256Digest;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SealRecordingRequest {
    pub recording_id: RecordingId,
}

/// Complete seal response inputs. Occurrences must be distinct across roles.
/// ```compile_fail
/// use veoveo_recording_contract::SealRecordingOutput;
/// fn replace_manifest(output: &mut SealRecordingOutput) {
///     output.layer_artifact_uris.clear();
/// }
/// ```
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "SealRecordingOutput")]
pub struct SealRecordingOutputBuilder {
    pub recording_id: RecordingId,
    pub manifest_artifact_uri: ArtifactUri,
    pub layer_artifact_uris: Vec<ArtifactUri>,
    pub blueprint_artifact_uri: Option<ArtifactUri>,
}
impl SealRecordingOutputBuilder {
    pub fn build(self) -> Result<SealRecordingOutput, RecordingContractError> {
        let mut occurrences = BTreeSet::new();
        if self.layer_artifact_uris.is_empty()
            || !std::iter::once(&self.manifest_artifact_uri)
                .chain(&self.layer_artifact_uris)
                .chain(self.blueprint_artifact_uri.as_ref())
                .all(|uri| occurrences.insert(uri.artifact_id()))
        {
            return Err(RecordingContractError::Seal);
        }
        Ok(SealRecordingOutput(self))
    }
}
checked_model!(SealRecordingOutput, SealRecordingOutputBuilder);

pub const RECORDING_MANIFEST_SCHEMA: &str = "veoveo.ai/recording-manifest/v9";
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
pub enum RecordingManifestSchema {
    #[serde(rename = "veoveo.ai/recording-manifest/v9")]
    V9,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "RecordingManifest")]
pub struct RecordingManifestBuilder {
    pub schema: RecordingManifestSchema,
    pub dataset_id: RecordingDatasetId,
    pub recording_segment_id: RecordingId,
    pub catalog_revision: String,
    pub layers: Vec<ManifestLayer>,
    pub blueprint: Option<ManifestBlueprint>,
    #[schemars(with = "String")]
    pub sealed_at: DateTime<Utc>,
}
impl RecordingManifestBuilder {
    pub fn build(self) -> Result<RecordingManifest, RecordingContractError> {
        let mut layer_ids = BTreeSet::new();
        let mut layer_names = BTreeSet::new();
        let mut occurrences = BTreeSet::new();
        let valid_layers = self.layers.iter().all(|layer| {
            layer_ids.insert(layer.layer_id)
                && layer_names.insert(&layer.layer_name)
                && occurrences.insert(layer.artifact_uri.artifact_id())
        });
        let valid_blueprint = self.blueprint.as_ref().is_none_or(|blueprint| {
            text(&blueprint.blueprint_id, 512)
                && occurrences.insert(blueprint.artifact_uri.artifact_id())
        });
        if !text(&self.catalog_revision, 128)
            || self.layers.is_empty()
            || !valid_layers
            || !valid_blueprint
        {
            return Err(RecordingContractError::Seal);
        }
        Ok(RecordingManifest(self))
    }
}
checked_model!(RecordingManifest, RecordingManifestBuilder);

/// Blueprint facts checked with their enclosing manifest.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManifestBlueprint {
    pub blueprint_id: String,
    pub revision: NonZeroU64,
    pub byte_len: NonZeroU64,
    pub message_count: NonZeroU64,
    #[serde(with = "hex_digest")]
    #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
    pub sha256: Sha256Digest,
    pub artifact_uri: ArtifactUri,
}
