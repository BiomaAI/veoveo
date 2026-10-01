use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_mcp_contract::{
    ArtifactReadAuthority, GatewayInternalIdentity, PrincipalKind, TokenIssuer, TokenSubject,
};
use veoveo_platform_store::{
    PrincipalKind as StorePrincipalKind, RecordingDatasetId, RecordingId, RecordingLayerId,
    RecordingLayerKind, RecordingLayerState, RecordingState,
};
use veoveo_rrd::ingest_parts::{
    ingest_part_paths, ingest_part_sequence, ingest_segment_parts_directory,
};
use veoveo_rrd::segment::inspect_segment;
use veoveo_types::{DataLabelId, PrincipalId, Sha256Digest, TenantId};

use super::{MAX_LAYERS, RecordingReader};
use crate::access::{authorized_live_layer_path, record_uuid};
use crate::cache::CachedLayer;

/// Stable identity and clearance used to reopen a governed recording.
///
/// Submitted bearer credentials are absent. Materialization requires a current
/// Artifact caller or a bounded task-read capability with the same identity.
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

#[derive(Clone)]
pub struct RecordingReadLayer {
    pub layer_id: RecordingLayerId,
    pub layer_name: String,
    pub kind: RecordingLayerKind,
    pub ordinal: Option<i64>,
    pub state: RecordingLayerState,
    pub byte_len: u64,
    pub sha256: Option<Sha256Digest>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub path: PathBuf,
    cached: Option<CachedLayer>,
}

