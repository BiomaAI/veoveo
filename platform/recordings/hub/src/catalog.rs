//! SurrealDB-backed catalog projection for immutable recording layers.
//!
//! The filesystem remains the byte authority while a recording is open. This
//! module gives each recording and segment a typed installation identity and
//! makes crash recovery explicit by reconciling footer-less files on startup.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use chrono::{DateTime, NaiveDate, Utc};
use re_dataframe::{ChunkStoreConfig, QueryEngine};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    DataLabelId, PrincipalId as ContractPrincipalId, PutArtifactRequest, TokenIssuer, TokenSubject,
};
use veoveo_platform_store::{
    InvocationAuthorityRecord, PlatformIdentity, PlatformStore, PrincipalKind, RecordId,
    RecordIdKey, RecordingBlueprintCommit, RecordingBlueprintDraft, RecordingDatasetDraft,
    RecordingDatasetId, RecordingDraft, RecordingId, RecordingLayerDraft, RecordingLayerId,
    RecordingLayerRecord, RecordingLayerState, RecordingState,
};

use crate::config::DatasetName;
use crate::governance::{governed_classification, governed_labels};
use crate::ingest::is_authenticated_ingest_path;
use crate::layer_files::{RecordingLayerFileScope, collect_recording_layer_files};
use crate::publication::GatewayLayerPublisher;
use crate::spool::{FrozenSegment, OpenedSegment, PublishedBlueprint, SegmentCatalog, SegmentKey};

#[derive(Clone, Debug)]
pub struct CatalogPolicy {
    pub tenant_key: String,
    pub work_context_key: String,
    pub owner_key: String,
    pub owner_issuer: String,
    pub owner_subject: String,
    pub classification: String,
    pub labels: Vec<String>,
    pub maximum_blueprint_revisions: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SegmentInspection {
    pub application_id: String,
    pub recording_key: String,
    pub byte_len: u64,
    pub sha256: String,
}

#[derive(Clone)]
pub struct PlatformCatalog {
    store: PlatformStore,
    identity: PlatformIdentity,
    authority: InvocationAuthorityRecord,
    spool_root: PathBuf,
    policy: CatalogPolicy,
    runtime: tokio::runtime::Handle,
    publisher: GatewayLayerPublisher,
}

impl PlatformCatalog {
    pub async fn new(
        store: PlatformStore,
        spool_root: PathBuf,
        policy: CatalogPolicy,
        runtime: tokio::runtime::Handle,
        publisher: GatewayLayerPublisher,
    ) -> Result<Self> {
        ensure!(spool_root.is_absolute(), "spool root must be absolute");
        std::fs::create_dir_all(&spool_root)
            .with_context(|| format!("creating spool root {}", spool_root.display()))?;
        let spool_root = spool_root
            .canonicalize()
            .with_context(|| format!("canonicalizing spool root {}", spool_root.display()))?;
        let identity = store
            .ensure_identity(
                &policy.tenant_key,
                &policy.owner_key,
                &policy.owner_issuer,
                &policy.owner_subject,
                PrincipalKind::Service,
            )
            .await?;
        let principal_id = ContractPrincipalId::new(format!(
            "{}#{}",
            TokenIssuer::new(&policy.owner_issuer)?,
            TokenSubject::new(&policy.owner_subject)?
        ))?;
        let context = store
            .work_context_by_key(identity.tenant_id, &policy.work_context_key)
            .await?
            .with_context(|| {
                format!(
                    "work context `{}` is not materialized for tenant `{}`",
                    policy.work_context_key, policy.tenant_key
                )
            })?;
        let membership = context
            .membership_for_principal(principal_id.as_str())
            .with_context(|| {
                format!(
                    "principal `{principal_id}` is not a member of work context `{}`",
                    policy.work_context_key
                )
            })?;
        let authority = context.automated_authority(membership);
        Ok(Self {
            store,
            identity,
            authority,
            spool_root,
            policy,
            runtime,
            publisher,
        })
    }

