use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use flate2::read::GzDecoder;
use futures::StreamExt;
use reqwest::{Client, header::CONTENT_TYPE, redirect::Policy};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    authority::LeapSecondTable,
    catalog::{TimeCatalog, TimeScope},
    contract::{
        AuthorityDatasetKind, AuthorityRelease, AuthorityReleaseId, AuthorityReleaseState,
        TimeAcquisition, TimeAcquisitionId, TimeAcquisitionStatus, TimeSource,
    },
};

#[derive(Clone, Debug)]
pub struct AcquisitionServiceConfig {
    pub scratch_root: PathBuf,
    pub release_root: PathBuf,
    pub zic_executable: PathBuf,
    pub maximum_source_bytes: u64,
    pub maximum_expanded_bytes: u64,
    pub timeout: Duration,
}

#[derive(Clone)]
pub struct AcquisitionService {
    config: AcquisitionServiceConfig,
    catalog: TimeCatalog,
    client: Client,
    cancellation: Arc<tokio::sync::Mutex<BTreeMap<String, CancellationToken>>>,
}

impl AcquisitionService {
    pub fn new(config: AcquisitionServiceConfig, catalog: TimeCatalog) -> Result<Self> {
        validate_config(&config)?;
        std::fs::create_dir_all(&config.scratch_root)?;
        std::fs::create_dir_all(&config.release_root)?;
        let client = Client::builder()
            .redirect(Policy::none())
            .timeout(config.timeout)
            .https_only(true)
            .build()?;
        Ok(Self {
            config,
            catalog,
            client,
            cancellation: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
        })
    }

    pub async fn start(
        &self,
        scope: TimeScope,
        source: TimeSource,
        expected_digest: Option<String>,
        idempotency_key: String,
    ) -> Result<TimeAcquisition> {
        if !source.enabled {
            bail!("time authority source is disabled");
        }
        if let Some(digest) = &expected_digest {
            validate_sha256(digest)?;
        }
        if idempotency_key.trim().is_empty() {
            bail!("time acquisition idempotency key must not be empty");
        }
        if let Some(existing) = self
            .catalog
            .acquisition_for_idempotency(&scope, &idempotency_key)
            .await?
        {
            if existing.source_id == source.source_id
                && existing.expected_source_digest_sha256.as_deref() == expected_digest.as_deref()
            {
                return Ok(existing);
            }
            bail!("time acquisition idempotency key conflicts with a different request");
        }
        let now = Utc::now();
        let proposed_id = TimeAcquisitionId::new(format!("time-acquisition-{}", Uuid::now_v7()))
            .map_err(anyhow::Error::msg)?;
        let acquisition = TimeAcquisition {
            acquisition_id: proposed_id.clone(),
            source_id: source.source_id.clone(),
            expected_source_digest_sha256: expected_digest.clone(),
            status: TimeAcquisitionStatus::Queued,
            phase: "queued".to_owned(),
            staged_release_id: None,
            message: "authority acquisition queued".to_owned(),
            created_at: now,
            updated_at: now,
            record_version: 1,
        };
        let acquisition = self
            .catalog
            .create_acquisition(&scope, acquisition, idempotency_key)
            .await?;
        if acquisition.acquisition_id != proposed_id {
            return Ok(acquisition);
        }
        if acquisition.status != TimeAcquisitionStatus::Queued {
            return Ok(acquisition);
        }
        let cancellation = CancellationToken::new();
        self.cancellation
            .lock()
            .await
            .insert(acquisition.acquisition_id.to_string(), cancellation.clone());
        let service = self.clone();
        let acquisition_id = acquisition.acquisition_id.clone();
        tokio::spawn(async move {
            let run_cancellation = cancellation.clone();
            let result = service
                .run(
                    scope.clone(),
                    source,
                    acquisition_id.clone(),
                    expected_digest,
                    run_cancellation,
                )
                .await;
            if let Err(error) = result {
                tracing::warn!(%acquisition_id, "time authority acquisition failed: {error}");
                if let Ok(Some(mut job)) =
                    service.catalog.acquisition(&scope, &acquisition_id).await
                {
                    if !matches!(
                        job.status,
                        TimeAcquisitionStatus::Cancelled | TimeAcquisitionStatus::Succeeded
                    ) {
                        let cancelled = cancellation.is_cancelled()
                            || job.status == TimeAcquisitionStatus::CancelRequested;
                        job.status = if cancelled {
                            TimeAcquisitionStatus::Cancelled
                        } else {
                            TimeAcquisitionStatus::Failed
                        };
                        job.phase = if cancelled { "cancelled" } else { "failed" }.to_owned();
                        job.message = if cancelled {
                            "authority acquisition cancelled".to_owned()
                        } else {
                            error.to_string().chars().take(1024).collect()
                        };
                        let _ = service.catalog.update_acquisition(&scope, job).await;
                    }
                }
            }
            let _ = tokio::fs::remove_dir_all(
                service.config.scratch_root.join(acquisition_id.as_str()),
            )
            .await;
            service
                .cancellation
                .lock()
                .await
                .remove(acquisition_id.as_str());
        });
        Ok(acquisition)
    }

