use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use chrono::Utc;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use veoveo_duckdb_runtime::{
    HttpsSourcePolicy, materialize_https_source, materialize_https_source_with_headers,
};
use veoveo_mcp_contract::{ArtifactPut, ComplianceMetadata, PlaneCaller};

use crate::{
    artifacts::ArtifactRepository,
    catalog::{MapCatalog, MapScope},
    contract::{
        AcquisitionId, AcquisitionPhase, AcquisitionStatus, CreateAcquisitionRequest,
        DatasetRelease, DatasetReleaseId, DatasetReleaseState, RegisteredSource, SourceCredential,
        SourceLocation,
    },
    release_products::ReleaseProducts,
};

use super::AcquisitionHelper;

const NORMALIZATION_PIPELINE_VERSION: &str = "map-data-v1";
const ROUTING_BUILD_VERSION: &str = "valhalla-map-build-v1";
const MAX_SECRET_BYTES: u64 = 65_536;

#[derive(Clone, Debug)]
pub struct AcquisitionServiceConfig {
    pub scratch_root: PathBuf,
    pub mount_root: PathBuf,
    pub secret_root: PathBuf,
    pub maximum_artifact_bytes: u64,
}

#[derive(Clone)]
pub struct AcquisitionService {
    config: AcquisitionServiceConfig,
    catalog: MapCatalog,
    helper: AcquisitionHelper,
    artifacts: ArtifactRepository,
    products: ReleaseProducts,
    workers: Arc<Mutex<HashMap<AcquisitionId, CancellationToken>>>,
}

