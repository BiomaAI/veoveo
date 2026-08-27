use std::collections::HashMap;
use std::fs::File;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context as _, Result, ensure};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::{
    CreateRecordingProjectionRequest, GatewayInternalIdentity, PlaneCaller,
    RECORDING_PROJECTION_HANDLE_SCHEMA, RecordingProjectionHandle,
    RecordingProjectionResultMetadata, RecordingProjectionSampling, RecordingProjectionSparseFill,
};
use veoveo_platform_store::{
    RecordId, RecordIdKey, RecordingDatasetId, RecordingId, RecordingProjectionReceiptDraft,
    RecordingProjectionReceiptId, RecordingProjectionReceiptRecord, RecordingProjectionState,
    RecordingReadGrantClass, RecordingReadGrantId,
};
use veoveo_rrd::projection::{
    MAX_PROJECTION_BYTES, MAX_PROJECTION_COMPONENTS, MAX_PROJECTION_ENTITIES, MAX_PROJECTION_ROWS,
    MAX_PROJECTION_SAMPLES, ProjectionQuery, ProjectionSampling, ProjectionSparseFill,
    write_arrow_projection_cancelable,
};

use super::{RecordingService, record_uuid};

const MAX_PROJECTION_DEADLINE_MS: u64 = 15_000;
const MAX_PROJECTION_CONCURRENCY: usize = 2;
const MAX_PROJECTION_SCRATCH_BYTES: u64 = 96 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProjectionRuntimeLimits {
    pub aggregate_scratch_bytes: u64,
    pub minimum_free_bytes: u64,
    pub concurrent_projections: usize,
    pub maximum_deadline_ms: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionRuntimeStats {
    pub managed_bytes: u64,
    pub minimum_free_bytes: u64,
    pub available_bytes: u64,
    pub committed_bytes: u64,
    pub reserved_bytes: u64,
    pub files: usize,
    pub headroom_rejections: u64,
    pub concurrency_rejections: u64,
}

#[derive(Clone)]
pub(super) struct ProjectionRuntime {
    inner: Arc<ProjectionRuntimeInner>,
}

struct ProjectionRuntimeInner {
    root: PathBuf,
    limits: ProjectionRuntimeLimits,
    state: Mutex<ProjectionScratchState>,
    permits: Arc<Semaphore>,
}

#[derive(Default)]
struct ProjectionScratchState {
    files: HashMap<RecordingProjectionReceiptId, u64>,
    reserved_bytes: u64,
    headroom_rejections: u64,
    concurrency_rejections: u64,
}

struct ProjectionReservation {
    runtime: ProjectionRuntime,
    projection_id: RecordingProjectionReceiptId,
    reserved_bytes: u64,
    settled: bool,
}

pub struct ProjectionDownload {
    pub path: PathBuf,
    pub byte_len: u64,
    pub sha256: String,
}

impl ProjectionRuntime {
    pub(super) fn new(root: PathBuf, limits: ProjectionRuntimeLimits) -> Result<Self> {
        ensure!(
            root.is_absolute(),
            "projection scratch root must be absolute"
        );
        ensure!(
            (1..=MAX_PROJECTION_SCRATCH_BYTES).contains(&limits.aggregate_scratch_bytes),
            "projection aggregate scratch limit exceeds the reviewed maximum"
        );
        ensure!(
            limits.minimum_free_bytes > 0,
            "projection minimum free bytes must be positive"
        );
        ensure!(
            (1..=MAX_PROJECTION_CONCURRENCY).contains(&limits.concurrent_projections),
            "projection concurrency exceeds the reviewed maximum"
        );
        ensure!(
            (1..=MAX_PROJECTION_DEADLINE_MS).contains(&limits.maximum_deadline_ms),
            "projection deadline exceeds the reviewed maximum"
        );
        std::fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let mut state = ProjectionScratchState::default();
        cleanup_and_index_scratch(&root, &mut state)?;
        ensure!(
            state.files.values().sum::<u64>() <= limits.aggregate_scratch_bytes,
            "existing projection scratch exceeds its managed ceiling"
        );
        Ok(Self {
            inner: Arc::new(ProjectionRuntimeInner {
                root,
                limits,
                state: Mutex::new(state),
                permits: Arc::new(Semaphore::new(limits.concurrent_projections)),
            }),
        })
    }

    fn try_acquire(&self) -> Result<OwnedSemaphorePermit> {
        match self.inner.permits.clone().try_acquire_owned() {
            Ok(permit) => Ok(permit),
            Err(_) => {
                if let Ok(mut state) = self.inner.state.lock() {
                    state.concurrency_rejections = state.concurrency_rejections.saturating_add(1);
                }
                anyhow::bail!("recording projection concurrency limit reached")
            }
        }
    }

    fn reserve(
        &self,
        projection_id: RecordingProjectionReceiptId,
        byte_len: u64,
    ) -> Result<ProjectionReservation> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("projection scratch state is poisoned"))?;
        ensure!(
            !state.files.contains_key(&projection_id),
            "projection result already exists"
        );
        let committed = state.files.values().sum::<u64>();
        let available = fs4::available_space(&self.inner.root)?;
        let managed = committed
            .checked_add(state.reserved_bytes)
            .and_then(|value| value.checked_add(byte_len));
        if !managed.is_some_and(|value| value <= self.inner.limits.aggregate_scratch_bytes)
            || available < byte_len.saturating_add(self.inner.limits.minimum_free_bytes)
        {
            state.headroom_rejections = state.headroom_rejections.saturating_add(1);
            anyhow::bail!("recording projection scratch has insufficient headroom");
        }
        state.reserved_bytes = state.reserved_bytes.saturating_add(byte_len);
        Ok(ProjectionReservation {
            runtime: self.clone(),
            projection_id,
            reserved_bytes: byte_len,
            settled: false,
        })
    }

    fn paths(&self, projection_id: RecordingProjectionReceiptId) -> ProjectionPaths {
        let stem = projection_id.to_string();
        ProjectionPaths {
            partial_arrow: self.inner.root.join(format!("{stem}.arrow.partial")),
            final_arrow: self.inner.root.join(format!("{stem}.arrow")),
            partial_metadata: self.inner.root.join(format!("{stem}.json.partial")),
            final_metadata: self.inner.root.join(format!("{stem}.json")),
        }
    }

    fn stats(&self) -> Result<ProjectionRuntimeStats> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("projection scratch state is poisoned"))?;
        Ok(ProjectionRuntimeStats {
            managed_bytes: self.inner.limits.aggregate_scratch_bytes,
            minimum_free_bytes: self.inner.limits.minimum_free_bytes,
            available_bytes: fs4::available_space(&self.inner.root)?,
            committed_bytes: state.files.values().sum(),
            reserved_bytes: state.reserved_bytes,
            files: state.files.len(),
            headroom_rejections: state.headroom_rejections,
            concurrency_rejections: state.concurrency_rejections,
        })
    }

    fn readiness(&self) -> Result<()> {
        let stats = self.stats()?;
        ensure!(
            stats.committed_bytes.saturating_add(stats.reserved_bytes) <= stats.managed_bytes,
            "recording projection scratch exceeds its managed ceiling"
        );
        ensure!(
            stats.available_bytes >= stats.minimum_free_bytes,
            "recording projection scratch is below its minimum free-space headroom"
        );
        Ok(())
    }
}

