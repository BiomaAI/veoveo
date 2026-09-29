//! Public layer facts and their lifecycle-dependent integrity requirements.
use crate::{
    RecordingContractError, RecordingLayerId,
    checked::{checked_model, text},
};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;
use veoveo_artifact_contract::ArtifactUri;
use veoveo_types::Sha256Digest;
use veoveo_types::sha256_hex;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecordingLayerKind {
    Capture,
    Properties,
    Derived,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecordingLayerState {
    Writing,
    Staged,
    Committed,
    Failed,
}

fn valid_identity(kind: RecordingLayerKind, name: &str, ordinal: Option<u64>) -> bool {
    text(name, 256)
        && match kind {
            RecordingLayerKind::Capture => {
                ordinal.is_some_and(|n| n <= i64::MAX as u64 && name == format!("capture-{n:020}"))
            }
            RecordingLayerKind::Properties => name == "properties" && ordinal.is_none(),
            RecordingLayerKind::Derived => name.starts_with("derived-") && ordinal.is_none(),
        }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "LayerView")]
pub struct LayerViewBuilder {
    pub layer_id: RecordingLayerId,
    pub layer_name: String,
    pub kind: RecordingLayerKind,
    pub ordinal: Option<u64>,
    pub state: RecordingLayerState,
    pub byte_len: u64,
    pub message_count: u64,
    #[serde(default, with = "sha256_hex::optional")]
    #[schemars(with = "Option<String>")]
    pub sha256: Option<Sha256Digest>,
    pub artifact_uri: Option<ArtifactUri>,
    pub rrd_version: Option<String>,
    #[serde(default, with = "sha256_hex::optional")]
    #[schemars(with = "Option<String>")]
    pub schema_digest: Option<Sha256Digest>,
    #[schemars(with = "String")]
    pub created_at: DateTime<Utc>,
    #[schemars(with = "String")]
    pub updated_at: DateTime<Utc>,
}
impl LayerViewBuilder {
    pub fn build(self) -> Result<LayerView, RecordingContractError> {
        let finalized = matches!(
            self.state,
            RecordingLayerState::Staged | RecordingLayerState::Committed
        );
        if !valid_identity(self.kind, &self.layer_name, self.ordinal)
            || self.updated_at < self.created_at
            || self
                .rrd_version
                .as_ref()
                .is_some_and(|version| !text(version, 64))
            || self.artifact_uri.is_some() != (self.state == RecordingLayerState::Committed)
            || (finalized
                && (self.byte_len == 0
                    || self.message_count == 0
                    || self.sha256.is_none()
                    || self.schema_digest.is_none()
                    || self.rrd_version.is_none()))
        {
            return Err(RecordingContractError::Layer);
        }
        Ok(LayerView(self))
    }
}
checked_model!(LayerView, LayerViewBuilder);

/// Committed layer facts for a sealed manifest.
/// ```compile_fail
/// use veoveo_recording_contract::ManifestLayer;
/// fn change_digest(layer: &mut ManifestLayer) { layer.sha256 = layer.sha256.clone(); }
/// ```
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "ManifestLayer")]
pub struct ManifestLayerBuilder {
    pub layer_id: RecordingLayerId,
    pub layer_name: String,
    pub kind: RecordingLayerKind,
    pub ordinal: Option<u64>,
    pub byte_len: NonZeroU64,
    #[serde(with = "sha256_hex")]
    #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
    pub sha256: Sha256Digest,
    pub artifact_uri: ArtifactUri,
    pub rrd_version: String,
    #[serde(with = "sha256_hex")]
    #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
    pub schema_digest: Sha256Digest,
}
impl ManifestLayerBuilder {
    pub fn build(self) -> Result<ManifestLayer, RecordingContractError> {
        if !valid_identity(self.kind, &self.layer_name, self.ordinal)
            || !text(&self.rrd_version, 64)
        {
            return Err(RecordingContractError::Layer);
        }
        Ok(ManifestLayer(self))
    }
}
checked_model!(ManifestLayer, ManifestLayerBuilder);
