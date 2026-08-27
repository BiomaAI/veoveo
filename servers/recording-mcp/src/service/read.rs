use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde::Serialize;
use veoveo_mcp_contract::{
    DataLabelId, GatewayInternalIdentity, PlaneCaller, PrincipalId, PrincipalKind, TenantId,
    TokenIssuer, TokenSubject,
};
use veoveo_platform_store::{
    PrincipalKind as StorePrincipalKind, RecordingDatasetId, RecordingId, RecordingLayerId,
    RecordingLayerKind, RecordingLayerState, RecordingState,
};
use veoveo_recording_hub::{
    ingest_part_paths, ingest_part_sequence, ingest_segment_parts_directory, inspect_segment,
};

use super::{
    MAX_LAYERS, RecordingService, authorized_live_layer_path, labels_visible, record_uuid,
};
use crate::layer_cache::CachedLayer;

/// Stable identity and clearance used to reopen a governed recording.
///
/// Bearer credentials are deliberately absent. Callers that need an Artifact
/// download provide a fresh, short-lived Artifact-plane caller separately.
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
    pub sha256: Option<String>,
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
    pub application_id: String,
    pub recording_key: String,
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
    pub sha256: String,
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
                            inspection.application_id == self.application_id
                                && inspection.recording_key == self.recording_key,
                            "live ingest part changed its producer recording identity"
                        );
                        sources.push(RecordingReadSource {
                            layer_id: layer.layer_id,
                            layer_name: layer.layer_name.clone(),
                            layer_ordinal: layer.ordinal,
                            kind: RecordingReadSourceKind::LiveIngestPart,
                            part_sequence: Some(sequence),
                            byte_len: inspection.byte_len,
                            sha256: inspection.sha256,
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

    fn materialize_analysis_snapshot(self) -> Result<MaterializedRecordingReadSnapshot> {
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
                    && copied_inspection.sha256 == source.sha256,
                "copied live ingest part does not match its captured identity"
            );
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

impl RecordingService {
    pub async fn materialize_analysis_snapshot(
        &self,
        _authority: &RecordingReadAuthority,
        _recording_id: RecordingId,
    ) -> Result<Option<MaterializedRecordingReadSnapshot>> {
        anyhow::bail!("Artifact-backed recording reads require a fresh Artifact-read credential")
    }

    pub async fn materialize_analysis_snapshot_with_caller(
        &self,
        authority: &RecordingReadAuthority,
        caller: &PlaneCaller,
        recording_id: RecordingId,
    ) -> Result<Option<MaterializedRecordingReadSnapshot>> {
        let Some(plan) = self.read_plan(authority, caller, recording_id).await? else {
            return Ok(None);
        };
        tokio::task::spawn_blocking(move || plan.materialize_analysis_snapshot())
            .await
            .context("recording analysis snapshot worker panicked")?
            .map(Some)
    }

    pub async fn read_plan(
        &self,
        authority: &RecordingReadAuthority,
        caller: &PlaneCaller,
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
        let dataset_id =
            RecordingDatasetId::from_uuid(record_uuid(&recording.dataset, "recording_dataset")?);
        let dataset = self
            .store
            .recording_dataset(platform_identity.tenant_id, dataset_id)
            .await?
            .context("recording dataset is missing")?;
        let cache = self
            .layer_cache
            .as_ref()
            .context("recording layer cache is not configured")?;
        let dataset_uuid = record_uuid(&dataset.id, "recording_dataset")?;
        let recording_uuid = record_uuid(&recording.id, "recording")?;
        let catalog_layers = self
            .store
            .recording_layers(platform_identity.tenant_id, recording_id, MAX_LAYERS)
            .await?;
        let mut layers = Vec::with_capacity(catalog_layers.len());
        for layer in catalog_layers {
            let layer_id = RecordingLayerId::from_uuid(record_uuid(&layer.id, "recording_layer")?);
            let (path, cached) = match layer.state {
                RecordingLayerState::Committed => {
                    let artifact_id = veoveo_mcp_contract::ArtifactId::parse(
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
                    let sha256 = layer
                        .sha256
                        .as_deref()
                        .context("committed layer has no digest")?;
                    let cached = cache
                        .materialize(
                            caller,
                            artifact_id,
                            byte_len,
                            sha256,
                            dataset_uuid,
                            recording_uuid,
                        )
                        .await?;
                    (cached.path().to_path_buf(), Some(cached))
                }
                RecordingLayerState::Writing => {
                    let relative = layer
                        .staging_path
                        .as_deref()
                        .context("writing layer has no staging path")?;
                    (
                        authorized_live_layer_path(&self.spool_root, relative)?,
                        None,
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
                sha256: layer.sha256,
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
            application_id: recording.application_id,
            recording_key: recording.recording_key,
            state: recording.state,
            classification: recording.classification,
            labels: recording.labels,
            layers,
        }))
    }
}