impl ProjectionReservation {
    fn commit(mut self, actual_bytes: u64) -> Result<()> {
        ensure!(
            actual_bytes <= self.reserved_bytes,
            "projection exceeded its reservation"
        );
        let mut state = self
            .runtime
            .inner
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("projection scratch state is poisoned"))?;
        state.reserved_bytes = state.reserved_bytes.saturating_sub(self.reserved_bytes);
        state.files.insert(self.projection_id, actual_bytes);
        self.settled = true;
        Ok(())
    }
}

impl Drop for ProjectionReservation {
    fn drop(&mut self) {
        if self.settled {
            return;
        }
        if let Ok(mut state) = self.runtime.inner.state.lock() {
            state.reserved_bytes = state.reserved_bytes.saturating_sub(self.reserved_bytes);
        }
    }
}

struct ProjectionPaths {
    partial_arrow: PathBuf,
    final_arrow: PathBuf,
    partial_metadata: PathBuf,
    final_metadata: PathBuf,
}

impl RecordingService {
    pub fn projection_runtime_stats(&self) -> Result<Option<ProjectionRuntimeStats>> {
        self.projection_runtime
            .as_ref()
            .map(ProjectionRuntime::stats)
            .transpose()
    }

    pub(super) fn projection_runtime_readiness(&self) -> Result<()> {
        self.projection_runtime
            .as_ref()
            .context("recording projection runtime is not configured")?
            .readiness()
    }