    pub fn identity(&self) -> &PlatformIdentity {
        &self.identity
    }

    pub async fn reconcile(&self) -> Result<usize> {
        let mut reconciled = 0;
        let mut recovered_recordings = BTreeSet::new();
        for path in
            collect_recording_layer_files(&self.spool_root, RecordingLayerFileScope::Committed)?
        {
            if is_authenticated_ingest_path(&path) {
                continue;
            }
            let relative = relative_path(&self.spool_root, &path)?;
            let layer = if let Some(layer) = self
                .store
                .recording_layer_by_staging_path(self.identity.tenant_id, &relative)
                .await?
            {
                layer
            } else {
                let inspection = inspect_segment(&path)?;
                match self.canonical_recovery_layer(&path, &inspection).await? {
                    Some(layer) => layer,
                    None => {
                        let key = segment_key_from_path(&self.spool_root, &path, &inspection)?;
                        self.register_opened(&OpenedSegment {
                            key,
                            path: path.clone(),
                            started_at: Utc::now(),
                        })
                        .await?
                    }
                }
            };
            recovered_recordings.insert(recording_id(&layer.recording)?);
            self.publish_layer(layer, &path, None).await?;
            reconciled += 1;
        }
        for recording_id in recovered_recordings {
            let recording = self
                .store
                .recording(self.identity.tenant_id, recording_id)
                .await?
                .context("reconciled segment has no recording catalog entry")?;
            if recording.state == RecordingState::Live {
                self.store
                    .interrupt_recording(
                        &self.identity,
                        recording_id,
                        recording.last_data_at,
                        "capture process stopped before recording completion",
                    )
                    .await?;
            }
        }
        Ok(reconciled)
    }

    async fn canonical_recovery_layer(
        &self,
        path: &Path,
        inspection: &SegmentInspection,
    ) -> Result<Option<RecordingLayerRecord>> {
        let Ok(dataset_uuid) = uuid::Uuid::parse_str(&inspection.application_id) else {
            return Ok(None);
        };
        let Ok(recording_uuid) = uuid::Uuid::parse_str(&inspection.recording_key) else {
            return Ok(None);
        };
        if dataset_uuid.get_version_num() != 7 || recording_uuid.get_version_num() != 7 {
            return Ok(None);
        }
        let recording_id = RecordingId::from_uuid(recording_uuid);
        let Some(recording) = self
            .store
            .recording(self.identity.tenant_id, recording_id)
            .await?
        else {
            return Ok(None);
        };
        if recording.dataset != RecordingDatasetId::from_uuid(dataset_uuid).record_id() {
            return Ok(None);
        }
        let layer_name = format!("capture-{:020}", direct_capture_ordinal(path)?);
        self.store
            .recording_layer_by_name(self.identity.tenant_id, recording_id, &layer_name)
            .await
            .map_err(Into::into)
    }

    async fn register_opened(&self, segment: &OpenedSegment) -> Result<RecordingLayerRecord> {
        let dataset = self
            .store
            .ensure_recording_dataset(RecordingDatasetDraft::installation_default(
                self.identity.clone(),
                segment.key.dataset.as_str(),
            ))
            .await?;
        let dataset_id = dataset_id(&dataset.id)?;
        let recording = self
            .store
            .create_recording(RecordingDraft {
                identity: self.identity.clone(),
                authority: self.authority.clone(),
                dataset_id,
                application_id: segment.key.application_id.clone(),
                recording_key: segment.key.recording.clone(),
                classification: governed_classification(
                    &self.authority,
                    &self.policy.classification,
                ),
                labels: governed_labels(&self.authority, &self.policy.labels),
                metadata: BTreeMap::from([
                    ("source".to_owned(), serde_json::json!("recording-hub")),
                    (
                        "dataset".to_owned(),
                        serde_json::json!(segment.key.dataset.as_str()),
                    ),
                ]),
                started_at: segment.started_at,
            })
            .await?;
        let recording_id = recording_id(&recording.id)?;
        let relative_path = relative_path(&self.spool_root, &segment.path)?;
        let ordinal = direct_capture_ordinal(&segment.path)?;
        Ok(self
            .store
            .open_recording_layer(RecordingLayerDraft::capture(
                self.identity.clone(),
                recording_id,
                ordinal,
                relative_path,
                Some(segment.started_at),
            )?)
            .await?)
    }

