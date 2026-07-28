use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_mcp_contract::{
    DataLabelId, GatewayInternalIdentity, PrincipalId, PrincipalKind, TenantId, TokenIssuer,
    TokenSubject,
};
use veoveo_platform_store::{
    PrincipalKind as StorePrincipalKind, RecordingId, RecordingState, SegmentId, SegmentState,
};
use veoveo_recording_hub::{
    ingest_part_paths, ingest_part_sequence, ingest_segment_parts_directory, inspect_segment,
};

use super::{MAX_SEGMENTS, RecordingService, labels_visible, record_uuid};

/// Stable identity and clearance needed to reopen an authorized recording.
///
/// Unlike a gateway assertion this value has no bearer or expiry. It can be
/// reconstructed from a durable task owner after restart, while the recording
/// policy is evaluated again against current catalog state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingReadAuthority {
    principal_id: PrincipalId,
    principal_kind: PrincipalKind,
    issuer: TokenIssuer,
    subject: TokenSubject,
    tenant: Option<TenantId>,
    data_labels: BTreeSet<DataLabelId>,
}

impl RecordingReadAuthority {
    pub fn from_gateway(identity: &GatewayInternalIdentity) -> Self {
        Self {
            principal_id: identity.actor.id.clone(),
            principal_kind: identity.actor.kind,
            issuer: identity.actor.issuer.clone(),
            subject: identity.actor.subject.clone(),
            tenant: identity.actor.tenant.clone(),
            data_labels: identity.actor.data_labels.clone(),
        }
    }