    pub async fn create_projection(
        &self,
        identity: &GatewayInternalIdentity,
        artifact_caller: &PlaneCaller,
        request: CreateRecordingProjectionRequest,
        cancellation: CancellationToken,
    ) -> Result<RecordingProjectionHandle> {
        let runtime = self
            .projection_runtime
            .as_ref()
            .context("recording projection runtime is not configured")?;
        ensure!(
            request.dataset_id.get_version_num() == 7
                && request.recording_id.get_version_num() == 7,
            "recording projection identities must be UUIDv7"
        );
        ensure!(
            (1..=runtime.inner.limits.maximum_deadline_ms).contains(&request.deadline_ms),
            "recording projection deadline exceeds the configured maximum"
        );
        validate_result_metadata_inputs(&request)?;
        let dataset_id = RecordingDatasetId::from_uuid(request.dataset_id);
        let recording_id = RecordingId::from_uuid(request.recording_id);
        let plan = self
            .playback_plan(identity, Some(artifact_caller), recording_id)
            .await?
            .context("recording projection source is not visible")?;
        ensure!(
            plan.dataset_id == dataset_id,
            "recording does not belong to the requested dataset"
        );
        ensure!(
            !plan.archive_layers.is_empty(),
            "recording has no committed immutable layers"
        );
        let platform_identity = self.platform_identity(identity).await?;
        let query_digest = projection_query_digest(&request)?;
        let manifest_digest = projection_manifest_digest(&plan);
        let existing = self
            .store
            .recording_projection_by_idempotency_key(
                platform_identity.tenant_id,
                platform_identity.principal_id,
                &request.idempotency_key,
            )
            .await?;
        let receipt = if let Some(existing) = existing {
            ensure!(
                existing.manifest_digest == manifest_digest
                    && existing.query_digest == query_digest,
                "recording projection idempotency key conflicts with another request"
            );
            existing
        } else {
            let grant = self
                .issue_read_grant(
                    identity,
                    dataset_id,
                    RecordingReadGrantClass::AppProjection,
                    vec![recording_id],
                    plan.catalog_revision.clone(),
                    None,
                )
                .await?;
            let grant_id = RecordingReadGrantId::from_uuid(record_uuid(
                &grant.id,
                RecordingReadGrantId::TABLE,
            )?);
            self.store
                .reserve_recording_projection(RecordingProjectionReceiptDraft {
                    identity: platform_identity.clone(),
                    grant_id,
                    caller_idempotency_key: request.idempotency_key.clone(),
                    manifest_digest: manifest_digest.clone(),
                    query_digest: query_digest.clone(),
                    expires_at: grant.expires_at,
                })
                .await?
        };
        let projection_id = projection_id(&receipt.id)?;
        let paths = runtime.paths(projection_id);
        if receipt.state == RecordingProjectionState::Ready {
            return read_handle(&paths, &receipt, &request, true);
        }
        if receipt.state == RecordingProjectionState::Materializing
            && paths.final_arrow.is_file()
            && paths.final_metadata.is_file()
        {
            let handle = read_handle(&paths, &receipt, &request, false)?;
            self.store
                .complete_recording_projection(
                    &platform_identity,
                    projection_id,
                    i64::try_from(handle.result.byte_len)?,
                    &handle.result.payload_sha256,
                )
                .await?;
            return Ok(handle);
        }
        ensure!(
            receipt.state == RecordingProjectionState::Reserved
                || receipt.state == RecordingProjectionState::Materializing,
            "recording projection is not retryable"
        );
        let _permit = runtime.try_acquire()?;
        let reservation = runtime.reserve(projection_id, request.maximum_bytes)?;
        self.store
            .begin_recording_projection(&platform_identity, projection_id)
            .await?;
        remove_projection_paths(&paths, false)?;
        let query = projection_query(&request);
        let layer_paths = plan
            .archive_layers
            .iter()
            .map(|layer| layer.cached.path().to_path_buf())
            .collect::<Vec<_>>();
        let partial_arrow = paths.partial_arrow.clone();
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let mut worker = tokio::task::spawn_blocking(move || {
            write_arrow_projection_cancelable(
                &layer_paths,
                &query,
                &partial_arrow,
                worker_cancelled,
            )
        });
        let deadline = tokio::time::sleep(Duration::from_millis(request.deadline_ms));
        tokio::pin!(deadline);
        let (worker_result, terminal) = tokio::select! {
            result = &mut worker => (Some(result), None),
            () = cancellation.cancelled() => (None, Some(ProjectionTerminal::Cancelled)),
            () = &mut deadline => (None, Some(ProjectionTerminal::Deadline)),
        };
        let worker_result = if let Some(result) = worker_result {
            result
        } else {
            cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
            worker.await
        };
        if let Some(terminal) = terminal {
            remove_projection_paths(&paths, true)?;
            match terminal {
                ProjectionTerminal::Cancelled => {
                    self.store
                        .cancel_recording_projection(&platform_identity, projection_id, "cancelled")
                        .await?;
                    anyhow::bail!("recording projection was cancelled");
                }
                ProjectionTerminal::Deadline => {
                    self.store
                        .fail_recording_projection(
                            &platform_identity,
                            projection_id,
                            "deadline_exceeded",
                        )
                        .await?;
                    anyhow::bail!("recording projection deadline exceeded");
                }
            }
        }
        let summary = match worker_result {
            Ok(Ok(summary)) => summary,
            Ok(Err(error)) => {
                remove_projection_paths(&paths, true)?;
                self.store
                    .fail_recording_projection(
                        &platform_identity,
                        projection_id,
                        "materialization_failed",
                    )
                    .await?;
                return Err(error);
            }
            Err(error) => {
                remove_projection_paths(&paths, true)?;
                self.store
                    .fail_recording_projection(&platform_identity, projection_id, "worker_failed")
                    .await?;
                return Err(error.into());
            }
        };
        let handle = RecordingProjectionHandle {
            schema: RECORDING_PROJECTION_HANDLE_SCHEMA.to_owned(),
            projection_id: projection_id.as_uuid(),
            dataset_id: request.dataset_id,
            recording_id: request.recording_id,
            result: RecordingProjectionResultMetadata {
                catalog_revision: plan.catalog_revision,
                query_digest,
                timeline: request.timeline.clone(),
                sample_grid: sample_grid(&request.sampling),
                units: request.units.clone(),
                coordinate_frame_refs: request.coordinate_frame_refs.clone(),
                omitted_sample_count: summary.omitted_sample_count,
                row_count: summary.row_count,
                arrow_schema_sha256: summary.schema_sha256,
                byte_len: summary.byte_len,
                payload_sha256: summary.sha256,
            },
            expires_at: receipt.expires_at.to_rfc3339(),
        };
        write_metadata(&paths.partial_metadata, &handle)?;
        std::fs::rename(&paths.partial_arrow, &paths.final_arrow)?;
        std::fs::rename(&paths.partial_metadata, &paths.final_metadata)?;
        sync_directory(&runtime.inner.root)?;
        reservation.commit(handle.result.byte_len)?;
        self.store
            .complete_recording_projection(
                &platform_identity,
                projection_id,
                i64::try_from(handle.result.byte_len)?,
                &handle.result.payload_sha256,
            )
            .await?;
        Ok(handle)
    }