    pub async fn cancel(
        &self,
        scope: &TimeScope,
        id: &TimeAcquisitionId,
    ) -> Result<TimeAcquisition> {
        let mut acquisition = self
            .catalog
            .acquisition(scope, id)
            .await?
            .context("unknown time acquisition")?;
        if matches!(
            acquisition.status,
            TimeAcquisitionStatus::Queued
                | TimeAcquisitionStatus::Running
                | TimeAcquisitionStatus::CancelRequested
        ) {
            acquisition.status = TimeAcquisitionStatus::CancelRequested;
            acquisition.phase = "cancelling".to_owned();
            acquisition.message = "cancellation requested".to_owned();
            acquisition = self.catalog.update_acquisition(scope, acquisition).await?;
            if let Some(token) = self.cancellation.lock().await.get(id.as_str()) {
                token.cancel();
            }
        }
        Ok(acquisition)
    }

    async fn run(
        &self,
        scope: TimeScope,
        source: TimeSource,
        acquisition_id: TimeAcquisitionId,
        expected_digest: Option<String>,
        cancellation: CancellationToken,
    ) -> Result<()> {
        self.progress(
            &scope,
            &acquisition_id,
            TimeAcquisitionStatus::Running,
            "downloading",
            "downloading authoritative source",
            None,
        )
        .await?;
        let scratch = self.config.scratch_root.join(acquisition_id.as_str());
        tokio::fs::create_dir_all(&scratch).await?;
        let raw = scratch.join("source.bin");
        let digest = self.download(&source, &raw, cancellation.clone()).await?;
        if expected_digest
            .as_ref()
            .is_some_and(|expected| !expected.eq_ignore_ascii_case(&digest))
        {
            bail!("authority source digest does not match the acquisition request");
        }
        check_cancelled(&cancellation)?;
        self.progress(
            &scope,
            &acquisition_id,
            TimeAcquisitionStatus::Running,
            "validating",
            "validating authority data",
            None,
        )
        .await?;
        let (product, version_label) = match source.dataset_kind {
            AuthorityDatasetKind::Tzdb => {
                self.build_tzdb(&raw, &scratch, cancellation.clone())
                    .await?
            }
            AuthorityDatasetKind::LeapSeconds => {
                let content = tokio::fs::read_to_string(&raw).await?;
                LeapSecondTable::from_iana_content(&content)?;
                (raw.clone(), leap_version_label(&content, &digest))
            }
        };
        check_cancelled(&cancellation)?;
        let release_id = AuthorityReleaseId::new(format!("time-release-{}", Uuid::now_v7()))
            .map_err(anyhow::Error::msg)?;
        let release_dir = self.config.release_root.join(release_id.as_str());
        tokio::fs::create_dir_all(&release_dir).await?;
        let final_path = match source.dataset_kind {
            AuthorityDatasetKind::Tzdb => release_dir.join("tzdb"),
            AuthorityDatasetKind::LeapSeconds => release_dir.join("leap-seconds.list"),
        };
        tokio::fs::rename(&product, &final_path).await?;
        let now = Utc::now();
        let release = AuthorityRelease {
            release_id: release_id.clone(),
            source_id: source.source_id,
            dataset_kind: source.dataset_kind,
            version_label,
            source_url: source.url,
            source_digest_sha256: digest,
            artifact_path: final_path.to_string_lossy().into_owned(),
            state: AuthorityReleaseState::Staged,
            retrieved_at: now,
            validated_at: now,
            record_version: 1,
        };
        self.catalog.create_release(&scope, release).await?;
        self.progress(
            &scope,
            &acquisition_id,
            TimeAcquisitionStatus::Succeeded,
            "complete",
            "authority release staged",
            Some(release_id),
        )
        .await?;
        Ok(())
    }