    async fn register_frozen(&self, frozen: &FrozenSegment) -> Result<()> {
        let recording = self
            .store
            .recording_by_key(
                self.identity.tenant_id,
                &frozen.key.application_id,
                &frozen.key.recording,
            )
            .await?
            .context("frozen segment has no recording catalog entry")?;
        let recording_id = recording_id(&recording.id)?;
        let layer_name = format!("capture-{:020}", direct_capture_ordinal(&frozen.path)?);
        let layer = self
            .store
            .recording_layer_by_name(self.identity.tenant_id, recording_id, &layer_name)
            .await?
            .context("frozen capture has no recording layer entry")?;
        self.publish_layer(layer, &frozen.path, Some(frozen.ended_at))
            .await
    }

    async fn publish_layer(
        &self,
        layer: RecordingLayerRecord,
        path: &Path,
        ended_at: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let layer_id = layer_id(&layer.id)?;
        if layer.state == RecordingLayerState::Committed {
            remove_if_exists(path)?;
            return Ok(());
        }
        ensure!(
            matches!(
                layer.state,
                RecordingLayerState::Writing | RecordingLayerState::Staged
            ),
            "failed recording layer {layer_id} requires operator repair"
        );
        let recording_id = recording_id(&layer.recording)?;
        let recording = self
            .store
            .recording(self.identity.tenant_id, recording_id)
            .await?
            .context("recording layer has no recording")?;
        let dataset_id = dataset_id(&recording.dataset)?;
        let path = path.to_path_buf();
        let normalization_path = path.clone();
        let inspection = tokio::task::spawn_blocking(move || {
            veoveo_rrd::recording_layer::normalize_recording_layer(
                &normalization_path,
                dataset_id.as_uuid(),
                recording_id.as_uuid(),
            )
        })
        .await
        .context("joining recording layer normalization")??;
        let staged = self
            .store
            .stage_recording_layer(
                &self.identity,
                layer_id,
                i64::try_from(inspection.byte_len)?,
                i64::try_from(inspection.message_count)?,
                &inspection.sha256,
                Some(&inspection.rrd_version),
                Some(&inspection.schema_digest),
                ended_at,
            )
            .await?;
        let metadata = self
            .publisher
            .publish(
                layer_id,
                PutArtifactRequest {
                    mime_type: Some("application/vnd.rerun.rrd".to_owned()),
                    filename: Some(format!("{}.rrd", staged.layer_name)),
                    classification: artifact_classification(&recording.classification)?,
                    data_labels: artifact_labels(&recording.labels)?,
                    retention_expires_at: None,
                    metadata: serde_json::json!({
                        "recording_id": recording_id.to_string(),
                        "dataset_id": dataset_id.to_string(),
                        "layer_kind": "capture",
                        "schema_digest": inspection.schema_digest,
                    }),
                },
                path.as_path(),
                inspection.byte_len,
                &inspection.sha256,
            )
            .await?;
        let artifact_id =
            veoveo_platform_store::ArtifactId::from_uuid(metadata.artifact_id.as_uuid());
        self.store
            .commit_recording_layer(&self.identity, layer_id, artifact_id)
            .await?;
        remove_if_exists(path.as_path())?;
        Ok(())
    }
}

fn artifact_labels(values: &[String]) -> Result<BTreeSet<DataLabelId>> {
    values
        .iter()
        .map(|value| DataLabelId::new(value.to_owned()).map_err(Into::into))
        .collect()
}

fn artifact_classification(value: &str) -> Result<Option<DataLabelId>> {
    if value == "unclassified" {
        Ok(None)
    } else {
        DataLabelId::new(value.to_owned())
            .map(Some)
            .map_err(Into::into)
    }
}

impl SegmentCatalog for PlatformCatalog {
    fn segment_opened(&mut self, segment: &OpenedSegment) -> Result<()> {
        let this = self.clone();
        let segment = segment.clone();
        self.runtime
            .block_on(async move { this.register_opened(&segment).await })?;
        Ok(())
    }