    pub async fn projection_download(
        &self,
        identity: &GatewayInternalIdentity,
        recording_id: RecordingId,
        projection_id: RecordingProjectionReceiptId,
    ) -> Result<Option<ProjectionDownload>> {
        let runtime = self
            .projection_runtime
            .as_ref()
            .context("recording projection runtime is not configured")?;
        let platform_identity = self.platform_identity(identity).await?;
        let Some(receipt) = self
            .store
            .recording_projection_receipt(platform_identity.tenant_id, projection_id)
            .await?
        else {
            return Ok(None);
        };
        if receipt.actor != platform_identity.principal_id.record_id()
            || receipt.recordings.as_slice() != [recording_id.record_id()]
            || receipt.state != RecordingProjectionState::Ready
            || receipt.expires_at <= Utc::now()
        {
            return Ok(None);
        }
        let byte_len = u64::try_from(
            receipt
                .result_byte_len
                .context("ready projection has no length")?,
        )?;
        let sha256 = receipt
            .result_sha256
            .clone()
            .context("ready projection has no digest")?;
        let path = runtime.paths(projection_id).final_arrow;
        let validation_path = path.clone();
        let expected_sha256 = sha256.clone();
        tokio::task::spawn_blocking(move || {
            verify_file(&validation_path, byte_len, &expected_sha256)
        })
        .await??;
        Ok(Some(ProjectionDownload {
            path,
            byte_len,
            sha256,
        }))
    }
}