    async fn download(
        &self,
        source: &TimeSource,
        destination: &Path,
        cancellation: CancellationToken,
    ) -> Result<String> {
        let response = self
            .client
            .get(&source.url)
            .send()
            .await?
            .error_for_status()?;
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        if !content_type.starts_with(&source.expected_content_type) {
            bail!("authority source returned an unexpected content type");
        }
        if response
            .content_length()
            .is_some_and(|length| length > self.config.maximum_source_bytes)
        {
            bail!("authority source exceeds the configured size limit");
        }
        let mut file = tokio::fs::File::create(destination).await?;
        let mut digest = Sha256::new();
        let mut received = 0_u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = tokio::select! { value = stream.next() => value, () = cancellation.cancelled() => bail!("authority acquisition cancelled") }
        {
            let chunk = chunk?;
            received = received.saturating_add(chunk.len() as u64);
            if received > self.config.maximum_source_bytes {
                bail!("authority source exceeds the configured size limit");
            }
            digest.update(&chunk);
            file.write_all(&chunk).await?;
        }
        file.flush().await?;
        Ok(hex::encode(digest.finalize()))
    }

    async fn build_tzdb(
        &self,
        raw: &Path,
        scratch: &Path,
        cancellation: CancellationToken,
    ) -> Result<(PathBuf, String)> {
        let source_dir = scratch.join("tzdb-source");
        let raw = raw.to_owned();
        let source_dir_clone = source_dir.clone();
        let maximum = self.config.maximum_expanded_bytes;
        tokio::task::spawn_blocking(move || extract_tzdb(&raw, &source_dir_clone, maximum))
            .await??;
        check_cancelled(&cancellation)?;
        let version_label = tokio::fs::read_to_string(source_dir.join("version"))
            .await
            .context("TZDB archive has no version file")?
            .trim()
            .to_owned();
        if version_label.is_empty() || version_label.len() > 32 {
            bail!("TZDB version label is invalid");
        }
        let output = scratch.join("tzdb-compiled");
        tokio::fs::create_dir_all(&output).await?;
        let files = [
            "africa",
            "antarctica",
            "asia",
            "australasia",
            "backward",
            "etcetera",
            "europe",
            "northamerica",
            "southamerica",
        ];
        let mut command = tokio::process::Command::new(&self.config.zic_executable);
        command
            .arg("-d")
            .arg(&output)
            .args(files.iter().map(|name| source_dir.join(name)))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let result = tokio::select! {
            result = tokio::time::timeout(self.config.timeout, command.output()) => result.context("zic compilation timed out")??,
            () = cancellation.cancelled() => bail!("authority acquisition cancelled"),
        };
        if !result.status.success() {
            bail!(
                "zic rejected the TZDB release: {}",
                String::from_utf8_lossy(&result.stderr)
                    .chars()
                    .take(2048)
                    .collect::<String>()
            );
        }
        jiff::tz::TimeZoneDatabase::from_dir(&output)?.get("UTC")?;
        jiff::tz::TimeZoneDatabase::from_dir(&output)?.get("America/New_York")?;
        Ok((output, version_label))
    }