    pub fn new(
        principal_id: PrincipalId,
        principal_kind: PrincipalKind,
        issuer: TokenIssuer,
        subject: TokenSubject,
        tenant: Option<TenantId>,
        data_labels: BTreeSet<DataLabelId>,
    ) -> Self {
        Self {
            principal_id,
            principal_kind,
            issuer,
            subject,
            tenant,
            data_labels,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingReadSegment {
    pub segment_id: SegmentId,
    pub ordinal: i64,
    pub state: SegmentState,
    pub byte_len: u64,
    pub sha256: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordingReadPlan {
    pub recording_id: RecordingId,
    pub dataset: String,
    pub application_id: String,
    pub recording_key: String,
    pub state: RecordingState,
    pub classification: String,
    pub labels: Vec<String>,
    pub segments: Vec<RecordingReadSegment>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingReadSourceKind {
    FrozenSegment,
    SealedSegment,
    LiveIngestPart,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecordingReadSource {
    pub segment_id: SegmentId,
    pub segment_ordinal: i64,
    pub kind: RecordingReadSourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_sequence: Option<u64>,
    pub byte_len: u64,
    pub sha256: String,
    #[serde(skip)]
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecordingReadSnapshot {
    pub recording_id: RecordingId,
    pub captured_at: DateTime<Utc>,
    pub sources: Vec<RecordingReadSource>,
}

#[derive(Debug)]
pub struct MaterializedRecordingReadSnapshot {
    pub snapshot: RecordingReadSnapshot,
    paths: Vec<PathBuf>,
    _temporary: Option<tempfile::TempDir>,
}

impl MaterializedRecordingReadSnapshot {
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }
}

impl RecordingReadPlan {
    /// Frozen and sealed paths retained for callers that require archival
    /// sources. Live analysis should use `analysis_snapshot`.
    pub fn stable_segment_paths(&self) -> Vec<PathBuf> {
        self.segments
            .iter()
            .filter(|segment| matches!(segment.state, SegmentState::Frozen | SegmentState::Sealed))
            .map(|segment| segment.path.clone())
            .collect()
    }

    /// Capture the immutable recording sources visible at this instant.
    ///
    /// Frozen and sealed segments remain direct sources. An authenticated
    /// writing segment contributes only complete, acknowledged ingest parts;
    /// the mutable native-writer file is never admitted.
    pub fn analysis_snapshot(&self) -> Result<RecordingReadSnapshot> {
        let mut sources = Vec::new();
        for segment in &self.segments {
            match segment.state {
                SegmentState::Frozen | SegmentState::Sealed => {
                    let metadata = std::fs::metadata(&segment.path).with_context(|| {
                        format!("reading recording source {}", segment.path.display())
                    })?;
                    ensure!(
                        metadata.is_file() && metadata.len() == segment.byte_len,
                        "recording segment byte length no longer matches the catalog"
                    );
                    sources.push(RecordingReadSource {
                        segment_id: segment.segment_id,
                        segment_ordinal: segment.ordinal,
                        kind: if segment.state == SegmentState::Frozen {
                            RecordingReadSourceKind::FrozenSegment
                        } else {
                            RecordingReadSourceKind::SealedSegment
                        },
                        part_sequence: None,
                        byte_len: segment.byte_len,
                        sha256: segment
                            .sha256
                            .clone()
                            .context("immutable recording segment is missing sha256")?,
                        path: segment.path.clone(),
                    });
                }
                SegmentState::Writing if !segment.path.exists() => {
                    let parts_directory = ingest_segment_parts_directory(&segment.path);
                    for path in ingest_part_paths(&parts_directory)? {
                        let sequence = ingest_part_sequence(&path).with_context(|| {
                            format!("reading live ingest part sequence {}", path.display())
                        })?;
                        let inspection = inspect_segment(&path).with_context(|| {
                            format!("validating live ingest part {}", path.display())
                        })?;
                        ensure!(
                            inspection.application_id == self.application_id
                                && inspection.recording_key == self.recording_key,
                            "live ingest part changed its logical recording identity"
                        );
                        sources.push(RecordingReadSource {
                            segment_id: segment.segment_id,
                            segment_ordinal: segment.ordinal,
                            kind: RecordingReadSourceKind::LiveIngestPart,
                            part_sequence: Some(sequence),
                            byte_len: inspection.byte_len,
                            sha256: inspection.sha256,
                            path,
                        });
                    }
                }
                SegmentState::Writing | SegmentState::Failed => {}
            }
        }
        sources.sort_by_key(|source| {
            (
                source.segment_ordinal,
                source.part_sequence.unwrap_or_default(),
            )
        });
        Ok(RecordingReadSnapshot {
            recording_id: self.recording_id,
            captured_at: Utc::now(),
            sources,
        })
    }

    /// Copy live ingest parts into a task-local snapshot before extraction.
    ///
    /// Hub may replace a writing parts directory with its frozen shard at any
    /// time. These bounded copies retain the exact acknowledged bytes selected
    /// for one task while immutable archive sources remain zero-copy.
    pub fn materialize_analysis_snapshot(&self) -> Result<MaterializedRecordingReadSnapshot> {
        let snapshot = self.analysis_snapshot()?;
        let mut temporary = None;
        let mut paths = Vec::with_capacity(snapshot.sources.len());
        for (index, source) in snapshot.sources.iter().enumerate() {
            if source.kind != RecordingReadSourceKind::LiveIngestPart {
                paths.push(source.path.clone());
                continue;
            }
            let directory = match &temporary {
                Some(directory) => directory,
                None => temporary.insert(
                    tempfile::Builder::new()
                        .prefix("veoveo-recording-snapshot-")
                        .tempdir()
                        .context("creating live recording snapshot workspace")?,
                ),
            };
            let destination = directory.path().join(format!(
                "{index:05}-{}-{:020}.rrd",
                source.segment_id,
                source
                    .part_sequence
                    .context("live ingest source is missing its part sequence")?
            ));
            let copied = std::fs::copy(&source.path, &destination).with_context(|| {
                format!(
                    "copying live ingest part {} into the analysis snapshot",
                    source.path.display()
                )
            })?;
            ensure!(
                copied == source.byte_len,
                "live ingest part changed while the analysis snapshot was captured"
            );
            let copied_inspection = inspect_segment(&destination).with_context(|| {
                format!(
                    "validating copied live ingest part {}",
                    destination.display()
                )
            })?;
            ensure!(
                copied_inspection.byte_len == source.byte_len
                    && copied_inspection.sha256 == source.sha256,
                "copied live ingest part does not match its captured identity"
            );
            paths.push(destination);
        }
        Ok(MaterializedRecordingReadSnapshot {
            snapshot,
            paths,
            _temporary: temporary,
        })
    }
}

impl RecordingService {
    /// Resolve one governed recording into a local, typed read plan.
    ///
    /// Callers persist the recording identity, not these filesystem paths, and
    /// call this method again when a resumable task is reclaimed.
    pub async fn read_plan(
        &self,
        authority: &RecordingReadAuthority,
        recording_id: RecordingId,
    ) -> Result<Option<RecordingReadPlan>> {
        let tenant_key = authority
            .tenant
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "installation".to_owned());
        let platform_identity = self
            .store
            .ensure_identity(
                &tenant_key,
                authority.principal_id.as_str(),
                authority.issuer.as_str(),
                authority.subject.as_str(),
                match authority.principal_kind {
                    PrincipalKind::User => StorePrincipalKind::User,
                    PrincipalKind::Service => StorePrincipalKind::Service,
                },
            )
            .await?;
        let Some(recording) = self
            .store
            .recording(platform_identity.tenant_id, recording_id)
            .await?
        else {
            return Ok(None);
        };
        if !labels_visible(
            &recording,
            authority.data_labels.iter().map(|label| label.as_str()),
        ) {
            return Ok(None);
        }
        let catalog_segments = self
            .store
            .recording_segments(platform_identity.tenant_id, recording_id, MAX_SEGMENTS)
            .await?;
        let mut segments = Vec::with_capacity(catalog_segments.len());
        for segment in catalog_segments {
            ensure!(
                segment.byte_len >= 0,
                "recording segment has negative byte_len"
            );
            let path = match segment.state {
                SegmentState::Writing => self.live_segment_path(&segment.relative_path),
                SegmentState::Frozen | SegmentState::Sealed => {
                    self.segment_path(&segment.relative_path)
                }
                SegmentState::Failed => {
                    super::confined_segment_path(&self.spool_root, &segment.relative_path)
                }
            };
            let path = match path {
                Ok(path) => path,
                Err(error)
                    if recording.state == RecordingState::Live
                        && error_chain_contains_not_found(&error) =>
                {
                    continue;
                }
                Err(error) => return Err(error),
            };
            segments.push(RecordingReadSegment {
                segment_id: SegmentId::from_uuid(record_uuid(&segment.id, "segment")?),
                ordinal: segment.ordinal,
                state: segment.state,
                byte_len: u64::try_from(segment.byte_len)
                    .context("recording segment byte_len exceeds u64")?,
                sha256: segment.sha256,
                started_at: segment.start_time,
                ended_at: segment.end_time,
                path,
            });
        }
        Ok(Some(RecordingReadPlan {
            recording_id,
            dataset: recording.dataset,
            application_id: recording.application_id,
            recording_key: recording.recording_key,
            state: recording.state,
            classification: recording.classification,
            labels: recording.labels,
            segments,
        }))
    }
}

fn error_chain_contains_not_found(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    })
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use anyhow::Context as _;
    use re_build_info::CrateVersion;
    use re_log_encoding::{EncodingOptions, rrd::Encoder};
    use re_log_types::LogMsg;
    use re_sdk::RecordingStreamBuilder;
    use re_sdk_types::archetypes::Scalars;
    use veoveo_recording_hub::query_segments_in_range;

    use super::*;

    fn encoded_rrd(application_id: &str, recording_key: &str, value: f64) -> Vec<u8> {
        let (recording, storage) = RecordingStreamBuilder::new(application_id)
            .recording_id(recording_key)
            .memory()
            .unwrap();
        recording.set_duration_secs("sensor_time", value);
        recording
            .log("/sensor/value", &Scalars::single(value))
            .unwrap();
        let mut encoder = Encoder::new_eager(
            CrateVersion::LOCAL,
            EncodingOptions::PROTOBUF_COMPRESSED,
            Vec::new(),
        )
        .unwrap();
        for message in storage.take() {
            encoder.append(&message).unwrap();
        }
        encoder.finish().unwrap();
        let bytes = encoder.into_inner().unwrap();
        re_log_encoding::Decoder::<LogMsg>::decode_eager(Cursor::new(&bytes))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        bytes
    }

    #[test]
    fn identifies_not_found_through_context() {
        let error = std::fs::File::open("/a-path-that-does-not-exist")
            .context("opening projected recording segment")
            .unwrap_err();
        assert!(error_chain_contains_not_found(&error));
        assert!(!error_chain_contains_not_found(&anyhow::anyhow!(
            "catalog authorization failed"
        )));
    }

    #[test]
    fn analysis_snapshot_keeps_acknowledged_live_parts_across_rollover() {
        let directory = tempfile::tempdir().unwrap();
        let application_id = "live-perception";
        let recording_key = "flight-a";
        let frozen_path = directory.path().join("frozen.rrd");
        std::fs::write(
            &frozen_path,
            encoded_rrd(application_id, recording_key, 1.0),
        )
        .unwrap();
        let frozen_inspection = inspect_segment(&frozen_path).unwrap();

        let live_path = directory
            .path()
            .join(format!("flight.ingest-{}-s1.rrd", uuid::Uuid::now_v7()));
        let parts_directory = ingest_segment_parts_directory(&live_path);
        std::fs::create_dir(&parts_directory).unwrap();
        for (sequence, value) in [(7_u64, 2.0), (8, 3.0)] {
            std::fs::write(
                parts_directory.join(format!("{sequence:020}.rrd")),
                encoded_rrd(application_id, recording_key, value),
            )
            .unwrap();
        }

        let recording_id = RecordingId::from_uuid(uuid::Uuid::now_v7());
        let plan = RecordingReadPlan {
            recording_id,
            dataset: "world".to_owned(),
            application_id: application_id.to_owned(),
            recording_key: recording_key.to_owned(),
            state: RecordingState::Live,
            classification: "unclassified".to_owned(),
            labels: Vec::new(),
            segments: vec![
                RecordingReadSegment {
                    segment_id: SegmentId::from_uuid(uuid::Uuid::now_v7()),
                    ordinal: 0,
                    state: SegmentState::Frozen,
                    byte_len: frozen_inspection.byte_len,
                    sha256: Some(frozen_inspection.sha256),
                    started_at: None,
                    ended_at: None,
                    path: frozen_path,
                },
                RecordingReadSegment {
                    segment_id: SegmentId::from_uuid(uuid::Uuid::now_v7()),
                    ordinal: 1,
                    state: SegmentState::Writing,
                    byte_len: 0,
                    sha256: None,
                    started_at: None,
                    ended_at: None,
                    path: live_path,
                },
            ],
        };

        let materialized = plan.materialize_analysis_snapshot().unwrap();
        assert_eq!(materialized.snapshot.recording_id, recording_id);
        assert_eq!(materialized.snapshot.sources.len(), 3);
        assert_eq!(
            materialized.snapshot.sources[0].kind,
            RecordingReadSourceKind::FrozenSegment
        );
        assert_eq!(
            materialized.snapshot.sources[1].kind,
            RecordingReadSourceKind::LiveIngestPart
        );
        assert_eq!(materialized.snapshot.sources[1].part_sequence, Some(7));
        assert_eq!(materialized.snapshot.sources[2].part_sequence, Some(8));

        let public_snapshot = serde_json::to_string(&materialized.snapshot).unwrap();
        assert!(!public_snapshot.contains(directory.path().to_str().unwrap()));
        assert!(!public_snapshot.contains("\"path\""));

        std::fs::remove_dir_all(parts_directory).unwrap();
        assert!(materialized.paths()[1].is_file());
        assert!(materialized.paths()[2].is_file());
        let queried =
            query_segments_in_range(materialized.paths(), "/**", "sensor_time", 10, None).unwrap();
        assert_eq!(
            queried
                .rows_by_recording
                .get(recording_key)
                .copied()
                .unwrap_or_default(),
            3
        );
    }
}