enum ProjectionTerminal {
    Cancelled,
    Deadline,
}

fn projection_query(request: &CreateRecordingProjectionRequest) -> ProjectionQuery {
    ProjectionQuery {
        entity_paths: request.entity_paths.clone(),
        component_ids: request.component_ids.clone(),
        timeline: request.timeline.clone(),
        sampling: match &request.sampling {
            RecordingProjectionSampling::Range { start, end } => ProjectionSampling::Range {
                start: *start,
                end: *end,
            },
            RecordingProjectionSampling::LatestAt { at } => {
                ProjectionSampling::LatestAt { at: *at }
            }
            RecordingProjectionSampling::SampleGrid { values } => ProjectionSampling::SampleGrid {
                values: values.clone(),
            },
        },
        sparse_fill: match request.sparse_fill {
            RecordingProjectionSparseFill::None => ProjectionSparseFill::None,
            RecordingProjectionSparseFill::LatestAtGlobal => ProjectionSparseFill::LatestAtGlobal,
        },
        maximum_entities: request.maximum_entities,
        maximum_columns: request.maximum_columns,
        maximum_samples: request.maximum_samples,
        maximum_rows: request.maximum_rows,
        maximum_bytes: request.maximum_bytes,
    }
}

fn projection_query_digest(request: &CreateRecordingProjectionRequest) -> Result<String> {
    let mut value = serde_json::to_value(request)?;
    value
        .as_object_mut()
        .context("projection request is not an object")?
        .remove("idempotency_key");
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&value)?)))
}

fn projection_manifest_digest(plan: &crate::RecordingPlaybackPlan) -> String {
    let mut digest = Sha256::new();
    digest.update(plan.dataset_id.as_uuid().as_bytes());
    digest.update(plan.recording_id.as_uuid().as_bytes());
    for layer in &plan.archive_layers {
        digest.update(layer.layer_id.as_uuid().as_bytes());
        digest.update(layer.sha256.as_bytes());
    }
    hex::encode(digest.finalize())
}

fn sample_grid(sampling: &RecordingProjectionSampling) -> Vec<i64> {
    match sampling {
        RecordingProjectionSampling::Range { .. } => Vec::new(),
        RecordingProjectionSampling::LatestAt { at } => vec![*at],
        RecordingProjectionSampling::SampleGrid { values } => values.clone(),
    }
}

