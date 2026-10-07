//! Private, bounded microphone session. PCM is binary and never a tool argument.
use crate::{DictationSessionId, DictationUri};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const MAX_CHUNK_BYTES: usize = 192_000;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartDictation {
    pub id: DictationSessionId,
    pub sample_rate: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DictationId {
    pub id: DictationSessionId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DictationStatus {
    Listening,
    Flushing,
    Completed,
    Cancelled,
    Failed,
}
impl DictationStatus {
    pub fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", try_from = "DictationSnapshotWire")]
pub struct DictationSnapshot {
    id: DictationSessionId,
    result_uri: DictationUri,
    pub status: DictationStatus,
    pub next_sequence: u32,
    pub max_duration_seconds: u32,
    pub transcript: Option<super::transcript::Transcript>,
}

impl DictationSnapshot {
    pub fn new(id: DictationSessionId) -> Self {
        veoveo_types::Checked::new(Self {
            id,
            result_uri: DictationUri::new(id),
            status: DictationStatus::Listening,
            next_sequence: 0,
            max_duration_seconds: super::transcript::MAX_DICTATION_SECONDS,
            transcript: None,
        })
        .unwrap_or_else(|_| unreachable!("dictation constructor binds the same identity"))
        .into_inner()
    }
    pub fn id(&self) -> DictationSessionId {
        self.id
    }
    pub fn result_uri(&self) -> DictationUri {
        self.result_uri
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DictationSnapshotWire {
    id: DictationSessionId,
    result_uri: DictationUri,
    status: DictationStatus,
    next_sequence: u32,
    max_duration_seconds: u32,
    transcript: Option<super::transcript::Transcript>,
}
impl TryFrom<DictationSnapshotWire> for DictationSnapshot {
    type Error = super::SpeechIdentityError;
    fn try_from(value: DictationSnapshotWire) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(Self {
            id: value.id,
            result_uri: value.result_uri,
            status: value.status,
            next_sequence: value.next_sequence,
            max_duration_seconds: value.max_duration_seconds,
            transcript: value.transcript,
        })
        .map(veoveo_types::Checked::into_inner)
    }
}
impl veoveo_types::Check for DictationSnapshot {
    type Error = super::SpeechIdentityError;
    fn check(&self) -> Result<(), Self::Error> {
        if self.id != self.result_uri.id() {
            return Err(super::SpeechIdentityError);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_decoder_rejects_a_different_session_address() {
        let snapshot = DictationSnapshot::new(DictationSessionId::new());
        let mut wire = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(
            serde_json::from_value::<DictationSnapshot>(wire.clone())
                .unwrap()
                .id(),
            snapshot.id()
        );
        wire["resultUri"] = DictationUri::new(DictationSessionId::new())
            .to_string()
            .into();
        assert!(serde_json::from_value::<DictationSnapshot>(wire).is_err());
    }
}
