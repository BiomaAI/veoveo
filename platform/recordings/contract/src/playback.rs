//! Playback manifest admission shared by the producer and browser edge.
use std::{fmt, num::NonZeroU64, ops::Deref};
use veoveo_types::sha256_hex;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use veoveo_types::Sha256Digest;

use crate::{
    PlaybackArchiveUri, RecordingContractError, RecordingDatasetId, RecordingId,
    RecordingReadGrantId,
};

pub const PLAYBACK_MANIFEST_SCHEMA: &str = "veoveo.ai/recording-playback/v10";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
pub enum PlaybackManifestSchema {
    #[serde(rename = "veoveo.ai/recording-playback/v10")]
    V10,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RecordingState {
    Live,
    Ready,
    Sealing,
    Sealed,
    Interrupted,
    Failed,
}
impl fmt::Display for RecordingState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Live => "live",
            Self::Ready => "ready",
            Self::Sealing => "sealing",
            Self::Sealed => "sealed",
            Self::Interrupted => "interrupted",
            Self::Failed => "failed",
        })
    }
}

/// Complete construction inputs. `build` admits relationships before the manifest is usable.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaybackManifestBuilder {
    pub schema: PlaybackManifestSchema,
    pub dataset_id: RecordingDatasetId,
    pub recording_segment_id: RecordingId,
    pub application_id: String,
    pub recording_key: String,
    pub state: RecordingState,
    #[schemars(with = "String")]
    pub started_at: DateTime<Utc>,
    #[schemars(with = "Option<String>")]
    pub ended_at: Option<DateTime<Utc>>,
    pub catalog_revision: String,
    pub access: PlaybackAccess,
    pub archive: Option<PlaybackArchive>,
    pub live: Option<PlaybackLiveReceiver>,
    pub blueprint: Option<PlaybackBlueprint>,
}
impl PlaybackManifestBuilder {
    pub fn build(self) -> Result<PlaybackManifest, RecordingContractError> {
        let text = |value: &str, max| {
            !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
        };
        let mut valid = text(&self.application_id, 512)
            && text(&self.recording_key, 512)
            && text(&self.catalog_revision, 128)
            && !self.access.redap_token.trim().is_empty()
            && self.ended_at.is_none_or(|end| end >= self.started_at)
            && !(self.live.is_some() && self.archive.is_some());
        if self.state == RecordingState::Live {
            valid &= self.live.is_some() && self.archive.is_none() && self.ended_at.is_none();
        } else {
            valid &= self.live.is_none();
        }
        if let Some(archive) = &self.archive {
            valid &= archive.dataset_id == self.dataset_id
                && archive.recording_segment_id == self.recording_segment_id
                && archive.catalog_revision == self.catalog_revision
                && archive.uri.dataset_id() == self.dataset_id
                && archive.uri.recording_id() == self.recording_segment_id
                && text(&archive.rrd_version, 128)
                && text(&archive.optimization_profile, 128)
                && archive.layer_count > 0
                && archive.byte_len > 0;
        }
        if let Some(live) = &self.live {
            valid &= live.history_seconds > 0 && live.video_preroll_seconds > 0;
        }
        if let Some(blueprint) = &self.blueprint {
            valid &= text(&blueprint.blueprint_id, 512);
        }
        if !valid {
            return Err(RecordingContractError::Playback);
        }
        Ok(PlaybackManifest(self))
    }
}

/// An admitted, immutable manifest. Rust construction and JSON use the same checks.
/// ```compile_fail
/// use veoveo_recording_contract::PlaybackManifest;
/// fn change_parent(manifest: &mut PlaybackManifest) {
///     manifest.recording_segment_id = Default::default();
/// }
/// ```
#[derive(Clone, Debug, Serialize, JsonSchema)]
#[serde(transparent)]
#[schemars(with = "PlaybackManifestBuilder")]
pub struct PlaybackManifest(PlaybackManifestBuilder);
impl Deref for PlaybackManifest {
    type Target = PlaybackManifestBuilder;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<'de> Deserialize<'de> for PlaybackManifest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        PlaybackManifestBuilder::deserialize(deserializer)?
            .build()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaybackAccess {
    pub grant_id: RecordingReadGrantId,
    pub redap_token: String,
    #[schemars(with = "String")]
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaybackArchive {
    pub uri: PlaybackArchiveUri,
    pub dataset_id: RecordingDatasetId,
    pub recording_segment_id: RecordingId,
    pub catalog_revision: String,
    pub rrd_version: String,
    pub optimization_profile: String,
    pub byte_len: u64,
    pub layer_count: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
/// Recording-scoped channel, available throughout Live, including capture-layer gaps.
pub struct PlaybackLiveReceiver {
    pub history_seconds: u64,
    pub video_preroll_seconds: u64,
    pub transport: PlaybackLiveTransport,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackLiveTransport {
    RerunRrdChannelV2,
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaybackBlueprint {
    pub blueprint_id: String,
    pub revision: NonZeroU64,
    #[serde(with = "sha256_hex")]
    #[schemars(with = "String", regex(pattern = "^[0-9a-f]{64}$"))]
    pub sha256: Sha256Digest,
    pub byte_len: NonZeroU64,
    pub map_provider: PlaybackMapProvider,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PlaybackMapProvider {
    None,
    OpenStreetMap,
    Mapbox,
    Mixed,
}