fn validate_result_metadata_inputs(request: &CreateRecordingProjectionRequest) -> Result<()> {
    ensure!(
        request.maximum_entities <= MAX_PROJECTION_ENTITIES,
        "maximum_entities is too large"
    );
    ensure!(
        request.maximum_columns <= MAX_PROJECTION_COMPONENTS,
        "maximum_columns is too large"
    );
    ensure!(
        request.maximum_samples <= MAX_PROJECTION_SAMPLES,
        "maximum_samples is too large"
    );
    ensure!(
        request.maximum_rows <= MAX_PROJECTION_ROWS,
        "maximum_rows is too large"
    );
    ensure!(
        request.maximum_bytes <= MAX_PROJECTION_BYTES,
        "maximum_bytes is too large"
    );
    ensure!(
        request.idempotency_key.len() <= 128 && !request.idempotency_key.is_empty(),
        "idempotency_key is invalid"
    );
    ensure!(
        request.units.len() <= 64,
        "projection units exceed the fixed limit"
    );
    ensure!(
        request.coordinate_frame_refs.len() <= 64,
        "coordinate frame references exceed the fixed limit"
    );
    for value in request
        .units
        .iter()
        .flat_map(|(key, value)| [key.as_str(), value.as_str()])
        .chain(request.coordinate_frame_refs.iter().map(String::as_str))
    {
        ensure!(
            !value.is_empty() && value.len() <= 256,
            "projection metadata value is invalid"
        );
    }
    Ok(())
}

fn projection_id(record: &RecordId) -> Result<RecordingProjectionReceiptId> {
    ensure!(
        record.table.as_str() == RecordingProjectionReceiptId::TABLE,
        "projection record has the wrong table"
    );
    let raw = match &record.key {
        RecordIdKey::Uuid(value) => value.to_string(),
        RecordIdKey::String(value) => value.clone(),
        other => anyhow::bail!("projection record key is not a UUID: {other:?}"),
    };
    let value = uuid::Uuid::parse_str(&raw)?;
    ensure!(
        value.get_version_num() == 7,
        "projection record key is not UUIDv7"
    );
    Ok(RecordingProjectionReceiptId::from_uuid(value))
}

fn write_metadata(path: &Path, handle: &RecordingProjectionHandle) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    file.write_all(&serde_json::to_vec(handle)?)?;
    file.sync_all()?;
    Ok(())
}

fn read_handle(
    paths: &ProjectionPaths,
    receipt: &RecordingProjectionReceiptRecord,
    request: &CreateRecordingProjectionRequest,
    require_ready_receipt: bool,
) -> Result<RecordingProjectionHandle> {
    if require_ready_receipt {
        ensure!(
            receipt.state == RecordingProjectionState::Ready,
            "projection is not ready"
        );
    }
    let handle: RecordingProjectionHandle =
        serde_json::from_slice(&std::fs::read(&paths.final_metadata)?)?;
    ensure!(
        handle.schema == RECORDING_PROJECTION_HANDLE_SCHEMA
            && handle.projection_id == projection_id(&receipt.id)?.as_uuid()
            && handle.dataset_id == request.dataset_id
            && handle.recording_id == request.recording_id
            && handle.result.catalog_revision == receipt.catalog_revision
            && handle.result.query_digest == receipt.query_digest,
        "projection metadata does not match its receipt"
    );
    verify_file(
        &paths.final_arrow,
        handle.result.byte_len,
        &handle.result.payload_sha256,
    )?;
    if require_ready_receipt {
        ensure!(
            receipt.result_byte_len == Some(i64::try_from(handle.result.byte_len)?)
                && receipt.result_sha256.as_deref() == Some(&handle.result.payload_sha256),
            "projection result does not match its ready receipt"
        );
    }
    Ok(handle)
}

fn verify_file(path: &Path, byte_len: u64, sha256: &str) -> Result<()> {
    let mut file = File::open(path)?;
    ensure!(
        file.metadata()?.len() == byte_len,
        "projection file length mismatch"
    );
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    ensure!(
        hex::encode(digest.finalize()) == sha256,
        "projection file digest mismatch"
    );
    Ok(())
}