    fn segment_frozen(&mut self, segment: &FrozenSegment) -> Result<()> {
        let this = self.clone();
        let segment = segment.clone();
        self.runtime
            .block_on(async move { this.register_frozen(&segment).await })
    }

    fn recording_finished(&mut self, key: &SegmentKey, ended_at: DateTime<Utc>) -> Result<()> {
        let this = self.clone();
        let key = key.clone();
        self.runtime.block_on(async move {
            let recording = this
                .store
                .recording_by_key(this.identity.tenant_id, &key.application_id, &key.recording)
                .await?
                .context("finished recording has no catalog entry")?;
            this.store
                .finish_recording(&this.identity, recording_id(&recording.id)?, ended_at)
                .await?;
            Result::<()>::Ok(())
        })
    }

    fn next_blueprint_revision(&mut self, key: &SegmentKey) -> Result<u64> {
        let this = self.clone();
        let key = key.clone();
        self.runtime.block_on(async move {
            let recording = this
                .store
                .recording_by_key(this.identity.tenant_id, &key.application_id, &key.recording)
                .await?
                .context("producer Blueprint has no recording catalog entry")?;
            let current = this
                .store
                .current_recording_blueprint(this.identity.tenant_id, recording_id(&recording.id)?)
                .await?;
            current
                .map(|blueprint| {
                    u64::try_from(blueprint.revision)
                        .context("stored Blueprint revision is negative")?
                        .checked_add(1)
                        .context("stored Blueprint revision overflow")
                })
                .unwrap_or(Ok(1))
        })
    }