#[derive(Clone)]
pub struct RecordingReadPlan {
    pub recording_id: RecordingId,
    pub dataset_id: RecordingDatasetId,
    pub dataset_key: String,
    producer_application_id: String,
    producer_recording_key: String,
    pub state: RecordingState,
    pub classification: String,
    pub labels: Vec<String>,
    pub layers: Vec<RecordingReadLayer>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordingReadSourceKind {
    CommittedLayer,
    LiveIngestPart,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecordingReadSource {
    pub layer_id: RecordingLayerId,
    pub layer_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer_ordinal: Option<i64>,
    pub kind: RecordingReadSourceKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_sequence: Option<u64>,
    pub byte_len: u64,
    #[serde(with = "veoveo_types::sha256_hex")]
    pub sha256: Sha256Digest,
    #[serde(skip)]
    pub path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecordingReadSnapshot {
    pub recording_id: RecordingId,
    pub dataset_id: RecordingDatasetId,
    pub captured_at: DateTime<Utc>,
    pub sources: Vec<RecordingReadSource>,
}

pub struct MaterializedRecordingReadSnapshot {
    pub plan: RecordingReadPlan,
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
    pub fn stable_layer_paths(&self) -> Vec<PathBuf> {
        self.layers
            .iter()
            .filter(|layer| layer.state == RecordingLayerState::Committed)
            .map(|layer| layer.path.clone())
            .collect()
    }

    fn analysis_snapshot(&self) -> Result<RecordingReadSnapshot> {
        let mut sources = Vec::new();
        for layer in &self.layers {
            match layer.state {
                RecordingLayerState::Committed => {
                    ensure!(
                        layer.cached.is_some(),
                        "committed recording layer is not pinned in the verified cache"
                    );
                    let metadata = std::fs::metadata(&layer.path).with_context(|| {
                        format!("reading recording source {}", layer.path.display())
                    })?;
                    ensure!(
                        metadata.is_file() && metadata.len() == layer.byte_len,
                        "recording layer byte length no longer matches the catalog"
                    );
                    sources.push(RecordingReadSource {
                        layer_id: layer.layer_id,
                        layer_name: layer.layer_name.clone(),
                        layer_ordinal: layer.ordinal,
                        kind: RecordingReadSourceKind::CommittedLayer,
                        part_sequence: None,
                        byte_len: layer.byte_len,
                        sha256: layer
                            .sha256
                            .clone()
                            .context("committed recording layer is missing sha256")?,
                        path: layer.path.clone(),
                    });
                }
                RecordingLayerState::Writing if !layer.path.exists() => {
                    let parts_directory = ingest_segment_parts_directory(&layer.path);
                    for path in ingest_part_paths(&parts_directory)? {
                        let sequence = ingest_part_sequence(&path).with_context(|| {
                            format!("reading live ingest part sequence {}", path.display())
                        })?;
                        let inspection = inspect_segment(&path).with_context(|| {
                            format!("validating live ingest part {}", path.display())
                        })?;
                        ensure!(
                            inspection.application_id == self.producer_application_id
                                && inspection.recording_key == self.producer_recording_key,
                            "live ingest part changed its producer recording identity"
                        );
                        sources.push(RecordingReadSource {
                            layer_id: layer.layer_id,
                            layer_name: layer.layer_name.clone(),
                            layer_ordinal: layer.ordinal,
                            kind: RecordingReadSourceKind::LiveIngestPart,
                            part_sequence: Some(sequence),
                            byte_len: inspection.byte_len,
                            sha256: Sha256Digest::from_hex(inspection.sha256)?,
                            path,
                        });
                    }
                }
                RecordingLayerState::Writing
                | RecordingLayerState::Staged
                | RecordingLayerState::Failed => {}
            }
        }
        sources.sort_by_key(|source| {
            (
                source.layer_ordinal,
                source.layer_name.clone(),
                source.part_sequence.unwrap_or_default(),
            )
        });
        Ok(RecordingReadSnapshot {
            recording_id: self.recording_id,
            dataset_id: self.dataset_id,
            captured_at: Utc::now(),
            sources,
        })
    }

    fn materialize_analysis_snapshot(
        self,
        max_source_bytes: u64,
    ) -> Result<MaterializedRecordingReadSnapshot> {
        let snapshot = self.analysis_snapshot()?;
        let bytes = snapshot.sources.iter().try_fold(0_u64, |total, source| {
            total
                .checked_add(source.byte_len)
                .context("recording source byte count overflow")
        })?;
        ensure!(
            bytes <= max_source_bytes,
            "recording snapshot exceeds its source byte limit"
        );
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
                source.layer_id,
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
            let copied_inspection = inspect_segment(&destination)?;
            ensure!(
                copied_inspection.byte_len == source.byte_len
                    && copied_inspection.sha256 == source.sha256.hex(),
                "copied live ingest part does not match its captured identity"
            );
            // Committed layers already use these catalog IDs. Normalize only
            // the verified task-local copy so codec state and live samples join
            // into one Rerun store after a segment rolls over.
            veoveo_rrd::recording_layer::normalize_recording_layer(
                &destination,
                self.dataset_id.as_uuid(),
                self.recording_id.as_uuid(),
            )?;
            paths.push(destination);
        }
        Ok(MaterializedRecordingReadSnapshot {
            plan: self,
            snapshot,
            paths,
            _temporary: temporary,
        })
    }
}

impl RecordingReader {
    pub async fn materialize_analysis_snapshot(
        &self,
        authority: &RecordingReadAuthority,
        credential: ArtifactReadAuthority<'_>,
        recording_id: RecordingId,
        max_source_bytes: u64,
    ) -> Result<Option<MaterializedRecordingReadSnapshot>> {
        ensure!(
            max_source_bytes > 0,
            "recording source byte limit must be positive"
        );
        let limit = match credential {
            ArtifactReadAuthority::Caller(caller) => {
                ensure!(
                    *authority == RecordingReadAuthority::from_gateway(&caller.identity),
                    "recording authority differs from Artifact caller"
                );
                max_source_bytes
            }
            ArtifactReadAuthority::Task {
                capability,
                task_id,
            } => {
                let scope = self
                    .layer_cache
                    .artifacts()
                    .read_capability_scope(capability, task_id)
                    .await?;
                let delegated = RecordingReadAuthority::new(
                    scope.principal_id,
                    scope.principal_kind,
                    scope.issuer,
                    scope.subject,
                    Some(scope.tenant),
                    scope.data_labels,
                );
                ensure!(
                    *authority == delegated,
                    "recording authority differs from task read delegation"
                );
                max_source_bytes.min(scope.max_total_bytes.get())
            }
        };
        let Some(plan) = self.read_plan(authority, credential, recording_id).await? else {
            return Ok(None);
        };
        tokio::task::spawn_blocking(move || plan.materialize_analysis_snapshot(limit))
            .await
            .context("recording analysis snapshot worker panicked")?
            .map(Some)
    }

    async fn read_plan(
        &self,
        authority: &RecordingReadAuthority,
        credential: ArtifactReadAuthority<'_>,
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
            .visible_recording(
                &veoveo_platform_store::RecordingReadScope {
                    tenant_id: platform_identity.tenant_id,
                    data_labels: authority
                        .data_labels
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                },
                recording_id,
            )
            .await?
        else {
            return Ok(None);
        };
        let dataset_id =
            RecordingDatasetId::from_uuid(record_uuid(&recording.dataset, "recording_dataset")?);
        let dataset = self
            .store
            .recording_dataset(platform_identity.tenant_id, dataset_id)
            .await?
            .context("recording dataset is missing")?;
        let cache = &self.layer_cache;
        let dataset_uuid = record_uuid(&dataset.id, "recording_dataset")?;
        let recording_uuid = record_uuid(&recording.id, "recording")?;
        let catalog_layers = self
            .store
            .recording_layers(platform_identity.tenant_id, recording_id, MAX_LAYERS)
            .await?;
        let mut layers = Vec::with_capacity(catalog_layers.len());
        for layer in catalog_layers {
            let layer_id = RecordingLayerId::from_uuid(record_uuid(&layer.id, "recording_layer")?);
            let (path, cached, sha256) = match layer.state {
                RecordingLayerState::Committed => {
                    let artifact_id = veoveo_artifact_contract::ArtifactId::parse(
                        record_uuid(
                            layer
                                .artifact
                                .as_ref()
                                .context("committed layer has no Artifact occurrence")?,
                            "artifact_occurrence",
                        )?
                        .to_string(),
                    )?;
                    let byte_len = u64::try_from(layer.byte_len)
                        .context("committed layer has negative byte length")?;
                    let digest = Sha256Digest::from_hex(
                        layer
                            .sha256
                            .as_ref()
                            .context("committed layer has no digest")?,
                    )?;
                    let cached = cache
                        .materialize(
                            credential,
                            artifact_id,
                            byte_len,
                            &digest,
                            dataset_uuid,
                            recording_uuid,
                        )
                        .await?;
                    (cached.path().to_path_buf(), Some(cached), Some(digest))
                }
                RecordingLayerState::Writing => {
                    let relative = layer
                        .staging_path
                        .as_deref()
                        .context("writing layer has no staging path")?;
                    (
                        authorized_live_layer_path(&self.spool_root, relative)?,
                        None,
                        layer
                            .sha256
                            .as_ref()
                            .map(Sha256Digest::from_hex)
                            .transpose()?,
                    )
                }
                RecordingLayerState::Staged | RecordingLayerState::Failed => continue,
            };
            layers.push(RecordingReadLayer {
                layer_id,
                layer_name: layer.layer_name,
                kind: layer.kind,
                ordinal: layer.ordinal,
                state: layer.state,
                byte_len: u64::try_from(layer.byte_len)
                    .context("recording layer byte length is negative")?,
                sha256,
                started_at: layer.start_time,
                ended_at: layer.end_time,
                path,
                cached,
            });
        }
        Ok(Some(RecordingReadPlan {
            recording_id,
            dataset_id,
            dataset_key: dataset.dataset_key,
            producer_application_id: recording.application_id,
            producer_recording_key: recording.recording_key,
            state: recording.state,
            classification: recording.classification,
            labels: recording.labels,
            layers,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_samples_join_committed_codec_metadata_after_rollover() {
        use re_sdk::RecordingStreamBuilder;
        use re_sdk_types::{archetypes::VideoStream, components::VideoCodec};
        use veoveo_rrd::video_clip::{VideoClipRequest, extract_video_clip};

        let directory = tempfile::tempdir().unwrap();
        let dataset_id = RecordingDatasetId::new();
        let recording_id = RecordingId::new();
        let committed = directory.path().join("committed.rrd");
        let static_recording = RecordingStreamBuilder::new(
            re_sdk::ApplicationId::try_new(dataset_id.to_string()).unwrap(),
        )
        .recording_id(recording_id.to_string())
        .save(&committed)
        .unwrap();
        static_recording
            .log_static("/camera", &VideoStream::new(VideoCodec::H264))
            .unwrap();
        static_recording.flush_blocking().unwrap();
        drop(static_recording);

        let live_layer = directory.path().join("live.rrd");
        let parts = ingest_segment_parts_directory(&live_layer);
        std::fs::create_dir(&parts).unwrap();
        let part = parts.join("00000000000000000042.rrd");
        let live = RecordingStreamBuilder::new("producer-camera")
            .recording_id("producer-session")
            .save(&part)
            .unwrap();
        let video = include_bytes!("../../rrd/tests/fixtures/video.h264");
        let next_access_unit = (1..video.len().saturating_sub(4))
            .find(|index| video[*index..].starts_with(&[0, 0, 0, 1, 9]))
            .unwrap();
        live.set_duration_secs("sensor_time", 1.0);
        live.log(
            "/camera",
            &VideoStream::update_fields().with_sample(video[..next_access_unit].to_vec()),
        )
        .unwrap();
        live.flush_blocking().unwrap();
        drop(live);
        let original = inspect_segment(&part).unwrap();

        let plan = RecordingReadPlan {
            recording_id,
            dataset_id,
            dataset_key: "fixture".to_owned(),
            producer_application_id: "producer-camera".to_owned(),
            producer_recording_key: "producer-session".to_owned(),
            state: RecordingState::Ready,
            classification: "unclassified".to_owned(),
            labels: Vec::new(),
            layers: vec![RecordingReadLayer {
                layer_id: RecordingLayerId::new(),
                layer_name: "capture-1".to_owned(),
                kind: RecordingLayerKind::Capture,
                ordinal: Some(1),
                state: RecordingLayerState::Writing,
                byte_len: 0,
                sha256: None,
                started_at: None,
                ended_at: None,
                path: live_layer,
                cached: None,
            }],
        };
        let materialized = plan.materialize_analysis_snapshot(1_000_000).unwrap();
        let mut paths = vec![committed];
        paths.extend_from_slice(materialized.paths());
        let clip = extract_video_clip(
            &paths,
            &VideoClipRequest {
                application_id: dataset_id.to_string(),
                recording_key: recording_id.to_string(),
                entity_path: "/camera".into(),
                timeline: "sensor_time".into(),
                start_index: 1_000_000_000,
                end_index: 1_000_000_000,
                max_samples: 1,
                max_encoded_bytes: 1_000_000,
            },
        )
        .unwrap();
        assert_eq!(clip.samples.len(), 1);
        assert!(clip.samples[0].is_keyframe);
        let source = &materialized.snapshot.sources[0];
        assert_eq!(source.part_sequence, Some(42));
        assert_eq!(source.byte_len, original.byte_len);
        assert_eq!(source.sha256.hex(), original.sha256);
        assert_eq!(inspect_segment(&part).unwrap().sha256, original.sha256);
        let normalized = inspect_segment(&materialized.paths()[0]).unwrap();
        assert_eq!(normalized.application_id, dataset_id.to_string());
        assert_eq!(normalized.recording_key, recording_id.to_string());
    }

    #[test]
    fn committed_analysis_sources_require_a_verified_cache_lease() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("unverified.rrd");
        std::fs::write(&path, b"unverified archive").unwrap();
        let plan = RecordingReadPlan {
            recording_id: RecordingId::new(),
            dataset_id: RecordingDatasetId::new(),
            dataset_key: "test".to_owned(),
            producer_application_id: "test".to_owned(),
            producer_recording_key: "test".to_owned(),
            state: RecordingState::Ready,
            classification: "unclassified".to_owned(),
            labels: Vec::new(),
            layers: vec![RecordingReadLayer {
                layer_id: RecordingLayerId::new(),
                layer_name: "capture".to_owned(),
                kind: RecordingLayerKind::Capture,
                ordinal: Some(0),
                state: RecordingLayerState::Committed,
                byte_len: 18,
                sha256: Some(Sha256Digest::from_bytes([0; 32])),
                started_at: None,
                ended_at: None,
                path,
                cached: None,
            }],
        };
        let error = plan.analysis_snapshot().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("not pinned in the verified cache")
        );
    }
}