    async fn progress(
        &self,
        scope: &TimeScope,
        id: &TimeAcquisitionId,
        status: TimeAcquisitionStatus,
        phase: &str,
        message: &str,
        staged_release_id: Option<AuthorityReleaseId>,
    ) -> Result<TimeAcquisition> {
        let mut job = self
            .catalog
            .acquisition(scope, id)
            .await?
            .context("time acquisition disappeared")?;
        job.status = status;
        job.phase = phase.to_owned();
        job.message = message.to_owned();
        job.staged_release_id = staged_release_id;
        self.catalog.update_acquisition(scope, job).await
    }
}

fn extract_tzdb(raw: &Path, destination: &Path, maximum_bytes: u64) -> Result<()> {
    std::fs::create_dir_all(destination)?;
    let file = std::fs::File::open(raw)?;
    let decoder = GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut expanded = 0_u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            bail!("TZDB archive contains an unsafe path");
        }
        let kind = entry.header().entry_type();
        if !(kind.is_file() || kind.is_dir()) {
            continue;
        }
        expanded = expanded.saturating_add(entry.header().size()?);
        if expanded > maximum_bytes {
            bail!("expanded TZDB exceeds the configured size limit");
        }
        entry.unpack_in(destination)?;
    }
    Ok(())
}

fn leap_version_label(content: &str, digest: &str) -> String {
    content
        .lines()
        .find_map(|line| {
            line.strip_prefix("#h")
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .map_or_else(
            || format!("iana-{}", &digest[..12]),
            |hash| format!("iana-{}", hash.chars().take(24).collect::<String>()),
        )
}

fn check_cancelled(cancellation: &CancellationToken) -> Result<()> {
    if cancellation.is_cancelled() {
        bail!("authority acquisition cancelled");
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("expected digest must be 64 hexadecimal characters");
    }
    Ok(())
}

fn validate_config(config: &AcquisitionServiceConfig) -> Result<()> {
    if !config.scratch_root.is_absolute()
        || !config.release_root.is_absolute()
        || !config.zic_executable.is_absolute()
        || config.maximum_source_bytes == 0
        || config.maximum_expanded_bytes < config.maximum_source_bytes
        || config.timeout.is_zero()
    {
        bail!("invalid time acquisition configuration");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::GzEncoder};

    #[test]
    fn rejects_relative_runtime_roots() {
        let config = AcquisitionServiceConfig {
            scratch_root: PathBuf::from("scratch"),
            release_root: PathBuf::from("releases"),
            zic_executable: PathBuf::from("/usr/sbin/zic"),
            maximum_source_bytes: 1,
            maximum_expanded_bytes: 2,
            timeout: Duration::from_secs(1),
        };
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn leap_release_label_is_stable() {
        assert_eq!(
            leap_version_label("#h abcdef\n", &"0".repeat(64)),
            "iana-abcdef"
        );
    }

    #[test]
    fn rejects_archive_path_traversal() {
        let temporary = tempfile::tempdir().unwrap();
        let archive_path = temporary.path().join("malicious.tar.gz");
        let file = std::fs::File::create(&archive_path).unwrap();
        let encoder = GzEncoder::new(file, Compression::default());
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(1);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.as_mut_bytes()[..100].fill(0);
        header.as_mut_bytes()[..9].copy_from_slice(b"../escape");
        header.set_cksum();
        archive.append(&header, &b"x"[..]).unwrap();
        archive.into_inner().unwrap().finish().unwrap();

        let result = extract_tzdb(&archive_path, &temporary.path().join("output"), 1024);
        assert!(result.unwrap_err().to_string().contains("unsafe path"));
        assert!(!temporary.path().join("escape").exists());
    }
}
