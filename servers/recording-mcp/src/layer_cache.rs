//! Bounded, verified local materialization of immutable Artifact-backed RRD layers.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context as _, Result, ensure};
use chrono::{DateTime, Utc};
use futures::StreamExt as _;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use tokio::io::AsyncWriteExt as _;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{ArtifactId, PlaneCaller};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayerCacheLimits {
    pub managed_bytes: u64,
    pub minimum_free_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayerCacheStats {
    pub managed_bytes: u64,
    pub minimum_free_bytes: u64,
    pub available_bytes: u64,
    pub committed_bytes: u64,
    pub pinned_bytes: u64,
    pub reserved_bytes: u64,
    pub entries: usize,
    pub evictions: u64,
    pub headroom_rejections: u64,
}

#[derive(Clone)]
pub struct LayerCache {
    inner: Arc<LayerCacheInner>,
}

struct LayerCacheInner {
    root: PathBuf,
    limits: LayerCacheLimits,
    artifacts: HttpArtifactPlane,
    state: Mutex<CacheState>,
    materialization: tokio::sync::Mutex<()>,
}

#[derive(Default)]
struct CacheState {
    entries: HashMap<String, CacheEntry>,
    reserved_bytes: u64,
    evictions: u64,
    headroom_rejections: u64,
}

struct CacheEntry {
    path: PathBuf,
    byte_len: u64,
    pins: usize,
    accessed_at: DateTime<Utc>,
}

pub struct CachedLayer {
    key: String,
    path: PathBuf,
    inner: Arc<LayerCacheInner>,
}

impl Clone for CachedLayer {
    fn clone(&self) -> Self {
        if let Ok(mut state) = self.inner.state.lock()
            && let Some(entry) = state.entries.get_mut(&self.key)
        {
            entry.pins = entry.pins.saturating_add(1);
            entry.accessed_at = Utc::now();
        }
        Self {
            key: self.key.clone(),
            path: self.path.clone(),
            inner: self.inner.clone(),
        }
    }
}

impl Drop for CachedLayer {
    fn drop(&mut self) {
        if let Ok(mut state) = self.inner.state.lock()
            && let Some(entry) = state.entries.get_mut(&self.key)
        {
            entry.pins = entry.pins.saturating_sub(1);
            entry.accessed_at = Utc::now();
        }
    }
}

impl CachedLayer {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl LayerCache {
    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    pub fn new(
        root: PathBuf,
        limits: LayerCacheLimits,
        artifacts: HttpArtifactPlane,
    ) -> Result<Self> {
        ensure!(
            root.is_absolute(),
            "recording layer cache root must be absolute"
        );
        ensure!(
            limits.managed_bytes > 0 && limits.minimum_free_bytes > 0,
            "recording layer cache limits must be positive"
        );
        std::fs::create_dir_all(&root)
            .with_context(|| format!("creating recording layer cache {}", root.display()))?;
        let root = root.canonicalize()?;
        let mut state = CacheState::default();
        for entry in std::fs::read_dir(&root)? {
            let entry = entry?;
            let path = entry.path();
            ensure!(
                entry.file_type()?.is_file(),
                "recording layer cache contains a non-file entry"
            );
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.contains(".partial-") {
                std::fs::remove_file(path)?;
                continue;
            }
            if !valid_cache_key_filename(&name) {
                anyhow::bail!("recording layer cache contains unknown file `{name}`");
            }
            state.entries.insert(
                name,
                CacheEntry {
                    byte_len: entry.metadata()?.len(),
                    path,
                    pins: 0,
                    accessed_at: Utc::now(),
                },
            );
        }
        let cache = Self {
            inner: Arc::new(LayerCacheInner {
                root,
                limits,
                artifacts,
                state: Mutex::new(state),
                materialization: tokio::sync::Mutex::new(()),
            }),
        };
        ensure!(
            cache.stats()?.committed_bytes <= limits.managed_bytes,
            "existing recording layer cache exceeds its managed ceiling"
        );
        Ok(cache)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn materialize(
        &self,
        caller: &PlaneCaller,
        artifact_id: ArtifactId,
        expected_byte_len: u64,
        expected_sha256: &str,
        dataset_id: uuid::Uuid,
        recording_id: uuid::Uuid,
    ) -> Result<CachedLayer> {
        ensure!(expected_byte_len > 0, "recording layer must not be empty");
        ensure!(
            valid_sha256(expected_sha256),
            "recording layer digest is invalid"
        );
        ensure!(
            expected_byte_len <= self.inner.limits.managed_bytes,
            "recording layer exceeds the managed cache ceiling"
        );
        let _materialization = self.inner.materialization.lock().await;
        let key = format!("{artifact_id}-{expected_sha256}.rrd");
        if let Some(cached) = self
            .existing(
                &key,
                expected_byte_len,
                expected_sha256,
                dataset_id,
                recording_id,
            )
            .await?
        {
            return Ok(cached);
        }
        self.reserve(expected_byte_len)?;
        let partial = self
            .inner
            .root
            .join(format!("{key}.partial-{}", uuid::Uuid::now_v7()));
        let final_path = self.inner.root.join(&key);
        let result = self
            .download(
                caller,
                artifact_id,
                expected_byte_len,
                expected_sha256,
                dataset_id,
                recording_id,
                &partial,
                &final_path,
            )
            .await;
        if let Err(error) = result {
            let _ = tokio::fs::remove_file(&partial).await;
            self.release_reservation(expected_byte_len);
            return Err(error);
        }
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("recording layer cache state is poisoned"))?;
        state.reserved_bytes = state.reserved_bytes.saturating_sub(expected_byte_len);
        state.entries.insert(
            key.clone(),
            CacheEntry {
                path: final_path.clone(),
                byte_len: expected_byte_len,
                pins: 1,
                accessed_at: Utc::now(),
            },
        );
        Ok(CachedLayer {
            key,
            path: final_path,
            inner: self.inner.clone(),
        })
    }

    pub fn stats(&self) -> Result<LayerCacheStats> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("recording layer cache state is poisoned"))?;
        Ok(LayerCacheStats {
            managed_bytes: self.inner.limits.managed_bytes,
            minimum_free_bytes: self.inner.limits.minimum_free_bytes,
            available_bytes: fs4::available_space(&self.inner.root)?,
            committed_bytes: state.entries.values().map(|entry| entry.byte_len).sum(),
            pinned_bytes: state
                .entries
                .values()
                .filter(|entry| entry.pins > 0)
                .map(|entry| entry.byte_len)
                .sum(),
            reserved_bytes: state.reserved_bytes,
            entries: state.entries.len(),
            evictions: state.evictions,
            headroom_rejections: state.headroom_rejections,
        })
    }

    pub fn readiness(&self) -> Result<()> {
        let stats = self.stats()?;
        ensure!(
            stats.committed_bytes.saturating_add(stats.reserved_bytes) <= stats.managed_bytes,
            "recording layer cache exceeds its managed ceiling"
        );
        ensure!(
            stats.available_bytes >= stats.minimum_free_bytes,
            "recording layer cache is below its minimum free-space headroom"
        );
        Ok(())
    }

    async fn existing(
        &self,
        key: &str,
        byte_len: u64,
        sha256: &str,
        dataset_id: uuid::Uuid,
        recording_id: uuid::Uuid,
    ) -> Result<Option<CachedLayer>> {
        let path = {
            let state = self
                .inner
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("recording layer cache state is poisoned"))?;
            state.entries.get(key).map(|entry| entry.path.clone())
        };
        let Some(path) = path else {
            return Ok(None);
        };
        let validation_path = path.clone();
        let expected_sha256 = sha256.to_owned();
        let valid = tokio::task::spawn_blocking(move || {
            validate_file(
                &validation_path,
                byte_len,
                &expected_sha256,
                dataset_id,
                recording_id,
            )
            .is_ok()
        })
        .await?;
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("recording layer cache state is poisoned"))?;
        if valid {
            let entry = state
                .entries
                .get_mut(key)
                .context("recording layer cache entry disappeared")?;
            entry.pins = entry.pins.saturating_add(1);
            entry.accessed_at = Utc::now();
            return Ok(Some(CachedLayer {
                key: key.to_owned(),
                path,
                inner: self.inner.clone(),
            }));
        }
        state.entries.remove(key);
        drop(state);
        match std::fs::remove_file(path) {
            Ok(()) => sync_directory(&self.inner.root)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(None)
    }

    fn reserve(&self, byte_len: u64) -> Result<()> {
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("recording layer cache state is poisoned"))?;
        loop {
            let used = state
                .entries
                .values()
                .map(|entry| entry.byte_len)
                .sum::<u64>();
            let available = fs4::available_space(&self.inner.root)?;
            if used
                .checked_add(state.reserved_bytes)
                .and_then(|value| value.checked_add(byte_len))
                .is_some_and(|value| value <= self.inner.limits.managed_bytes)
                && available >= byte_len.saturating_add(self.inner.limits.minimum_free_bytes)
            {
                state.reserved_bytes = state.reserved_bytes.saturating_add(byte_len);
                return Ok(());
            }
            let candidate = state
                .entries
                .iter()
                .filter(|(_, entry)| entry.pins == 0)
                .min_by_key(|(_, entry)| entry.accessed_at)
                .map(|(key, entry)| (key.clone(), entry.path.clone()));
            let Some((key, path)) = candidate else {
                state.headroom_rejections = state.headroom_rejections.saturating_add(1);
                anyhow::bail!(
                    "recording layer cache has insufficient managed or filesystem headroom"
                );
            };
            std::fs::remove_file(&path).with_context(|| {
                format!("evicting recording layer cache file {}", path.display())
            })?;
            state.entries.remove(&key);
            state.evictions = state.evictions.saturating_add(1);
            sync_directory(&self.inner.root)?;
        }
    }

    fn release_reservation(&self, byte_len: u64) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.reserved_bytes = state.reserved_bytes.saturating_sub(byte_len);
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn download(
        &self,
        caller: &PlaneCaller,
        artifact_id: ArtifactId,
        expected_byte_len: u64,
        expected_sha256: &str,
        dataset_id: uuid::Uuid,
        recording_id: uuid::Uuid,
        partial: &Path,
        final_path: &Path,
    ) -> Result<()> {
        let download = self
            .inner
            .artifacts
            .download(caller, &artifact_id.plane_uri())
            .await?;
        ensure!(
            download.metadata.artifact_id == artifact_id
                && download.metadata.byte_len == expected_byte_len,
            "Artifact metadata does not match the committed recording layer"
        );
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(partial)
            .await?;
        let mut stream = download.response.bytes_stream();
        let mut digest = Sha256::new();
        let mut written = 0_u64;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            written = written
                .checked_add(u64::try_from(chunk.len())?)
                .context("recording layer download length overflow")?;
            ensure!(
                written <= expected_byte_len,
                "recording layer download exceeded its committed length"
            );
            digest.update(&chunk);
            file.write_all(&chunk).await?;
        }
        ensure!(
            written == expected_byte_len && hex::encode(digest.finalize()) == expected_sha256,
            "recording layer download failed digest or length verification"
        );
        file.sync_all().await?;
        drop(file);
        let validation_path = partial.to_path_buf();
        let expected_sha256 = expected_sha256.to_owned();
        tokio::task::spawn_blocking(move || {
            validate_file(
                &validation_path,
                expected_byte_len,
                &expected_sha256,
                dataset_id,
                recording_id,
            )
        })
        .await??;
        tokio::fs::rename(partial, final_path).await?;
        sync_directory(&self.inner.root)?;
        Ok(())
    }
}

