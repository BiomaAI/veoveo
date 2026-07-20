//! SurrealDB-backed catalog projection for durable RRD segments.
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
use veoveo_mcp_contract::{PrincipalId as ContractPrincipalId, TokenIssuer, TokenSubject};
use veoveo_platform_store::{
    InvocationAuthorityRecord, PlatformIdentity, PlatformStore, PrincipalKind, RecordId,
    RecordIdKey, RecordingDraft, RecordingId, RecordingState, SegmentDraft, SegmentId,
    SegmentState,
};

use crate::config::DatasetName;
use crate::governance::{governed_classification, governed_labels};
use crate::ingest::is_authenticated_ingest_path;
use crate::query::collect_segments;
use crate::spool::{FrozenSegment, OpenedSegment, SegmentCatalog, SegmentKey};

#[derive(Clone, Debug)]
pub struct CatalogPolicy {
    pub tenant_key: String,
    pub work_context_key: String,
    pub owner_key: String,
    pub owner_issuer: String,
    pub owner_subject: String,
    pub classification: String,
    pub labels: Vec<String>,
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
}

impl PlatformCatalog {
    pub async fn new(
        store: PlatformStore,
        spool_root: PathBuf,
        policy: CatalogPolicy,
        runtime: tokio::runtime::Handle,
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
        })
    }

    pub fn identity(&self) -> &PlatformIdentity {
        &self.identity
    }

    pub async fn reconcile(&self) -> Result<usize> {
        let mut reconciled = 0;
        let mut recovered_recordings = BTreeSet::new();
        for path in collect_segments(&self.spool_root)? {
            if is_authenticated_ingest_path(&path) {
                continue;
            }
            let inspection = inspect_segment(&path)?;
            let key = segment_key_from_path(&self.spool_root, &path, &inspection)?;
            let opened = OpenedSegment {
                key: key.clone(),
                path: path.clone(),
                started_at: Utc::now(),
            };
            let segment = self.register_opened(&opened).await?;
            recovered_recordings.insert(recording_id(&segment.recording)?);
            match segment.state {
                SegmentState::Writing => {
                    self.store
                        .freeze_segment(
                            &self.identity,
                            segment_id(&segment.id)?,
                            i64::try_from(inspection.byte_len)
                                .context("segment exceeds i64 byte length")?,
                            0,
                            &inspection.sha256,
                            None,
                        )
                        .await?;
                }
                SegmentState::Frozen | SegmentState::Sealed => {
                    ensure!(
                        segment.byte_len == i64::try_from(inspection.byte_len)?
                            && segment.sha256.as_deref() == Some(&inspection.sha256),
                        "cataloged segment {} changed on disk",
                        path.display()
                    );
                }
                SegmentState::Failed => {
                    anyhow::bail!("failed segment {} requires operator repair", path.display());
                }
            }
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

    async fn register_opened(
        &self,
        segment: &OpenedSegment,
    ) -> Result<veoveo_platform_store::SegmentRecord> {
        let recording = self
            .store
            .create_recording(RecordingDraft {
                identity: self.identity.clone(),
                authority: self.authority.clone(),
                dataset: segment.key.dataset.as_str().to_owned(),
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
        let segment_key = relative_path.clone();
        if let Some(existing) = self
            .store
            .segment_by_key(self.identity.tenant_id, recording_id, &segment_key)
            .await?
        {
            return Ok(existing);
        }
        let segments = self
            .store
            .recording_segments(self.identity.tenant_id, recording_id, 10_000)
            .await?;
        let ordinal = segments
            .iter()
            .map(|segment| segment.ordinal)
            .max()
            .map_or(0, |value| value + 1);
        Ok(self
            .store
            .open_segment(SegmentDraft {
                identity: self.identity.clone(),
                recording_id,
                segment_key,
                ordinal,
                relative_path,
                start_time: Some(Utc::now()),
            })
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
        let key = relative_path(&self.spool_root, &frozen.path)?;
        let segment = self
            .store
            .segment_by_key(self.identity.tenant_id, recording_id, &key)
            .await?
            .context("frozen segment has no segment catalog entry")?;
        self.store
            .freeze_segment(
                &self.identity,
                segment_id(&segment.id)?,
                i64::try_from(frozen.byte_len).context("segment exceeds i64 byte length")?,
                i64::try_from(frozen.message_count).context("segment exceeds i64 message count")?,
                &frozen.sha256,
                Some(frozen.ended_at),
            )
            .await?;
        Ok(())
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

fn segment_id(record: &RecordId) -> Result<SegmentId> {
    Ok(SegmentId::from_uuid(record_uuid(record, "segment")?))
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
