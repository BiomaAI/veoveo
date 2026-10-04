use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_mcp_contract::{
    ArtifactReadAuthority, GatewayInternalIdentity, PrincipalKind, TokenIssuer, TokenSubject,
};
use veoveo_platform_store::{
    PrincipalKind as StorePrincipalKind, RecordingDatasetId, RecordingId, RecordingLayerId,
    RecordingLayerKind, RecordingLayerState, RecordingState, TenantId as StoreTenantId,
};
use veoveo_rrd::ingest_parts::{
    ingest_part_paths, ingest_part_sequence, ingest_segment_parts_directory,
};
use veoveo_rrd::segment::inspect_segment;
use veoveo_types::{DataLabelId, PrincipalId, Sha256Digest, TenantId};

use super::{MAX_LAYERS, RecordingReader};
use crate::access::{authorized_live_layer_path, confined_layer_path, record_uuid};
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
    tenant_id: StoreTenantId,
    spool_root: PathBuf,
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

    fn materialize_analysis_snapshot(
        self,
        max_source_bytes: u64,
    ) -> Result<MaterializedRecordingReadSnapshot> {
        let mut temporary = None;
        let mut sources = Vec::new();
        let mut paths = Vec::new();
        let mut bytes = 0_u64;
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
                    admit_source_bytes(&mut bytes, layer.byte_len, max_source_bytes)?;
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
                    paths.push(layer.path.clone());
                }
                RecordingLayerState::Writing | RecordingLayerState::Staged => {
                    let relative = layer
                        .path
                        .strip_prefix(&self.spool_root)?
                        .to_str()
                        .context("live layer path is not UTF-8")?;
                    let authorized = authorized_live_layer_path(&self.spool_root, relative)?;
                    let parts_directory = ingest_segment_parts_directory(&authorized);
                    for path in ingest_part_paths(&parts_directory)? {
                        let sequence = ingest_part_sequence(&path).with_context(|| {
                            format!("reading live ingest part sequence {}", path.display())
                        })?;
                        // Hold the inode through the copy. Hub may unlink the
                        // immutable acknowledged part after committing its layer.
                        let part = PinnedLivePart::open(&path)?;
                        admit_source_bytes(&mut bytes, part.byte_len, max_source_bytes)?;
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
                            "{:05}-{}-{sequence:020}.rrd",
                            sources.len(),
                            layer.layer_id
                        ));
                        let inspection = part.copy_to(&destination)?;
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
                        // Normalize only the task-local copy; the snapshot keeps
                        // the original producer bytes and digest.
                        veoveo_rrd::recording_layer::normalize_recording_layer(
                            &destination,
                            self.dataset_id.as_uuid(),
                            self.recording_id.as_uuid(),
                        )?;
                        paths.push(destination);
                    }
                }
                RecordingLayerState::Failed => {}
            }
        }
        // Catalog layers and each layer's parts are already ordered. Keep the
        // source receipts and materialized paths in the same order.
        let snapshot = RecordingReadSnapshot {
            recording_id: self.recording_id,
            dataset_id: self.dataset_id,
            captured_at: Utc::now(),
            sources,
        };
        Ok(MaterializedRecordingReadSnapshot {
            plan: self,
            snapshot,
            paths,
            _temporary: temporary,
        })
    }
}

fn admit_source_bytes(total: &mut u64, byte_len: u64, limit: u64) -> Result<()> {
    *total = total
        .checked_add(byte_len)
        .context("recording source byte count overflow")?;
    ensure!(
        *total <= limit,
        "recording snapshot exceeds its source byte limit"
    );
    Ok(())
}

struct PinnedLivePart {
    file: File,
    byte_len: u64,
}