fn remove_projection_paths(paths: &ProjectionPaths, include_final: bool) -> Result<()> {
    for path in [&paths.partial_arrow, &paths.partial_metadata]
        .into_iter()
        .chain(include_final.then_some(&paths.final_arrow))
        .chain(include_final.then_some(&paths.final_metadata))
    {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn cleanup_and_index_scratch(root: &Path, state: &mut ProjectionScratchState) -> Result<()> {
    let now = Utc::now();
    let mut metadata = HashMap::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        ensure!(
            entry.file_type()?.is_file(),
            "projection scratch contains a non-file entry"
        );
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".partial") {
            std::fs::remove_file(path)?;
            continue;
        }
        if let Some(stem) = name.strip_suffix(".json") {
            let Ok(id) = stem.parse::<RecordingProjectionReceiptId>() else {
                anyhow::bail!("projection scratch contains unknown file `{name}`");
            };
            let handle = std::fs::read(&path)
                .map_err(serde_json::Error::io)
                .and_then(|bytes| serde_json::from_slice::<RecordingProjectionHandle>(&bytes));
            metadata.insert(id, (path, handle));
            continue;
        }
        if !name.ends_with(".arrow") {
            anyhow::bail!("projection scratch contains unknown file `{name}`");
        }
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(stem) = name.strip_suffix(".arrow") else {
            continue;
        };
        let id = stem.parse::<RecordingProjectionReceiptId>()?;
        let keep = metadata
            .get(&id)
            .and_then(|(_, handle)| handle.as_ref().ok())
            .is_some_and(|handle| {
                DateTime::parse_from_rfc3339(&handle.expires_at)
                    .map(|expires_at| expires_at.with_timezone(&Utc) > now)
                    .unwrap_or(false)
                    && handle.projection_id == id.as_uuid()
                    && handle.result.byte_len
                        == entry
                            .metadata()
                            .map(|value| value.len())
                            .unwrap_or_default()
            });
        if keep {
            state.files.insert(id, entry.metadata()?.len());
        } else {
            std::fs::remove_file(entry.path())?;
            if let Some((path, _)) = metadata.remove(&id) {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    for (_, (path, _)) in metadata {
        std::fs::remove_file(path)?;
    }
    sync_directory(root)?;
    Ok(())
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime(root: PathBuf, bytes: u64) -> ProjectionRuntime {
        ProjectionRuntime::new(
            root,
            ProjectionRuntimeLimits {
                aggregate_scratch_bytes: bytes,
                minimum_free_bytes: 1,
                concurrent_projections: 1,
                maximum_deadline_ms: 1_000,
            },
        )
        .unwrap()
    }

    #[test]
    fn reservations_and_concurrency_fail_before_work_starts() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = runtime(directory.path().to_path_buf(), 100);
        let first_id = RecordingProjectionReceiptId::new();
        let first = runtime.reserve(first_id, 80).unwrap();
        assert!(
            runtime
                .reserve(RecordingProjectionReceiptId::new(), 21)
                .is_err()
        );
        assert_eq!(runtime.stats().unwrap().headroom_rejections, 1);
        drop(first);
        assert_eq!(runtime.stats().unwrap().reserved_bytes, 0);

        let permit = runtime.try_acquire().unwrap();
        assert!(runtime.try_acquire().is_err());
        assert_eq!(runtime.stats().unwrap().concurrency_rejections, 1);
        drop(permit);
        let _permit = runtime.try_acquire().unwrap();
    }

    #[test]
    fn startup_removes_partial_and_invalid_projection_pairs() {
        let directory = tempfile::tempdir().unwrap();
        let partial = directory.path().join("orphan.arrow.partial");
        std::fs::write(&partial, b"partial").unwrap();
        let projection_id = RecordingProjectionReceiptId::new();
        let arrow = directory.path().join(format!("{projection_id}.arrow"));
        let metadata = directory.path().join(format!("{projection_id}.json"));
        std::fs::write(&arrow, b"arrow").unwrap();
        std::fs::write(&metadata, b"not-json").unwrap();

        let runtime = runtime(directory.path().to_path_buf(), 1024);
        assert!(!partial.exists());
        assert!(!arrow.exists());
        assert!(!metadata.exists());
        assert_eq!(runtime.stats().unwrap().files, 0);
        runtime.readiness().unwrap();
    }
}