    fn blueprint_published(&mut self, blueprint: &PublishedBlueprint) -> Result<()> {
        let this = self.clone();
        let blueprint = blueprint.clone();
        self.runtime.block_on(async move {
            let recording = this
                .store
                .recording_by_key(
                    this.identity.tenant_id,
                    &blueprint.key.application_id,
                    &blueprint.key.recording,
                )
                .await?
                .context("producer Blueprint has no recording catalog entry")?;
            let outcome = this
                .store
                .commit_recording_blueprint(RecordingBlueprintCommit {
                    draft: RecordingBlueprintDraft {
                        identity: this.identity.clone(),
                        recording_id: recording_id(&recording.id)?,
                        stream_id: None,
                        work_context: recording.work_context,
                        producer_id: this.policy.owner_key.clone(),
                        application_id: blueprint.key.application_id,
                        blueprint_id: blueprint.store_id.recording_id().as_str().to_owned(),
                        revision: blueprint.revision,
                        relative_path: relative_path(&this.spool_root, &blueprint.path)?,
                        sha256: blueprint.sha256,
                        byte_len: blueprint.byte_len,
                        message_count: blueprint.message_count,
                        maximum_revisions: this.policy.maximum_blueprint_revisions,
                    },
                    created_at: Utc::now(),
                })
                .await?;
            ensure!(
                !outcome.duplicate,
                "direct Blueprint revision unexpectedly duplicated"
            );
            Result::<()>::Ok(())
        })
    }
}

/// Fsync, decode, identify, and hash one RRD segment. It accepts a crash-safe
/// footer-less segment when Rerun can decode every complete message in it.
pub fn inspect_segment(path: &Path) -> Result<SegmentInspection> {
    let file = File::open(path).with_context(|| format!("opening segment {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("syncing segment {}", path.display()))?;
    let byte_len = file
        .metadata()
        .with_context(|| format!("reading segment metadata {}", path.display()))?
        .len();
    ensure!(byte_len > 0, "segment {} is empty", path.display());

    let mut hash = Sha256::new();
    let mut reader = BufReader::new(file);
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .with_context(|| format!("hashing segment {}", path.display()))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    let sha256 = hex::encode(hash.finalize());

    let engines = QueryEngine::from_rrd_filepath(&ChunkStoreConfig::DEFAULT, path)
        .with_context(|| format!("validating RRD segment {}", path.display()))?;
    let mut identities = engines
        .into_iter()
        .filter(|(store_id, _)| store_id.is_recording())
        .map(|(store_id, _)| {
            (
                store_id.application_id().as_str().to_owned(),
                store_id.recording_id().as_str().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    identities.sort();
    identities.dedup();
    ensure!(
        identities.len() == 1,
        "segment {} must contain exactly one recording identity",
        path.display()
    );
    let (application_id, recording_key) = identities.remove(0);
    Ok(SegmentInspection {
        application_id,
        recording_key,
        byte_len,
        sha256,
    })
}

fn segment_key_from_path(
    root: &Path,
    path: &Path,
    inspection: &SegmentInspection,
) -> Result<SegmentKey> {
    let relative = path
        .canonicalize()
        .with_context(|| format!("canonicalizing segment {}", path.display()))?
        .strip_prefix(root)
        .with_context(|| format!("segment {} escapes spool root", path.display()))?
        .to_path_buf();
    let mut components = relative.components();
    let dataset = components
        .next()
        .and_then(|value| value.as_os_str().to_str())
        .context("segment path has no UTF-8 dataset")?;
    let day = components
        .next()
        .and_then(|value| value.as_os_str().to_str())
        .context("segment path has no UTF-8 day")?;
    ensure!(components.count() == 1, "segment path has unexpected depth");
    Ok(SegmentKey {
        dataset: DatasetName::new(dataset)?,
        day: NaiveDate::parse_from_str(day, "%Y-%m-%d")?,
        application_id: inspection.application_id.clone(),
        recording: inspection.recording_key.clone(),
    })
}

fn relative_path(root: &Path, path: &Path) -> Result<String> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("canonicalizing segment {}", path.display()))?;
    let relative = canonical
        .strip_prefix(root)
        .with_context(|| format!("segment {} escapes spool root", path.display()))?;
    relative
        .to_str()
        .map(str::to_owned)
        .context("segment relative path is not UTF-8")
}

fn recording_id(record: &RecordId) -> Result<RecordingId> {
    Ok(RecordingId::from_uuid(record_uuid(record, "recording")?))
}

fn dataset_id(record: &RecordId) -> Result<RecordingDatasetId> {
    Ok(RecordingDatasetId::from_uuid(record_uuid(
        record,
        RecordingDatasetId::TABLE,
    )?))
}

fn layer_id(record: &RecordId) -> Result<RecordingLayerId> {
    Ok(RecordingLayerId::from_uuid(record_uuid(
        record,
        RecordingLayerId::TABLE,
    )?))
}

fn direct_capture_ordinal(path: &Path) -> Result<i64> {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .context("recording layer filename is not UTF-8")?;
    let ordinal = stem
        .rsplit_once(".r")
        .and_then(|(_, value)| value.parse::<i64>().ok())
        .unwrap_or(0);
    ensure!(ordinal >= 0, "recording layer ordinal must not be negative");
    Ok(ordinal)
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                File::open(parent)?.sync_all()?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("removing {}", path.display())),
    }
}

fn record_uuid(record: &RecordId, expected_table: &str) -> Result<uuid::Uuid> {
    ensure!(
        record.table.as_str() == expected_table,
        "expected {expected_table} record, got {}",
        record.table.as_str()
    );
    let raw = match &record.key {
        RecordIdKey::Uuid(value) => value.to_string(),
        RecordIdKey::String(value) => value.clone(),
        other => anyhow::bail!("record key is not a UUID: {other:?}"),
    };
    let value = uuid::Uuid::parse_str(&raw)?;
    ensure!(value.get_version_num() == 7, "record key is not UUIDv7");
    Ok(value)
}
