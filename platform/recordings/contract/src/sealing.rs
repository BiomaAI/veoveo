//! Published seal results and manifests with checked immutable occurrence references.
use crate::{
    ManifestLayer, RecordingContractError, RecordingDatasetId, RecordingId, checked::text,
};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, num::NonZeroU64};
use veoveo_artifact_contract::ArtifactUri;
use veoveo_types::Sha256Digest;
use veoveo_types::sha256_hex;

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
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
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[schemars(rename = "SealRecordingOutput")]
pub struct SealRecordingOutputBuilder {
    pub recording_id: RecordingId,
    pub manifest_artifact_uri: ArtifactUri,
    pub layer_artifact_uris: Vec<ArtifactUri>,
    pub blueprint_artifact_uri: Option<ArtifactUri>,
}
impl SealRecordingOutputBuilder {
    pub fn build(self) -> Result<SealRecordingOutput, RecordingContractError> {
        veoveo_types::Checked::new(self).map(SealRecordingOutput)
    }
}
impl veoveo_types::Check for SealRecordingOutputBuilder {
    type Error = RecordingContractError;
    fn check(&self) -> Result<(), Self::Error> {
        let mut occurrences = BTreeSet::new();
        if self.layer_artifact_uris.is_empty()
            || !std::iter::once(&self.manifest_artifact_uri)
                .chain(&self.layer_artifact_uris)
                .chain(self.blueprint_artifact_uri.as_ref())
                .all(|uri| occurrences.insert(uri.artifact_id()))
        {
            return Err(RecordingContractError::Seal);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct SealRecordingOutput(veoveo_types::Checked<SealRecordingOutputBuilder>);
impl std::ops::Deref for SealRecordingOutput {
    type Target = SealRecordingOutputBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}

pub const RECORDING_MANIFEST_SCHEMA: &str = "veoveo.ai/recording-manifest/v10";
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[schemars(transform = crate::format_tag_role)]
pub enum RecordingManifestSchema {
    #[serde(rename = "veoveo.ai/recording-manifest/v10")]
    V10,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[schemars(rename = "RecordingManifest")]
pub struct RecordingManifestBuilder {
    pub schema: RecordingManifestSchema,
    pub dataset_id: RecordingDatasetId,
    pub recording_segment_id: RecordingId,
    pub catalog_revision: String,
    pub layers: Vec<ManifestLayer>,
    pub blueprint: Option<ManifestBlueprint>,
    #[schemars(with = "String", extend("format" = "date-time"))]
    pub sealed_at: DateTime<Utc>,
}
impl RecordingManifestBuilder {
    pub fn build(self) -> Result<RecordingManifest, RecordingContractError> {
        veoveo_types::Checked::new(self).map(RecordingManifest)
    }
}
impl veoveo_types::Check for RecordingManifestBuilder {
    type Error = RecordingContractError;
    fn check(&self) -> Result<(), Self::Error> {
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
        Ok(())
    }
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
#[schemars(transparent)]
pub struct RecordingManifest(veoveo_types::Checked<RecordingManifestBuilder>);
impl std::ops::Deref for RecordingManifest {
    type Target = RecordingManifestBuilder;
    fn deref(&self) -> &Self::Target {
        self.0.get()
    }
}

/// Blueprint facts checked with their enclosing manifest.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ManifestBlueprint {
    pub blueprint_id: String,
    pub revision: NonZeroU64,
    pub byte_len: NonZeroU64,
    pub message_count: NonZeroU64,
    #[serde(with = "sha256_hex")]
    #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
    pub sha256: Sha256Digest,
    pub artifact_uri: ArtifactUri,
}

/// Existing public-edge bound shared by manifest receivers.
pub const MAX_RECORDING_MANIFEST_BYTES: u64 = 8 * 1024 * 1024;

impl RecordingManifest {
    /// Bind supplied immutable facts; current authorization belongs to the service.
    pub fn validate_selection(
        &self,
        dataset: crate::RecordingDatasetId,
        recording: crate::RecordingId,
        layers: &[ManifestLayer],
        blueprint: Option<&ManifestBlueprint>,
    ) -> Result<(), RecordingContractError> {
        let layer_agrees = |(actual, selected): (&ManifestLayer, &ManifestLayer)| {
            actual.layer_id == selected.layer_id
                && actual.layer_name == selected.layer_name
                && actual.kind == selected.kind
                && actual.ordinal == selected.ordinal
                && actual.byte_len == selected.byte_len
                && actual.sha256 == selected.sha256
                && actual.artifact_uri.artifact_id() == selected.artifact_uri.artifact_id()
                && actual.rrd_version == selected.rrd_version
                && actual.schema_digest == selected.schema_digest
        };
        let blueprint_agrees = match (self.blueprint.as_ref(), blueprint) {
            (None, None) => true,
            (Some(actual), Some(selected)) => {
                actual.blueprint_id == selected.blueprint_id
                    && actual.revision == selected.revision
                    && actual.byte_len == selected.byte_len
                    && actual.message_count == selected.message_count
                    && actual.sha256 == selected.sha256
                    && actual.artifact_uri.artifact_id() == selected.artifact_uri.artifact_id()
            }
            _ => false,
        };
        if self.dataset_id != dataset
            || self.recording_segment_id != recording
            || self.layers.len() != layers.len()
            || !self.layers.iter().zip(layers).all(layer_agrees)
            || !blueprint_agrees
        {
            return Err(RecordingContractError::Seal);
        }
        Ok(())
    }
}