impl PinnedLivePart {
    fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("opening live ingest part {}", path.display()))?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.len() > 0,
            "live ingest part is not a nonempty regular file"
        );
        Ok(Self {
            file,
            byte_len: metadata.len(),
        })
    }

    fn copy_to(mut self, destination: &Path) -> Result<veoveo_rrd::segment::SegmentInspection> {
        let mut output = File::create_new(destination)?;
        // Never copy more than the admitted length, even if a corrupt producer
        // modifies a part that should have been immutable after acknowledgement.
        let copied = std::io::copy(&mut (&mut self.file).take(self.byte_len), &mut output)
            .context("copying pinned live ingest part into the analysis snapshot")?;
        let mut extra = [0_u8; 1];
        ensure!(
            copied == self.byte_len && self.file.read(&mut extra)? == 0,
            "live ingest part changed while the analysis snapshot was captured"
        );
        let inspection = inspect_segment(destination)?;
        ensure!(
            inspection.byte_len == self.byte_len,
            "copied live ingest part changed length"
        );
        Ok(inspection)
    }
}

fn source_disappeared(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    })
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
        // A layer can commit while its parts are being enumerated, including
        // an apparently successful but empty/partial directory read. Recheck the
        // live catalog states before accepting any snapshot. Each retry obtains
        // a fresh authorized plan and committed layers through Artifact.
        for attempt in 0..3 {
            let Some(plan) = self.read_plan(authority, credential, recording_id).await? else {
                return Ok(None);
            };
            let tenant_id = plan.tenant_id;
            let live_layers = plan
                .layers
                .iter()
                .filter(|layer| {
                    matches!(
                        layer.state,
                        RecordingLayerState::Writing | RecordingLayerState::Staged
                    )
                })
                .map(|layer| layer.layer_id)
                .collect::<Vec<_>>();
            let result =
                tokio::task::spawn_blocking(move || plan.materialize_analysis_snapshot(limit))
                    .await
                    .context("recording analysis snapshot worker panicked")?;
            if let Err(error) = &result
                && !source_disappeared(error)
            {
                return result.map(Some);
            }
            let mut changed = false;
            for layer_id in live_layers {
                let current = self
                    .store
                    .recording_layer(tenant_id, layer_id)
                    .await?
                    .context("recording layer disappeared from its catalog during capture")?;
                if !matches!(
                    current.state,
                    RecordingLayerState::Writing | RecordingLayerState::Staged
                ) {
                    changed = true;
                }
            }
            if !changed {
                return result.map(Some);
            }
            ensure!(
                attempt < 2,
                "recording layers kept changing across three snapshot attempts"
            );
            // Drop all task-local files and cache pins before refreshing.
            drop(result);
        }
        unreachable!("last changed snapshot attempt returns an error")
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
                RecordingLayerState::Writing | RecordingLayerState::Staged => {
                    let relative = layer
                        .staging_path
                        .as_deref()
                        .context("uncommitted layer has no staging path")?;
                    (
                        confined_layer_path(&self.spool_root, relative)?,
                        None,
                        layer
                            .sha256
                            .as_ref()
                            .map(Sha256Digest::from_hex)
                            .transpose()?,
                    )
                }
                RecordingLayerState::Failed => continue,
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
            tenant_id: platform_identity.tenant_id,
            spool_root: self.spool_root.clone(),
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
        assert_publication_samples(RecordingLayerState::Writing, false);
    }

    #[test]
    fn materialized_writing_layer_keeps_acknowledged_samples_visible() {
        assert_publication_samples(RecordingLayerState::Writing, true);
    }

    #[test]
    fn staged_layer_keeps_acknowledged_samples_visible_until_commit() {
        assert_publication_samples(RecordingLayerState::Staged, true);
    }

    fn assert_publication_samples(state: RecordingLayerState, materialized_file: bool) {
        use re_sdk::RecordingStreamBuilder;
        use re_sdk_types::{archetypes::VideoStream, components::VideoCodec};
        use veoveo_rrd::video_clip::{VideoClipRequest, extract_video_clip};

        let video = include_bytes!("../../rrd/tests/fixtures/video.h264");
        let next_access_unit = (1..video.len().saturating_sub(4))
            .find(|index| video[*index..].starts_with(&[0, 0, 0, 1, 9]))
            .unwrap();
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
        static_recording.set_duration_secs("sensor_time", 0.5);
        static_recording
            .log(
                "/camera",
                &VideoStream::update_fields().with_sample(video[..next_access_unit].to_vec()),
            )
            .unwrap();
        static_recording.flush_blocking().unwrap();
        drop(static_recording);

        let live_layer = directory.path().canonicalize().unwrap().join("live.rrd");
        let parts = ingest_segment_parts_directory(&live_layer);
        std::fs::create_dir(&parts).unwrap();
        let part = parts.join("00000000000000000042.rrd");
        let live = RecordingStreamBuilder::new("producer-camera")
            .recording_id("producer-session")
            .save(&part)
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
        let pinned = PinnedLivePart::open(&part).unwrap();
        std::fs::remove_file(&part).unwrap();
        let copy = directory.path().join("pinned-copy.rrd");
        assert_eq!(pinned.copy_to(&copy).unwrap(), original);
        // The rest of this fixture qualifies live/committed codec composition.
        std::fs::rename(copy, &part).unwrap();
        if materialized_file {
            std::fs::copy(&part, &live_layer).unwrap();
            veoveo_rrd::recording_layer::normalize_recording_layer(
                &live_layer,
                dataset_id.as_uuid(),
                recording_id.as_uuid(),
            )
            .unwrap();
        }

        let plan = RecordingReadPlan {
            tenant_id: StoreTenantId::new(),
            spool_root: directory.path().canonicalize().unwrap(),
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
                state,
                byte_len: 0,
                sha256: None,
                started_at: None,
                ended_at: None,
                path: live_layer,
                cached: None,
            }],
        };
        let mut wrong_identity = plan.clone();
        wrong_identity.producer_recording_key = "unrelated-session".to_owned();
        let error = wrong_identity
            .materialize_analysis_snapshot(1_000_000)
            .err()
            .unwrap();
        assert!(error.to_string().contains("producer recording identity"));
        assert!(
            plan.clone()
                .materialize_analysis_snapshot(original.byte_len - 1)
                .is_err()
        );
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
    fn pinned_parts_reject_growth_without_copying_unadmitted_bytes() {
        use std::io::Write;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("part.rrd");
        std::fs::write(&source, b"original").unwrap();
        let pinned = PinnedLivePart::open(&source).unwrap();
        let mut writer = File::options().append(true).open(&source).unwrap();
        writer.write_all(b"unexpected growth").unwrap();
        let destination = directory.path().join("copy.rrd");
        let error = pinned.copy_to(&destination).unwrap_err();
        assert!(error.to_string().contains("changed while"));
        assert_eq!(std::fs::read(destination).unwrap(), b"original");
    }

    #[test]
    fn pinned_parts_reject_truncation() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("part.rrd");
        std::fs::write(&source, b"original").unwrap();
        let pinned = PinnedLivePart::open(&source).unwrap();
        File::options()
            .write(true)
            .open(&source)
            .unwrap()
            .set_len(2)
            .unwrap();
        let error = pinned
            .copy_to(&directory.path().join("copy.rrd"))
            .unwrap_err();
        assert!(error.to_string().contains("changed while"));
    }

    #[test]
    fn committed_analysis_sources_require_a_verified_cache_lease() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("unverified.rrd");
        std::fs::write(&path, b"unverified archive").unwrap();
        let plan = RecordingReadPlan {
            tenant_id: StoreTenantId::new(),
            spool_root: directory.path().canonicalize().unwrap(),
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
        let error = plan.materialize_analysis_snapshot(1_000_000).err().unwrap();
        assert!(
            error
                .to_string()
                .contains("not pinned in the verified cache")
        );
    }
}
