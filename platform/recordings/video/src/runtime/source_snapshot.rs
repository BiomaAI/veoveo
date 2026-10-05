use veoveo_recording_reader::{
    RecordingReadSnapshot, RecordingReadSource, RecordingReadSourceKind,
};

use crate::contract::{
    RecordingSourceIdentity, RecordingSourceIdentityKind, RecordingSourceSnapshot,
};

impl TryFrom<&RecordingReadSnapshot> for RecordingSourceSnapshot {
    type Error = anyhow::Error;
    fn try_from(value: &RecordingReadSnapshot) -> Result<Self, Self::Error> {
        Ok(crate::contract::RecordingSourceSnapshotBuilder {
            recording_id: veoveo_recording_contract::RecordingId::try_from(
                value.recording_id.as_uuid(),
            )?,
            dataset_id: veoveo_recording_contract::RecordingDatasetId::try_from(
                value.dataset_id.as_uuid(),
            )?,
            captured_at: value.captured_at,
            sources: value
                .sources
                .iter()
                .map(RecordingSourceIdentity::try_from)
                .collect::<Result<_, _>>()?,
        }
        .build()?)
    }
}

impl TryFrom<&RecordingReadSource> for RecordingSourceIdentity {
    type Error = anyhow::Error;
    fn try_from(value: &RecordingReadSource) -> Result<Self, Self::Error> {
        Ok(crate::contract::RecordingSourceIdentityBuilder {
            layer_id: veoveo_recording_contract::RecordingLayerId::try_from(
                value.layer_id.as_uuid(),
            )?,
            layer_name: value.layer_name.clone(),
            layer_ordinal: value.layer_ordinal.map(u64::try_from).transpose()?,
            kind: value.kind.into(),
            part_sequence: value.part_sequence,
            byte_len: value.byte_len.try_into()?,
            sha256: value.sha256.clone(),
        }
        .build()?)
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
    use veoveo_recording_store::{RecordingDatasetId, RecordingId, RecordingLayerId};

    fn reader_snapshot() -> (RecordingReadSnapshot, RecordingSourceSnapshot) {
        let expected: RecordingSourceSnapshot =
            serde_json::from_str(include_str!("../../testdata/source-snapshot.json")).unwrap();
        let read = RecordingReadSnapshot {
            recording_id: RecordingId::from_uuid(expected.recording_id.as_uuid()),
            dataset_id: RecordingDatasetId::from_uuid(expected.dataset_id.as_uuid()),
            captured_at: expected.captured_at,
            sources: expected
                .sources
                .iter()
                .map(|source| RecordingReadSource {
                    layer_id: RecordingLayerId::from_uuid(source.layer_id.as_uuid()),
                    layer_name: source.layer_name.clone(),
                    layer_ordinal: source.layer_ordinal.map(|n| i64::try_from(n).unwrap()),
                    kind: match source.kind {
                        RecordingSourceIdentityKind::CommittedLayer => {
                            RecordingReadSourceKind::CommittedLayer
                        }
                        RecordingSourceIdentityKind::LiveIngestPart => {
                            RecordingReadSourceKind::LiveIngestPart
                        }
                    },
                    part_sequence: source.part_sequence,
                    byte_len: source.byte_len.get(),
                    sha256: source.sha256.clone(),
                    path: "/local/private/source.rrd".into(),
                })
                .collect(),
        };
        (read, expected)
    }

    #[test]
    fn reader_snapshot_conversion_preserves_identity_and_excludes_local_paths() {
        let (read, expected) = reader_snapshot();
        let actual = RecordingSourceSnapshot::try_from(&read).unwrap();
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
    #[test]
    fn reader_snapshot_conversion_rejects_invalid_store_facts() {
        let (mut read, _) = reader_snapshot();
        read.recording_id =
            RecordingId::from_uuid("01983da0-0000-4000-8000-000000000000".parse().unwrap());
        assert!(RecordingSourceSnapshot::try_from(&read).is_err());
        let (mut read, _) = reader_snapshot();
        read.sources[0].byte_len = 0;
        assert!(RecordingSourceSnapshot::try_from(&read).is_err());
        let (mut read, _) = reader_snapshot();
        read.sources[0].layer_ordinal = Some(-1);
        assert!(RecordingSourceSnapshot::try_from(&read).is_err());
        let (mut read, _) = reader_snapshot();
        read.sources[0].part_sequence = Some(1);
        assert!(RecordingSourceSnapshot::try_from(&read).is_err());
    }
}
