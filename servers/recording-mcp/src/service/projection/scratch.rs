//! Projection scratch accounting, bounded metadata and restart integrity.
use crate::contract::{
    MAX_PROJECTION_BYTES, MAX_PROJECTION_DEADLINE_MS, RecordingProjectionHandle,
};
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use std::{
    collections::HashMap,
    fs::File,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use veoveo_platform_store::RecordingProjectionReceiptId;
use veoveo_types::Sha256Digest;

pub(super) const MAX_METADATA_BYTES: u64 = 512 * 1024;

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
pub(in crate::service) struct ProjectionRuntime {
    inner: Arc<ProjectionRuntimeInner>,
}

struct ProjectionRuntimeInner {
    root: PathBuf,
    limits: ProjectionRuntimeLimits,
    state: Mutex<ProjectionScratchState>,
    permits: Arc<Semaphore>,
}

struct ProjectionScratchEntry {
    byte_len: u64,
    expires_at: DateTime<Utc>,
}

#[derive(Default)]
struct ProjectionScratchState {
    files: HashMap<RecordingProjectionReceiptId, ProjectionScratchEntry>,
    reserved_bytes: u64,
    headroom_rejections: u64,
    concurrency_rejections: u64,
}

pub(super) struct ProjectionReservation {
    runtime: ProjectionRuntime,
    projection_id: RecordingProjectionReceiptId,
    reserved_bytes: u64,
    settled: bool,
}

impl ProjectionRuntime {
    pub(super) fn maximum_deadline_ms(&self) -> u64 {
        self.inner.limits.maximum_deadline_ms
    }

    pub(super) fn sync(&self) -> Result<()> {
        sync_directory(&self.inner.root)
    }

    pub(in crate::service) fn new(root: PathBuf, limits: ProjectionRuntimeLimits) -> Result<Self> {
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
            state
                .files
                .values()
                .map(|entry| entry.byte_len)
                .sum::<u64>()
                <= limits.aggregate_scratch_bytes,
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

    pub(super) fn try_acquire(&self) -> Result<OwnedSemaphorePermit> {
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

    pub(super) fn reserve(
        &self,
        projection_id: RecordingProjectionReceiptId,
        byte_len: u64,
    ) -> Result<ProjectionReservation> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("projection scratch state is poisoned"))?;
        prune_expired(&self.inner.root, &mut state)?;
        ensure!(
            !state.files.contains_key(&projection_id),
            "projection result already exists"
        );
        let committed = state
            .files
            .values()
            .map(|entry| entry.byte_len)
            .sum::<u64>();
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

    pub(super) fn paths(&self, projection_id: RecordingProjectionReceiptId) -> ProjectionPaths {
        ProjectionPaths::new(&self.inner.root, projection_id)
    }

    pub(super) fn stats(&self) -> Result<ProjectionRuntimeStats> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("projection scratch state is poisoned"))?;
        Ok(ProjectionRuntimeStats {
            managed_bytes: self.inner.limits.aggregate_scratch_bytes,
            minimum_free_bytes: self.inner.limits.minimum_free_bytes,
            available_bytes: fs4::available_space(&self.inner.root)?,
            committed_bytes: state.files.values().map(|entry| entry.byte_len).sum(),
            reserved_bytes: state.reserved_bytes,
            files: state.files.len(),
            headroom_rejections: state.headroom_rejections,
            concurrency_rejections: state.concurrency_rejections,
        })
    }

    pub(super) fn readiness(&self) -> Result<()> {
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
    pub(super) fn commit(mut self, actual_bytes: u64, expires_at: DateTime<Utc>) -> Result<()> {
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
        state.files.insert(
            self.projection_id,
            ProjectionScratchEntry {
                byte_len: actual_bytes,
                expires_at,
            },
        );
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

pub(super) struct ProjectionPaths {
    pub(super) partial_arrow: PathBuf,
    pub(super) final_arrow: PathBuf,
    pub(super) partial_metadata: PathBuf,
    pub(super) final_metadata: PathBuf,
}

impl ProjectionPaths {
    fn new(root: &Path, projection_id: RecordingProjectionReceiptId) -> Self {
        let stem = projection_id.to_string();
        Self {
            partial_arrow: root.join(format!("{stem}.arrow.partial")),
            final_arrow: root.join(format!("{stem}.arrow")),
            partial_metadata: root.join(format!("{stem}.json.partial")),
            final_metadata: root.join(format!("{stem}.json")),
        }
    }
}

fn prune_expired(root: &Path, state: &mut ProjectionScratchState) -> Result<()> {
    let now = Utc::now();
    let expired: Vec<_> = state
        .files
        .iter()
        .filter_map(|(id, entry)| (entry.expires_at <= now).then_some(*id))
        .collect();
    for id in &expired {
        let paths = ProjectionPaths::new(root, *id);
        // Only committed pairs are indexed here. Active partial files remain owned by their worker.
        for path in [&paths.final_arrow, &paths.final_metadata] {
            remove_file_if_exists(path)?;
        }
        state.files.remove(id);
    }
    if !expired.is_empty() {
        sync_directory(root)?;
    }
    Ok(())
}

fn scratch_projection_id(stem: &str) -> Result<RecordingProjectionReceiptId> {
    Ok(RecordingProjectionReceiptId::from_uuid(
        crate::contract::RecordingProjectionId::parse(stem)?.as_uuid(),
    ))
}

pub(super) fn write_metadata(path: &Path, handle: &RecordingProjectionHandle) -> Result<()> {
    let bytes = serde_json::to_vec(handle)?;
    ensure!(
        bytes.len() as u64 <= MAX_METADATA_BYTES,
        "projection metadata exceeds its byte limit"
    );
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

pub(super) fn read_metadata(path: &Path) -> Result<RecordingProjectionHandle> {
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.is_file(),
        "projection metadata is not a regular file"
    );
    let mut bytes = Vec::new();
    file.take(MAX_METADATA_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_METADATA_BYTES,
        "projection metadata exceeds its byte limit"
    );
    Ok(serde_json::from_slice(&bytes)?)
}

pub(super) fn verify_file(path: &Path, byte_len: u64, sha256: &Sha256Digest) -> Result<()> {
    ensure!(
        (1..=MAX_PROJECTION_BYTES).contains(&byte_len),
        "projection file length exceeds its bounds"
    );
    let file = File::open(path)?;
    ensure!(
        file.metadata()?.len() == byte_len,
        "projection file length mismatch"
    );
    let mut file = file.take(byte_len + 1);
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
        Sha256Digest::from_bytes(digest.finalize().into()) == *sha256,
        "projection file digest mismatch"
    );
    Ok(())
}

pub(super) fn remove_projection_paths(paths: &ProjectionPaths, include_final: bool) -> Result<()> {
    for path in [&paths.partial_arrow, &paths.partial_metadata]
        .into_iter()
        .chain(include_final.then_some(&paths.final_arrow))
        .chain(include_final.then_some(&paths.final_metadata))
    {
        remove_file_if_exists(path)?;
    }
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
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
            let Ok(id) = scratch_projection_id(stem) else {
                anyhow::bail!("projection scratch contains unknown file `{name}`");
            };
            let handle = read_metadata(&path);
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
        let id = scratch_projection_id(stem)?;
        let candidate = metadata.remove(&id);
        let keep = candidate
            .as_ref()
            .and_then(|(_, handle)| handle.as_ref().ok())
            .is_some_and(|handle| {
                handle.expires_at > now
                    && handle.projection_id.as_uuid() == id.as_uuid()
                    && verify_file(
                        &entry.path(),
                        handle.result.byte_len.get(),
                        &handle.result.payload_sha256,
                    )
                    .is_ok()
            });
        if keep {
            let (metadata_path, handle) = candidate.expect("retained projection has metadata");
            state.files.insert(
                id,
                ProjectionScratchEntry {
                    byte_len: entry.metadata()?.len() + metadata_path.metadata()?.len(),
                    expires_at: handle.expect("retained metadata was validated").expires_at,
                },
            );
        } else {
            std::fs::remove_file(entry.path())?;
            if let Some((path, _)) = candidate {
                std::fs::remove_file(path)?;
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