impl AcquisitionService {
    pub fn new(
        config: AcquisitionServiceConfig,
        catalog: MapCatalog,
        helper: AcquisitionHelper,
        artifacts: ArtifactRepository,
        products: ReleaseProducts,
    ) -> Result<Self> {
        for (name, path) in [
            ("scratch_root", &config.scratch_root),
            ("mount_root", &config.mount_root),
            ("secret_root", &config.secret_root),
        ] {
            if !path.is_absolute() {
                bail!("{name} must be absolute");
            }
        }
        if config.maximum_artifact_bytes == 0 {
            bail!("maximum_artifact_bytes must be positive");
        }
        std::fs::create_dir_all(&config.scratch_root)?;
        Ok(Self {
            config,
            catalog,
            helper,
            artifacts,
            products,
            workers: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn start(
        self: &Arc<Self>,
        scope: MapScope,
        caller: PlaneCaller,
        request: CreateAcquisitionRequest,
    ) -> Result<crate::contract::AcquisitionJob> {
        let source = self
            .catalog
            .source(&scope, &request.source_id)
            .await?
            .context("unknown map source")?;
        if !source.enabled {
            bail!("map source is disabled");
        }
        if source.acquisition_model != crate::contract::AcquisitionModel::Snapshot {
            bail!(
                "this Map runtime accepts snapshot acquisitions; operational feeds and sequenced deltas require their dedicated ingestion path"
            );
        }
        if source.adapter_kind == crate::contract::SourceAdapterKind::GtfsRealtime {
            bail!("GTFS Realtime is operational state and cannot be staged as a base release");
        }
        if source.maximum_download_bytes > self.config.maximum_artifact_bytes {
            bail!("source download limit exceeds the server artifact limit");
        }
        let acquisition_id = AcquisitionId::new();
        let job = self
            .catalog
            .create_acquisition(&scope, request, acquisition_id.clone())
            .await?;
        if job.acquisition_id != acquisition_id {
            return Ok(job);
        }
        let cancellation = CancellationToken::new();
        self.workers
            .lock()
            .await
            .insert(acquisition_id.clone(), cancellation.clone());
        let service = self.clone();
        tokio::spawn(async move {
            if let Err(error) = service
                .run(
                    scope.clone(),
                    caller,
                    source,
                    acquisition_id.clone(),
                    cancellation.clone(),
                )
                .await
            {
                tracing::warn!(%acquisition_id, "map acquisition failed: {error}");
                if let Ok(Some(mut job)) =
                    service.catalog.acquisition(&scope, &acquisition_id).await
                {
                    let cancelled = cancellation.is_cancelled();
                    job.status = if cancelled {
                        AcquisitionStatus::Cancelled
                    } else {
                        AcquisitionStatus::Failed
                    };
                    job.progress.message = if cancelled {
                        "acquisition cancelled".to_owned()
                    } else {
                        format!("acquisition failed during {:?}", job.progress.phase).to_lowercase()
                    };
                    let _ = service.catalog.update_acquisition(&scope, job).await;
                }
            }
            service.workers.lock().await.remove(&acquisition_id);
        });
        Ok(job)
    }

    pub async fn cancel(
        &self,
        scope: &MapScope,
        acquisition_id: &AcquisitionId,
    ) -> Result<crate::contract::AcquisitionJob> {
        let mut job = self
            .catalog
            .acquisition(scope, acquisition_id)
            .await?
            .context("unknown acquisition job")?;
        if matches!(
            job.status,
            AcquisitionStatus::Succeeded | AcquisitionStatus::Failed | AcquisitionStatus::Cancelled
        ) {
            bail!("acquisition job is already terminal");
        }
        job.status = AcquisitionStatus::CancelRequested;
        job.progress.message = "cancellation requested".to_owned();
        let job = self.catalog.update_acquisition(scope, job).await?;
        if let Some(cancellation) = self.workers.lock().await.get(acquisition_id) {
            cancellation.cancel();
        }
        Ok(job)
    }

    pub async fn reconcile_interrupted(&self, scope: &MapScope) -> Result<()> {
        let active_workers = self
            .workers
            .lock()
            .await
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        for mut job in self.catalog.list_acquisitions(scope).await? {
            if matches!(
                job.status,
                AcquisitionStatus::Queued
                    | AcquisitionStatus::Running
                    | AcquisitionStatus::CancelRequested
            ) && !active_workers.contains(&job.acquisition_id)
            {
                job.status = AcquisitionStatus::Failed;
                job.progress.message =
                    "acquisition was interrupted by a Map server restart".to_owned();
                self.catalog.update_acquisition(scope, job).await?;
            }
        }
        Ok(())
    }

    async fn run(
        &self,
        scope: MapScope,
        caller: PlaneCaller,
        source: RegisteredSource,
        acquisition_id: AcquisitionId,
        cancellation: CancellationToken,
    ) -> Result<()> {
        let workspace = self.config.scratch_root.join(acquisition_id.as_str());
        tokio::fs::create_dir(&workspace)
            .await
            .context("creating unique acquisition workspace")?;
        let input_dir = workspace.join("input");
        let output_dir = workspace.join("output");
        tokio::fs::create_dir(&input_dir).await?;
        tokio::fs::create_dir(&output_dir).await?;

        let result = self
            .run_in_workspace(
                &scope,
                &caller,
                &source,
                &acquisition_id,
                &input_dir,
                &output_dir,
                cancellation,
            )
            .await;
        if let Err(error) = tokio::fs::remove_dir_all(&workspace).await {
            tracing::warn!(%acquisition_id, "failed to remove acquisition workspace: {error}");
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_in_workspace(
        &self,
        scope: &MapScope,
        caller: &PlaneCaller,
        source: &RegisteredSource,
        acquisition_id: &AcquisitionId,
        input_dir: &Path,
        output_dir: &Path,
        cancellation: CancellationToken,
    ) -> Result<()> {
        self.progress(
            scope,
            acquisition_id,
            AcquisitionStatus::Running,
            AcquisitionPhase::Downloading,
            "materializing governed source",
        )
        .await?;
        let source_path = self.materialize(source, input_dir).await?;
        if cancellation.is_cancelled() {
            bail!("acquisition cancelled");
        }
        self.progress(
            scope,
            acquisition_id,
            AcquisitionStatus::Running,
            AcquisitionPhase::Normalizing,
            "normalizing source release",
        )
        .await?;
        let expected_source_digest_sha256 = self
            .catalog
            .acquisition(scope, acquisition_id)
            .await?
            .context("acquisition disappeared")?
            .expected_source_digest_sha256;
        let normalized = self
            .helper
            .normalize(
                acquisition_id,
                source.adapter_kind,
                &source_path,
                output_dir,
                Duration::from_secs(source.maximum_elapsed_seconds),
                cancellation.clone(),
            )
            .await?;
        if let Some(expected) = expected_source_digest_sha256 {
            if !expected.eq_ignore_ascii_case(&normalized.source_digest_sha256) {
                bail!("source content digest did not match the requested SHA-256 digest");
            }
        }
        if cancellation.is_cancelled() {
            bail!("acquisition cancelled");
        }
        self.progress(
            scope,
            acquisition_id,
            AcquisitionStatus::Running,
            AcquisitionPhase::PublishingArtifacts,
            "publishing immutable release artifacts",
        )
        .await?;
        let raw = self
            .put_file(
                caller,
                &source_path,
                "application/octet-stream",
                serde_json::json!({
                    "kind": "map_raw_source",
                    "source_id": source.source_id,
                    "acquisition_id": acquisition_id,
                    "sha256": normalized.source_digest_sha256,
                }),
            )
            .await?;
        let mut normalized_uris = Vec::new();
        for path in &normalized.normalized_paths {
            let artifact = self
                .put_file(
                    caller,
                    path,
                    media_type(path),
                    serde_json::json!({
                        "kind": "map_normalized_release",
                        "source_id": source.source_id,
                        "acquisition_id": acquisition_id,
                    }),
                )
                .await?;
            normalized_uris.push(artifact.artifact_id.plane_uri());
        }
        if let Some(path) = &normalized.routing_build_path {
            let artifact = self
                .put_file(
                    caller,
                    path,
                    "application/gzip",
                    serde_json::json!({
                        "kind": "map_routing_build",
                        "source_id": source.source_id,
                        "acquisition_id": acquisition_id,
                    }),
                )
                .await?;
            normalized_uris.push(artifact.artifact_id.plane_uri());
        }
        let quality = self
            .put_file(
                caller,
                &normalized.quality_report_path,
                "application/json",
                serde_json::json!({
                    "kind": "map_quality_report",
                    "source_id": source.source_id,
                    "acquisition_id": acquisition_id,
                }),
            )
            .await?;
        let now = Utc::now();
        let release = DatasetRelease {
            release_id: DatasetReleaseId::new(),
            dataset_id: source.dataset_id.clone(),
            source_id: source.source_id.clone(),
            version_label: normalized.version_label,
            source_digest_sha256: normalized.source_digest_sha256,
            coverage: self
                .catalog
                .acquisition(scope, acquisition_id)
                .await?
                .context("acquisition disappeared")?
                .requested_coverage,
            acquired_at: now,
            valid_from: now,
            valid_until: None,
            schema_version: 1,
            normalization_pipeline_version: NORMALIZATION_PIPELINE_VERSION.to_owned(),
            routing_build_version: normalized
                .routing_build_path
                .is_some()
                .then(|| ROUTING_BUILD_VERSION.to_owned()),
            license: source.license.clone(),
            raw_artifact_uri: raw.artifact_id.plane_uri(),
            normalized_artifact_uris: normalized_uris,
            quality_report_uri: quality.artifact_id.plane_uri(),
            supersedes_release_id: self
                .catalog
                .list_releases(scope)
                .await?
                .into_iter()
                .filter(|release| {
                    release.dataset_id == source.dataset_id
                        && release.state == DatasetReleaseState::Active
                })
                .max_by_key(|release| release.updated_at)
                .map(|release| release.release_id),
            state: DatasetReleaseState::Staged,
            record_version: 1,
            updated_at: now,
        };
        self.products
            .stage(
                &release,
                &normalized.normalized_paths,
                normalized.routing_build_path.as_deref(),
            )
            .await?;
        let release = match self.catalog.create_release(scope, release.clone()).await {
            Ok(release) => release,
            Err(error) => {
                self.products.discard(&release).await;
                return Err(error);
            }
        };
        let mut job = self
            .catalog
            .acquisition(scope, acquisition_id)
            .await?
            .context("acquisition disappeared")?;
        job.status = AcquisitionStatus::Succeeded;
        job.progress.phase = AcquisitionPhase::Complete;
        job.progress.completed_units = 1;
        job.progress.total_units = Some(1);
        job.progress.message = "staged release created".to_owned();
        job.raw_artifact_uri = Some(raw.artifact_id.plane_uri());
        job.staged_release_id = Some(release.release_id);
        self.catalog.update_acquisition(scope, job).await?;
        Ok(())
    }

    async fn progress(
        &self,
        scope: &MapScope,
        acquisition_id: &AcquisitionId,
        status: AcquisitionStatus,
        phase: AcquisitionPhase,
        message: &str,
    ) -> Result<()> {
        let mut job = self
            .catalog
            .acquisition(scope, acquisition_id)
            .await?
            .context("acquisition disappeared")?;
        job.status = status;
        job.progress.phase = phase;
        job.progress.message = message.to_owned();
        self.catalog.update_acquisition(scope, job).await?;
        Ok(())
    }

    async fn materialize(&self, source: &RegisteredSource, input_dir: &Path) -> Result<PathBuf> {
        let filename = source_filename(source);
        match &source.location {
            SourceLocation::Https {
                endpoint,
                allowed_redirect_hosts,
            } => {
                let mut hosts = allowed_redirect_hosts
                    .iter()
                    .map(|host| host.as_str().to_owned())
                    .collect::<BTreeSet<_>>();
                hosts.insert(endpoint.host());
                self.fetch_https(source, endpoint.as_str(), hosts, input_dir, filename)
                    .await
            }
            SourceLocation::OsmReplication {
                snapshot_endpoint,
                replication_endpoint,
            } => {
                let hosts = BTreeSet::from([snapshot_endpoint.host(), replication_endpoint.host()]);
                self.fetch_https(
                    source,
                    snapshot_endpoint.as_str(),
                    hosts,
                    input_dir,
                    filename,
                )
                .await
            }
            SourceLocation::MountedExchangeSet {
                mount_id,
                relative_path,
            } => {
                let mount_root = self.config.mount_root.clone();
                let input_dir = input_dir.to_owned();
                let mount_id = mount_id.as_str().to_owned();
                let relative_path = relative_path.as_str().to_owned();
                let filename = filename.to_owned();
                tokio::task::spawn_blocking(move || {
                    let root = mount_root.canonicalize()?;
                    let source = root.join(mount_id).join(relative_path).canonicalize()?;
                    if !source.starts_with(&root) || !source.is_file() {
                        bail!("mounted source is outside its controlled root or is not a file");
                    }
                    let destination = input_dir.join(filename);
                    std::fs::copy(source, &destination)?;
                    Ok(destination)
                })
                .await?
            }
        }
    }

    async fn fetch_https(
        &self,
        source: &RegisteredSource,
        endpoint: &str,
        hosts: BTreeSet<String>,
        input_dir: &Path,
        filename: &str,
    ) -> Result<PathBuf> {
        let mut policy = HttpsSourcePolicy::new(hosts);
        policy.max_bytes = source.maximum_download_bytes;
        policy.total_timeout = Duration::from_secs(source.maximum_elapsed_seconds);
        policy.set_allowed_media_types(source.expected_media_types.iter().cloned());
        let headers = self.credential_headers(source.credential.as_ref()).await?;
        let input_dir = input_dir.to_owned();
        let endpoint = endpoint.to_owned();
        let filename = filename.to_owned();
        tokio::task::spawn_blocking(move || match headers {
            Some(headers) => materialize_https_source_with_headers(
                &input_dir, &endpoint, &filename, &policy, &headers,
            ),
            None => materialize_https_source(&input_dir, &endpoint, &filename, &policy),
        })
        .await?
    }

    async fn credential_headers(
        &self,
        credential: Option<&SourceCredential>,
    ) -> Result<Option<HeaderMap>> {
        let Some(credential) = credential else {
            return Ok(None);
        };
        let root = self.config.secret_root.canonicalize()?;
        let path = root.join(credential.secret_ref().as_str()).canonicalize()?;
        if !path.starts_with(&root) || !path.is_file() {
            bail!("source credential reference is outside the secret root");
        }
        let metadata = tokio::fs::metadata(&path).await?;
        if metadata.len() > MAX_SECRET_BYTES {
            bail!("source credential exceeds its byte limit");
        }
        let mut secret = tokio::fs::read(path).await?;
        while secret
            .last()
            .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
        {
            secret.pop();
        }
        if secret.is_empty() {
            bail!("source credential is empty");
        }
        let mut headers = HeaderMap::new();
        match credential {
            SourceCredential::Bearer { .. } => {
                if secret.iter().any(u8::is_ascii_whitespace) {
                    bail!("bearer source credential contains whitespace");
                }
                let mut value = b"Bearer ".to_vec();
                value.extend_from_slice(&secret);
                headers.insert(AUTHORIZATION, HeaderValue::from_bytes(&value)?);
            }
            SourceCredential::Header { header_name, .. } => {
                headers.insert(
                    HeaderName::from_bytes(header_name.as_str().as_bytes())?,
                    HeaderValue::from_bytes(&secret)?,
                );
            }
        }
        secret.fill(0);
        Ok(Some(headers))
    }

    async fn put_file(
        &self,
        caller: &PlaneCaller,
        path: &Path,
        mime_type: &str,
        metadata: serde_json::Value,
    ) -> Result<veoveo_mcp_contract::ArtifactMetadata> {
        let file_metadata = tokio::fs::metadata(path).await?;
        if !file_metadata.is_file() || file_metadata.len() > self.config.maximum_artifact_bytes {
            bail!("acquisition product is not a file or exceeds the artifact limit");
        }
        let mut put = ArtifactPut::new(tokio::fs::read(path).await?);
        put.mime_type = Some(mime_type.to_owned());
        put.filename = Some(
            path.file_name()
                .and_then(|name| name.to_str())
                .context("acquisition product filename is not UTF-8")?
                .to_owned(),
        );
        put.compliance = ComplianceMetadata {
            tenant_id: caller.identity.principal.tenant.clone(),
            owner_id: Some(caller.identity.principal.id.clone()),
            data_labels: caller.identity.principal.data_labels.clone(),
            ..Default::default()
        };
        put.metadata = metadata;
        Ok(self
            .artifacts
            .put(caller, put)
            .await?
            .without_download_url())
    }
}

fn source_filename(source: &RegisteredSource) -> &'static str {
    match source.adapter_kind {
        crate::contract::SourceAdapterKind::OpenStreetMap => "source.osm.pbf",
        crate::contract::SourceAdapterKind::GtfsSchedule => "source.gtfs.zip",
        crate::contract::SourceAdapterKind::GtfsRealtime => "source.gtfs-rt.pb",
        crate::contract::SourceAdapterKind::S57Enc => "source.000",
        crate::contract::SourceAdapterKind::S100 => "source.s100.zip",
        crate::contract::SourceAdapterKind::Aixm => "source.aixm.xml",
        crate::contract::SourceAdapterKind::FaaNasr => "source.nasr.zip",
        crate::contract::SourceAdapterKind::AuthorityVector => "source.geojson",
        crate::contract::SourceAdapterKind::Environmental => "source.dat",
    }
}

fn media_type(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("parquet") => "application/vnd.apache.parquet",
        Some("json") | Some("geojson") => "application/geo+json",
        Some("zip") => "application/zip",
        Some("gz") => "application/gzip",
        _ => "application/octet-stream",
    }
}
