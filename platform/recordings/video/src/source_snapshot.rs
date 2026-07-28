use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_recording_mcp::{RecordingReadSnapshot, RecordingReadSource, RecordingReadSourceKind};

/// Exact immutable recording inputs captured for one analysis run.
///
/// The complete list belongs in a governed results artifact. Artifact
/// descriptors carry only its SHA-256 digest so their control-plane shape
/// remains bounded as a recording accumulates source parts.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecordingSourceSnapshot {
    pub recording_id: String,
    pub captured_at: DateTime<Utc>,
    pub sources: Vec<RecordingSourceIdentity>,
}

impl RecordingSourceSnapshot {
    pub fn digest_sha256(&self) -> Result<String, serde_json::Error> {
        serde_json::to_vec(self).map(|bytes| hex::encode(Sha256::digest(bytes)))
    }
}

impl From<&RecordingReadSnapshot> for RecordingSourceSnapshot {
    fn from(value: &RecordingReadSnapshot) -> Self {
        Self {
            recording_id: value.recording_id.to_string(),
            captured_at: value.captured_at,
            sources: value
                .sources
                .iter()
                .map(RecordingSourceIdentity::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RecordingSourceIdentity {
    pub segment_id: String,
    pub segment_ordinal: i64,
    pub kind: RecordingSourceIdentityKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_sequence: Option<u64>,
    pub byte_len: u64,
    pub sha256: String,
}

impl From<&RecordingReadSource> for RecordingSourceIdentity {
    fn from(value: &RecordingReadSource) -> Self {
        Self {
            segment_id: value.segment_id.to_string(),
            segment_ordinal: value.segment_ordinal,
            kind: value.kind.into(),
            part_sequence: value.part_sequence,
            byte_len: value.byte_len,
            sha256: value.sha256.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecordingSourceIdentityKind {
    FrozenSegment,
    SealedSegment,
    LiveIngestPart,
}

impl From<RecordingReadSourceKind> for RecordingSourceIdentityKind {
    fn from(value: RecordingReadSourceKind) -> Self {
        match value {
            RecordingReadSourceKind::FrozenSegment => Self::FrozenSegment,
            RecordingReadSourceKind::SealedSegment => Self::SealedSegment,
            RecordingReadSourceKind::LiveIngestPart => Self::LiveIngestPart,
        }
    }
}