fn validate_file(
    path: &Path,
    byte_len: u64,
    sha256: &str,
    dataset_id: uuid::Uuid,
    recording_id: uuid::Uuid,
) -> Result<()> {
    ensure!(
        std::fs::metadata(path)?.len() == byte_len,
        "cached layer length mismatch"
    );
    let inspected = veoveo_rrd::recording_layer::inspect_canonical_recording_layer(
        path,
        dataset_id,
        recording_id,
    )?;
    ensure!(
        inspected.byte_len == byte_len && inspected.sha256 == sha256,
        "cached layer identity mismatch"
    );
    Ok(())
}

fn valid_cache_key_filename(name: &str) -> bool {
    let Some((artifact, digest_rrd)) = name.rsplit_once('-') else {
        return false;
    };
    let Some(digest) = digest_rrd.strip_suffix(".rrd") else {
        return false;
    };
    ArtifactId::parse(artifact).is_ok() && valid_sha256(digest)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(root: PathBuf) -> Result<LayerCache> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        LayerCache::new(
            root,
            LayerCacheLimits {
                managed_bytes: 1024 * 1024,
                minimum_free_bytes: 1,
            },
            HttpArtifactPlane::new("http://127.0.0.1:9"),
        )
    }

    #[test]
    fn startup_removes_partial_downloads_and_reports_headroom() {
        let directory = tempfile::tempdir().unwrap();
        let partial = directory.path().join("fixture.partial-0198");
        std::fs::write(&partial, b"partial").unwrap();
        let cache = cache(directory.path().to_path_buf()).unwrap();
        assert!(!partial.exists());
        let stats = cache.stats().unwrap();
        assert_eq!(stats.managed_bytes, 1024 * 1024);
        assert_eq!(stats.minimum_free_bytes, 1);
        assert_eq!(stats.committed_bytes, 0);
        assert!(stats.available_bytes > 0);
        cache.readiness().unwrap();
    }

    #[test]
    fn startup_rejects_unknown_cache_files() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("unexpected"), b"bad").unwrap();
        assert!(cache(directory.path().to_path_buf()).is_err());
    }
}
