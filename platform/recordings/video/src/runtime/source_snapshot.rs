use veoveo_recording_reader::{
    RecordingReadSnapshot, RecordingReadSource, RecordingReadSourceKind,
};

use crate::contract::{
    RecordingSourceIdentity, RecordingSourceIdentityKind, RecordingSourceSnapshot,
};

impl From<&RecordingReadSnapshot> for RecordingSourceSnapshot {
    fn from(value: &RecordingReadSnapshot) -> Self {
        Self {
            recording_id: value.recording_id.to_string(),
            dataset_id: value.dataset_id.to_string(),
            captured_at: value.captured_at,
            sources: value
                .sources
                .iter()
                .map(RecordingSourceIdentity::from)
                .collect(),
        }
    }
}

impl From<&RecordingReadSource> for RecordingSourceIdentity {
    fn from(value: &RecordingReadSource) -> Self {
        Self {
            layer_id: value.layer_id.to_string(),
            layer_name: value.layer_name.clone(),
            layer_ordinal: value.layer_ordinal,
            kind: value.kind.into(),
            part_sequence: value.part_sequence,
            byte_len: value.byte_len,
            sha256: value.sha256.clone(),
        }
    }
}

impl From<RecordingReadSourceKind> for RecordingSourceIdentityKind {
    fn from(value: RecordingReadSourceKind) -> Self {
        match value {
            RecordingReadSourceKind::CommittedLayer => Self::CommittedLayer,
            RecordingReadSourceKind::LiveIngestPart => Self::LiveIngestPart,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_platform_store::{RecordingDatasetId, RecordingId, RecordingLayerId};

    #[test]
    fn reader_snapshot_conversion_preserves_identity_and_excludes_local_paths() {
        let expected: RecordingSourceSnapshot =
            serde_json::from_str(include_str!("../../testdata/source-snapshot.json")).unwrap();
        let read = RecordingReadSnapshot {
            recording_id: RecordingId::from_uuid(expected.recording_id.parse().unwrap()),
            dataset_id: RecordingDatasetId::from_uuid(expected.dataset_id.parse().unwrap()),
            captured_at: expected.captured_at,
            sources: expected
                .sources
                .iter()
                .map(|source| RecordingReadSource {
                    layer_id: RecordingLayerId::from_uuid(source.layer_id.parse().unwrap()),
                    layer_name: source.layer_name.clone(),
                    layer_ordinal: source.layer_ordinal,
                    kind: match source.kind {
                        RecordingSourceIdentityKind::CommittedLayer => {
                            RecordingReadSourceKind::CommittedLayer
                        }
                        RecordingSourceIdentityKind::LiveIngestPart => {
                            RecordingReadSourceKind::LiveIngestPart
                        }
                    },
                    part_sequence: source.part_sequence,
                    byte_len: source.byte_len,
                    sha256: source.sha256.clone(),
                    path: "/local/private/source.rrd".into(),
                })
                .collect(),
        };
        let actual = RecordingSourceSnapshot::from(&read);
        assert_eq!(actual, expected);
        assert_eq!(
            actual.digest_sha256().unwrap(),
            expected.digest_sha256().unwrap()
        );
        assert!(
            !serde_json::to_string(&actual)
                .unwrap()
                .contains("/local/private")
        );
    }
}
