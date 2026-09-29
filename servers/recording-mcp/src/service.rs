use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use chrono::{TimeDelta, Utc};
use sha2::{Digest as _, Sha256};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{GatewayInternalIdentity, PlaneCaller, PutArtifactRequest};
use veoveo_platform_store::{
    ArtifactId as PlatformArtifactId, PlatformIdentity, PlatformStore, PrincipalKind,
    RecordingBlueprintRecord, RecordingDatasetId, RecordingId, RecordingLayerDraft,
    RecordingLayerId, RecordingLayerKind, RecordingLayerRecord, RecordingLayerState,
    RecordingRecord, RecordingSeal, RecordingState,
};
use veoveo_recording_hub::{GatewayLayerPublisher, live_segment_byte_len};
use veoveo_rrd::properties_layer::{RecordingProperties, build_properties_layer};
use veoveo_types::{DataLabelId, ScopeDefinition, ScopeName, Sha256Digest};

use crate::contract::{
    LayerView, PlaybackLiveReceiver, RecordingManifest, RecordingManifestBuilder,
    RecordingManifestSchema, RecordingScope, RecordingView, SealRecordingOutput,
    SealRecordingOutputBuilder,
};
use veoveo_recording_reader::cache::{CachedLayer, LayerCache, LayerCacheLimits, LayerCacheStats};

mod grants;
mod views;
use views::{artifact_reference, artifact_uri, layer_view, manifest_blueprint, manifest_layer};
mod index;
mod projection;
pub use projection::{ProjectionDownload, ProjectionRuntimeLimits, ProjectionRuntimeStats};

use veoveo_recording_reader::{
    MAX_LAYERS,
    access::{authorized_live_layer_path, confined_layer_path, record_uuid},
};
const DEFAULT_LIVE_HISTORY_SECONDS: u64 = 1;
const LIVE_VIDEO_PREROLL_SECONDS: u64 = 2;
const MANIFEST_MIME: &str = "application/vnd.veoveo.recording-manifest+json";
const VIEWER_GRANT_TTL: TimeDelta = TimeDelta::minutes(5);

#[derive(Clone)]
pub struct RecordingPlaybackPlan {
    pub dataset_id: RecordingDatasetId,
    pub dataset_key: String,
    pub catalog_revision: String,
    pub recording_id: RecordingId,
    pub application_id: String,
    pub recording_key: String,
    pub state: RecordingState,
    pub started_at: chrono::DateTime<Utc>,
    pub ended_at: Option<chrono::DateTime<Utc>>,
    pub archive_layers: Vec<PlaybackArchiveLayerPlan>,
    pub live: Option<PlaybackLiveLayerPlan>,
    pub blueprint: Option<PlaybackBlueprintPlan>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackArchiveSelection {
    Complete,
    SealedViewer,
    Omit,
}

impl PlaybackArchiveSelection {
    fn materializes(self, state: RecordingState) -> bool {
        match self {
            Self::Complete => true,
            Self::SealedViewer => state != RecordingState::Live,
            Self::Omit => false,
        }
    }
}

#[derive(Clone)]
pub struct PlaybackArchiveLayerPlan {
    pub layer_id: RecordingLayerId,
    pub layer_name: String,
    pub kind: RecordingLayerKind,
    pub ordinal: Option<i64>,
    pub byte_len: u64,
    pub sha256: Sha256Digest,
    pub cached: CachedLayer,
}

#[derive(Clone, Debug)]
pub struct PlaybackLiveLayerPlan {
    pub descriptor: PlaybackLiveReceiver,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct PlaybackBlueprintPlan {
    pub blueprint_id: String,
    pub revision: u64,
    pub byte_len: u64,
    pub sha256: Sha256Digest,
    pub path: PathBuf,
    pub cached: Option<CachedLayer>,
    pub map_provider: veoveo_recording_hub::BlueprintMapProviderSelection,
}

#[derive(Clone)]
pub struct RecordingService {
    pub(super) store: PlatformStore,
    artifacts: HttpArtifactPlane,
    pub(super) spool_root: PathBuf,
    layer_cache: Option<LayerCache>,
    catalog_cache_root: Option<PathBuf>,
    layer_publisher: Option<GatewayLayerPublisher>,
    projection_runtime: Option<projection::ProjectionRuntime>,
    live_history_seconds: u64,
}

impl RecordingService {
    pub fn new(
        store: PlatformStore,
        artifacts: HttpArtifactPlane,
        spool_root: PathBuf,
    ) -> Result<Self> {
        ensure!(
            spool_root.is_absolute(),
            "recording spool root must be absolute"
        );
        let spool_root = spool_root
            .canonicalize()
            .with_context(|| format!("canonicalizing spool root {}", spool_root.display()))?;
        Ok(Self {
            store,
            artifacts,
            spool_root,
            layer_cache: None,
            catalog_cache_root: None,
            layer_publisher: None,
            projection_runtime: None,
            live_history_seconds: DEFAULT_LIVE_HISTORY_SECONDS,
        })
    }

    pub fn with_layer_cache(mut self, root: PathBuf, limits: LayerCacheLimits) -> Result<Self> {
        ensure!(
            root.is_absolute(),
            "recording catalog cache root must be absolute"
        );
        std::fs::create_dir_all(&root)?;
        let root = root.canonicalize()?;
        let cache = LayerCache::new(root.join("layers"), limits, self.artifacts.clone())?;
        self.catalog_cache_root = Some(root);
        self.layer_cache = Some(cache);
        Ok(self)
    }

    pub fn with_projection_runtime(mut self, limits: ProjectionRuntimeLimits) -> Result<Self> {
        let root = self
            .catalog_cache_root
            .as_ref()
            .context("recording layer cache must be configured before projections")?
            .join("projections");
        self.projection_runtime = Some(projection::ProjectionRuntime::new(root, limits)?);
        Ok(self)
    }

    pub fn with_layer_publisher(mut self, publisher: GatewayLayerPublisher) -> Self {
        self.layer_publisher = Some(publisher);
        self
    }

    pub fn with_live_history_seconds(mut self, seconds: u64) -> Result<Self> {
        ensure!(
            (1..=3600).contains(&seconds),
            "live history seconds must be in 1..=3600"
        );
        self.live_history_seconds = seconds;
        Ok(self)
    }

    pub fn live_history(&self) -> std::time::Duration {
        std::time::Duration::from_secs(
            self.live_history_seconds
                .saturating_add(LIVE_VIDEO_PREROLL_SECONDS),
        )
    }

    pub fn layer_cache_stats(&self) -> Result<Option<LayerCacheStats>> {
        self.layer_cache.as_ref().map(LayerCache::stats).transpose()
    }

    pub fn storage_readiness(&self) -> Result<()> {
        self.layer_cache
            .as_ref()
            .context("recording layer cache is not configured")?
            .readiness()?;
        self.projection_runtime_readiness()
    }

    pub fn platform_store(&self) -> &PlatformStore {
        &self.store
    }

    pub async fn platform_identity(
        &self,
        identity: &GatewayInternalIdentity,
    ) -> Result<PlatformIdentity> {
        let tenant_key = identity
            .actor
            .tenant
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "installation".to_owned());
        Ok(self
            .store
            .ensure_identity(
                &tenant_key,
                identity.actor.id.as_str(),
                identity.actor.issuer.as_str(),
                identity.actor.subject.as_str(),
                match identity.actor.kind {
                    veoveo_mcp_contract::PrincipalKind::User => PrincipalKind::User,
                    veoveo_mcp_contract::PrincipalKind::Service => PrincipalKind::Service,
                },
            )
            .await?)
    }

    pub async fn layer_views(
        &self,
        identity: &GatewayInternalIdentity,
        recording_id: RecordingId,
    ) -> Result<Option<Vec<LayerView>>> {
        let Some((platform_identity, _)) = self.visible_recording(identity, recording_id).await?
        else {
            return Ok(None);
        };
        let layers = self
            .store
            .recording_layers(platform_identity.tenant_id, recording_id, MAX_LAYERS)
            .await?;
        Ok(Some(
            layers.iter().map(layer_view).collect::<Result<Vec<_>>>()?,
        ))
    }

    pub async fn recording_view(
        &self,
        identity: &GatewayInternalIdentity,
        recording_id: RecordingId,
    ) -> Result<Option<RecordingView>> {
        let Some((platform_identity, recording)) =
            self.visible_recording(identity, recording_id).await?
        else {
            return Ok(None);
        };
        Ok(Some(
            self.view(platform_identity.tenant_id, recording).await?,
        ))
    }

    pub async fn playback_plan(
        &self,
        identity: &GatewayInternalIdentity,
        artifact_caller: Option<&PlaneCaller>,
        recording_id: RecordingId,
        archive_selection: PlaybackArchiveSelection,
    ) -> Result<Option<RecordingPlaybackPlan>> {
        let Some((platform_identity, recording)) =
            self.visible_recording(identity, recording_id).await?
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
        let catalog_layers = self
            .store
            .recording_layers(platform_identity.tenant_id, recording_id, MAX_LAYERS)
            .await?;
        let dataset_uuid = record_uuid(&dataset.id, "recording_dataset")?;
        let recording_uuid = record_uuid(&recording.id, "recording")?;
        let mut archive_layers = Vec::new();
        if archive_selection.materializes(recording.state)
            && let Some(artifact_caller) = artifact_caller
        {
            let cache = self
                .layer_cache
                .as_ref()
                .context("recording layer cache is not configured")?;
            for layer in catalog_layers
                .iter()
                .filter(|layer| layer.state == RecordingLayerState::Committed)
            {
                let layer_id =
                    RecordingLayerId::from_uuid(record_uuid(&layer.id, "recording_layer")?);
                let artifact_id = veoveo_artifact_contract::ArtifactId::parse(
                    record_uuid(
                        layer
                            .artifact
                            .as_ref()
                            .context("committed recording layer has no Artifact occurrence")?,
                        "artifact_occurrence",
                    )?
                    .to_string(),
                )?;
                let byte_len = u64::try_from(layer.byte_len)
                    .context("committed recording layer has negative byte length")?;
                let sha256 = Sha256Digest::from_hex(
                    layer
                        .sha256
                        .as_ref()
                        .context("committed recording layer has no digest")?,
                )?;
                let cached = cache
                    .materialize(
                        veoveo_mcp_contract::ArtifactReadAuthority::Caller(artifact_caller),
                        artifact_id,
                        byte_len,
                        &sha256,
                        dataset_uuid,
                        recording_uuid,
                    )
                    .await?;
                archive_layers.push(PlaybackArchiveLayerPlan {
                    layer_id,
                    layer_name: layer.layer_name.clone(),
                    kind: layer.kind,
                    ordinal: layer.ordinal,
                    byte_len,
                    sha256,
                    cached,
                });
            }
        }
        archive_layers.sort_by_key(|layer| {
            (
                layer_kind_order(layer.kind),
                layer.ordinal,
                layer.layer_name.clone(),
            )
        });

        let live = catalog_layers
            .iter()
            .filter(|layer| layer.state == RecordingLayerState::Writing)
            .filter_map(|layer| layer.ordinal.map(|ordinal| (ordinal, layer)))
            .max_by_key(|(ordinal, _)| *ordinal)
            .map(|(ordinal, layer)| {
                let relative = layer
                    .staging_path
                    .as_deref()
                    .context("writing recording layer has no staging path")?;
                let path = authorized_live_layer_path(&self.spool_root, relative)?;
                Ok::<_, anyhow::Error>(PlaybackLiveLayerPlan {
                    descriptor: PlaybackLiveReceiver {
                        layer_id: crate::contract::RecordingLayerId::try_from(record_uuid(
                            &layer.id,
                            "recording_layer",
                        )?)?,
                        layer_name: layer.layer_name.clone(),
                        ordinal,
                        current_byte_len: live_segment_byte_len(&path)?,
                        history_seconds: self.live_history_seconds,
                        video_preroll_seconds: LIVE_VIDEO_PREROLL_SECONDS,
                        transport: crate::contract::PlaybackLiveTransport::RerunRrdChannelV2,
                    },
                    path,
                })
            })
            .transpose()?;
        let blueprint = self
            .store
            .current_recording_blueprint(platform_identity.tenant_id, recording_id)
            .await?;
        let blueprint = match blueprint {
            Some(blueprint) => {
                self.playback_blueprint_plan(&recording, blueprint, artifact_caller)
                    .await?
            }
            None => None,
        };
        let catalog_revision =
            catalog_revision(dataset.revision, recording.revision, &catalog_layers);
        Ok(Some(RecordingPlaybackPlan {
            dataset_id,
            dataset_key: dataset.dataset_key,
            catalog_revision,
            recording_id,
            application_id: recording.application_id,
            recording_key: recording.recording_key,
            state: recording.state,
            started_at: recording.started_at,
            ended_at: recording.ended_at,
            archive_layers,
            live,
            blueprint,
        }))
    }

    async fn playback_blueprint_plan(
        &self,
        recording: &RecordingRecord,
        blueprint: RecordingBlueprintRecord,
        artifact_caller: Option<&PlaneCaller>,
    ) -> Result<Option<PlaybackBlueprintPlan>> {
        let sha256 = Sha256Digest::from_hex(&blueprint.sha256)?;
        let byte_len =
            u64::try_from(blueprint.byte_len).context("Blueprint byte length is negative")?;
        let message_count = u64::try_from(blueprint.message_count)
            .context("Blueprint message count is negative")?;
        let (path, cached) = if let Some(artifact) = blueprint.artifact.as_ref() {
            let Some(artifact_caller) = artifact_caller else {
                return Ok(None);
            };
            let cache = self
                .layer_cache
                .as_ref()
                .context("recording layer cache is not configured")?;
            let cached = cache
                .materialize_with_validator(
                    veoveo_mcp_contract::ArtifactReadAuthority::Caller(artifact_caller),
                    veoveo_artifact_contract::ArtifactId::parse(
                        record_uuid(artifact, "artifact_occurrence")?.to_string(),
                    )?,
                    byte_len,
                    &sha256,
                    std::sync::Arc::new(crate::blueprint_cache::BlueprintIdentity {
                        application_id: recording.application_id.clone(),
                        blueprint_id: blueprint.blueprint_id.clone(),
                        message_count,
                    }),
                )
                .await?;
            (cached.path().to_path_buf(), Some(cached))
        } else {
            ensure!(
                recording.state != RecordingState::Sealed,
                "sealed recording Blueprint has no Artifact occurrence"
            );
            (self.archive_path(&blueprint.relative_path)?, None)
        };
        let bytes = std::fs::read(&path)?;
        ensure!(
            bytes.len() as i64 == blueprint.byte_len
                && Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == sha256,
            "playback Blueprint no longer matches its governed publication"
        );
        let validated = veoveo_recording_hub::validate_blueprint_rrd(
            &bytes,
            message_count,
            &recording.application_id,
        )?;
        ensure!(
            validated.store_id.recording_id().as_str() == blueprint.blueprint_id,
            "playback Blueprint identity no longer matches its catalog record"
        );
        Ok(Some(PlaybackBlueprintPlan {
            blueprint_id: blueprint.blueprint_id,
            revision: u64::try_from(blueprint.revision)
                .context("Blueprint revision is negative")?,
            byte_len,
            sha256,
            path,
            cached,
            map_provider: validated.map_provider,
        }))
    }

    pub async fn dataset_playback_plans(
        &self,
        identity: &GatewayInternalIdentity,
        artifact_caller: &PlaneCaller,
        dataset_id: RecordingDatasetId,
        mut recording_ids: Vec<RecordingId>,
    ) -> Result<Option<Vec<RecordingPlaybackPlan>>> {
        recording_ids.sort_unstable();
        recording_ids.dedup();
        ensure!(
            !recording_ids.is_empty() && recording_ids.len() <= 500,
            "catalog grant must admit 1..=500 recordings"
        );
        let mut plans = Vec::with_capacity(recording_ids.len());
        for recording_id in recording_ids {
            let Some(plan) = self
                .playback_plan(
                    identity,
                    Some(artifact_caller),
                    recording_id,
                    PlaybackArchiveSelection::Complete,
                )
                .await?
            else {
                return Ok(None);
            };
            if plan.dataset_id != dataset_id {
                return Ok(None);
            }
            plans.push(plan);
        }
        Ok(Some(plans))
    }

    pub async fn seal(
        &self,
        identity: &GatewayInternalIdentity,
        recording_id: RecordingId,
    ) -> Result<SealRecordingOutput> {
        ensure_scope(&identity.actor.scopes, RecordingScope::Seal)?;
        let Some((platform_identity, recording)) =
            self.visible_recording(identity, recording_id).await?
        else {
            anyhow::bail!("recording not found");
        };
        if recording.state == RecordingState::Sealed {
            let dataset_id = RecordingDatasetId::from_uuid(record_uuid(
                &recording.dataset,
                "recording_dataset",
            )?);
            let dataset = self
                .store
                .recording_dataset(platform_identity.tenant_id, dataset_id)
                .await?
                .context("sealed recording dataset is missing")?;
            let output = self.sealed_output(&platform_identity, recording).await?;
            self.remove_recording_static_context(recording_id, &dataset.dataset_key)?;
            return Ok(output);
        }
        ensure!(
            matches!(
                recording.state,
                RecordingState::Ready | RecordingState::Interrupted | RecordingState::Sealing
            ),
            "recording is not sealable from state {}",
            recording_state(recording.state)
        );
        let mut layers = self
            .store
            .recording_layers(platform_identity.tenant_id, recording_id, MAX_LAYERS)
            .await?;
        ensure!(!layers.is_empty(), "recording has no layers");
        ensure!(
            layers
                .iter()
                .all(|layer| layer.state == RecordingLayerState::Committed),
            "recording contains a non-committed layer"
        );
        for layer in &layers {
            manifest_layer(layer)?;
        }
        if recording.state != RecordingState::Sealing {
            self.store
                .begin_recording_seal(&platform_identity, recording_id, None)
                .await?;
        }
        let dataset_id =
            RecordingDatasetId::from_uuid(record_uuid(&recording.dataset, "recording_dataset")?);
        let mut dataset = self
            .store
            .recording_dataset(platform_identity.tenant_id, dataset_id)
            .await?
            .context("recording dataset is missing")?;
        let current = self
            .store
            .recording(platform_identity.tenant_id, recording_id)
            .await?
            .context("recording disappeared while sealing")?;
        let sealed_at = current.updated_at;
        self.ensure_properties_layer(
            &platform_identity,
            &current,
            dataset_id,
            &dataset.dataset_key,
            sealed_at,
            &layers,
        )
        .await?;
        self.ensure_blueprint_artifact(&platform_identity, &current, dataset_id, recording_id)
            .await?;
        layers = self
            .store
            .recording_layers(platform_identity.tenant_id, recording_id, MAX_LAYERS)
            .await?;
        dataset = self
            .store
            .recording_dataset(platform_identity.tenant_id, dataset_id)
            .await?
            .context("recording dataset disappeared while sealing")?;
        let manifest_layers = layers
            .iter()
            .map(manifest_layer)
            .collect::<Result<Vec<_>>>()?;
        let manifest_blueprint = self
            .store
            .current_recording_blueprint(platform_identity.tenant_id, recording_id)
            .await?
            .map(manifest_blueprint)
            .transpose()?;
        let current = self
            .store
            .recording(platform_identity.tenant_id, recording_id)
            .await?
            .context("recording disappeared while sealing")?;
        let manifest = RecordingManifestBuilder {
            schema: RecordingManifestSchema::V9,
            dataset_id: crate::contract::RecordingDatasetId::try_from(dataset_id.as_uuid())?,
            recording_segment_id: crate::contract::RecordingId::try_from(recording_id.as_uuid())?,
            catalog_revision: catalog_revision(dataset.revision, current.revision, &layers),
            layers: manifest_layers.clone(),
            blueprint: manifest_blueprint.clone(),
            sealed_at,
        }
        .build()?;
        let manifest_artifact_id = if let Some(record) = current.manifest_artifact {
            PlatformArtifactId::from_uuid(record_uuid(&record, "artifact_occurrence")?)
        } else {
            let metadata = self
                .publish_manifest(&recording, dataset_id, recording_id, &manifest)
                .await?;
            let artifact_id = PlatformArtifactId::from_uuid(metadata.artifact_id().as_uuid());
            self.store
                .stage_recording_manifest(&platform_identity, recording_id, artifact_id)
                .await?;
            artifact_id
        };
        let output = SealRecordingOutputBuilder {
            recording_id: crate::contract::RecordingId::try_from(recording_id.as_uuid())?,
            manifest_artifact_uri: artifact_uri(manifest_artifact_id)?,
            layer_artifact_uris: manifest_layers
                .into_iter()
                .map(|layer| layer.artifact_uri.clone())
                .collect(),
            blueprint_artifact_uri: manifest_blueprint.map(|blueprint| blueprint.artifact_uri),
        }
        .build()?;
        self.store
            .complete_recording_seal(RecordingSeal {
                identity: platform_identity.clone(),
                recording_id,
                task_id: None,
                manifest_artifact_id,
                sealed_at,
            })
            .await?;
        self.remove_recording_static_context(recording_id, &dataset.dataset_key)?;
        Ok(output)
    }

    async fn ensure_properties_layer(
        &self,
        identity: &PlatformIdentity,
        recording: &RecordingRecord,
        dataset_id: RecordingDatasetId,
        dataset_key: &str,
        sealed_at: chrono::DateTime<Utc>,
        source_layers: &[RecordingLayerRecord],
    ) -> Result<()> {
        let publisher = self
            .layer_publisher
            .as_ref()
            .context("recording properties publisher is not configured")?;
        let cache_root = self
            .catalog_cache_root
            .as_ref()
            .context("recording catalog cache is not configured")?;
        let recording_id = RecordingId::from_uuid(record_uuid(&recording.id, "recording")?);
        let relative_path = format!("properties/{recording_id}.rrd");
        let path = cache_root.join(&relative_path);
        std::fs::create_dir_all(
            path.parent()
                .context("recording properties layer has no parent")?,
        )?;
        let mut layer = self
            .store
            .open_recording_layer(RecordingLayerDraft {
                identity: identity.clone(),
                recording_id,
                layer_name: "properties".to_owned(),
                kind: RecordingLayerKind::Properties,
                ordinal: None,
                staging_path: Some(relative_path),
                start_time: None,
            })
            .await?;
        if layer.state == RecordingLayerState::Committed {
            return Ok(());
        }
        let properties = RecordingProperties {
            dataset_id: uuid::Uuid::parse_str(&dataset_id.to_string())?,
            recording_id: uuid::Uuid::parse_str(&recording_id.to_string())?,
            dataset_key: dataset_key.to_owned(),
            producer_recording_key: recording.recording_key.clone(),
            lifecycle_state: "sealed".to_owned(),
            started_at: recording.started_at.to_rfc3339(),
            ended_at: recording
                .ended_at
                .context("sealable recording has no end time")?
                .to_rfc3339(),
            sealed_at: sealed_at.to_rfc3339(),
            source_revision: recording.revision,
            immutable_manifest_digest: source_layer_manifest_digest(
                dataset_id,
                recording_id,
                source_layers,
            ),
            model_revisions: Default::default(),
            environment_revisions: Default::default(),
        };
        if layer.state == RecordingLayerState::Writing {
            let inspection = if path.exists() {
                veoveo_rrd::recording_layer::inspect_canonical_recording_layer(
                    &path,
                    properties.dataset_id,
                    properties.recording_id,
                )?
            } else {
                build_properties_layer(&path, &properties)?
            };
            layer = self
                .store
                .stage_recording_layer(
                    identity,
                    RecordingLayerId::from_uuid(record_uuid(&layer.id, "recording_layer")?),
                    i64::try_from(inspection.byte_len)?,
                    i64::try_from(inspection.message_count)?,
                    &inspection.sha256,
                    Some(&inspection.rrd_version),
                    Some(&inspection.schema_digest),
                    Some(sealed_at),
                )
                .await?;
        }
        ensure!(
            layer.state == RecordingLayerState::Staged,
            "recording properties layer is not publishable"
        );
        let layer_id = RecordingLayerId::from_uuid(record_uuid(&layer.id, "recording_layer")?);
        let sha256 = layer
            .sha256
            .as_deref()
            .context("staged properties layer has no digest")?;
        let byte_len = u64::try_from(layer.byte_len)?;
        let metadata = publisher
            .publish(
                layer_id,
                PutArtifactRequest {
                    mime_type: Some("application/vnd.rerun.rrd".to_owned()),
                    filename: Some(format!("{recording_id}.properties.rrd")),
                    classification: artifact_classification(&recording.classification)?,
                    data_labels: labels(&recording.labels)?,
                    retention_expires_at: None,
                    metadata: serde_json::json!({
                        "provenance": {
                            "kind": "recording_layer",
                            "layer_kind": "properties",
                            "dataset_id": dataset_id,
                            "recording_id": recording_id,
                            "layer_id": layer_id,
                            "sha256": sha256,
                        }
                    }),
                },
                &path,
                byte_len,
                sha256,
            )
            .await?;
        ensure!(
            metadata.artifact_id().as_uuid() == uuid::Uuid::parse_str(&layer_id.to_string())?
                && metadata.byte_len == byte_len,
            "published properties occurrence does not match its reserved layer"
        );
        self.store
            .commit_recording_layer(
                identity,
                layer_id,
                PlatformArtifactId::from_uuid(metadata.artifact_id().as_uuid()),
            )
            .await?;
        match std::fs::remove_file(&path) {
            Ok(()) => {
                File::open(
                    path.parent()
                        .context("recording properties layer has no parent")?,
                )?
                .sync_all()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    async fn ensure_blueprint_artifact(
        &self,
        identity: &PlatformIdentity,
        recording: &RecordingRecord,
        dataset_id: RecordingDatasetId,
        recording_id: RecordingId,
    ) -> Result<()> {
        let Some(blueprint) = self
            .store
            .current_recording_blueprint(identity.tenant_id, recording_id)
            .await?
        else {
            return Ok(());
        };
        if blueprint.artifact.is_some() {
            self.remove_spool_staging_file(&blueprint.relative_path)?;
            return Ok(());
        }
        let publisher = self
            .layer_publisher
            .as_ref()
            .context("recording Blueprint publisher is not configured")?;
        let path = self.archive_path(&blueprint.relative_path)?;
        let bytes = std::fs::read(&path)?;
        let byte_len = u64::try_from(blueprint.byte_len)
            .context("recording Blueprint byte length is negative")?;
        let message_count = u64::try_from(blueprint.message_count)
            .context("recording Blueprint message count is negative")?;
        ensure!(
            bytes.len() as u64 == byte_len
                && hex::encode(Sha256::digest(&bytes)) == blueprint.sha256,
            "recording Blueprint staging bytes changed before publication"
        );
        let validated = veoveo_recording_hub::validate_blueprint_rrd(
            &bytes,
            message_count,
            &recording.application_id,
        )?;
        ensure!(
            validated.store_id.recording_id().as_str() == blueprint.blueprint_id,
            "recording Blueprint staging identity changed before publication"
        );
        let blueprint_occurrence = record_uuid(&blueprint.id, "recording_blueprint")?;
        let metadata = publisher
            .publish_artifact(
                veoveo_artifact_contract::ArtifactId::parse(blueprint_occurrence.to_string())?,
                PutArtifactRequest {
                    mime_type: Some("application/vnd.rerun.rrd".to_owned()),
                    filename: Some(format!(
                        "{}.blueprint-{}.rrd",
                        recording.recording_key, blueprint.revision
                    )),
                    classification: artifact_classification(&recording.classification)?,
                    data_labels: labels(&recording.labels)?,
                    retention_expires_at: None,
                    metadata: serde_json::json!({
                        "provenance": {
                            "kind": "recording_blueprint",
                            "dataset_id": dataset_id,
                            "recording_id": recording_id,
                            "blueprint_id": blueprint.blueprint_id,
                            "revision": blueprint.revision,
                            "sha256": blueprint.sha256,
                        }
                    }),
                },
                &path,
                byte_len,
                &blueprint.sha256,
            )
            .await?;
        ensure!(
            metadata.artifact_id().as_uuid() == blueprint_occurrence
                && metadata.byte_len == byte_len,
            "published Blueprint occurrence does not match its reserved identity"
        );
        self.store
            .stage_recording_blueprint_artifact(
                identity,
                recording_id,
                u64::try_from(blueprint.revision)
                    .context("recording Blueprint revision is negative")?,
                PlatformArtifactId::from_uuid(metadata.artifact_id().as_uuid()),
            )
            .await?;
        self.remove_spool_staging_file(&blueprint.relative_path)?;
        Ok(())
    }

    async fn publish_manifest(
        &self,
        recording: &RecordingRecord,
        dataset_id: RecordingDatasetId,
        recording_id: RecordingId,
        manifest: &RecordingManifest,
    ) -> Result<veoveo_artifact_contract::ArtifactMetadata> {
        let publisher = self
            .layer_publisher
            .as_ref()
            .context("recording manifest publisher is not configured")?;
        let cache_root = self
            .catalog_cache_root
            .as_ref()
            .context("recording catalog cache is not configured")?;
        let directory = cache_root.join("manifests");
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{recording_id}.v9.json"));
        let bytes = serde_json::to_vec_pretty(manifest)?;
        let sha256 = hex::encode(Sha256::digest(&bytes));
        if path.exists() {
            let existing = std::fs::read(&path)?;
            ensure!(
                existing == bytes,
                "staged recording manifest differs from the retry input"
            );
        } else {
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)?;
            use std::io::Write as _;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            File::open(&directory)?.sync_all()?;
        }
        let artifact_id = veoveo_artifact_contract::ArtifactId::parse(recording_id.to_string())?;
        let metadata = publisher
            .publish_artifact(
                artifact_id,
                PutArtifactRequest {
                    mime_type: Some(MANIFEST_MIME.to_owned()),
                    filename: Some(format!("{}.recording-v9.json", recording.recording_key)),
                    classification: artifact_classification(&recording.classification)?,
                    data_labels: labels(&recording.labels)?,
                    retention_expires_at: None,
                    metadata: serde_json::json!({
                        "provenance": {
                            "kind": "recording_manifest",
                            "recording_id": recording_id,
                            "dataset_id": dataset_id,
                            "catalog_revision": manifest.catalog_revision,
                            "sha256": sha256,
                        }
                    }),
                },
                &path,
                u64::try_from(bytes.len())?,
                &sha256,
            )
            .await?;
        match std::fs::remove_file(path) {
            Ok(()) => File::open(&directory)?.sync_all()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(metadata)
    }

    async fn sealed_output(
        &self,
        identity: &PlatformIdentity,
        recording: RecordingRecord,
    ) -> Result<SealRecordingOutput> {
        let recording_id = RecordingId::from_uuid(record_uuid(&recording.id, "recording")?);
        let manifest = recording
            .manifest_artifact
            .as_ref()
            .context("sealed recording has no manifest artifact")?;
        let layers = self
            .store
            .recording_layers(identity.tenant_id, recording_id, MAX_LAYERS)
            .await?;
        let layer_artifact_uris = layers
            .iter()
            .map(|layer| manifest_layer(layer).map(|layer| layer.artifact_uri.clone()))
            .collect::<Result<Vec<_>>>()?;
        let blueprint_artifact_uri = self
            .store
            .current_recording_blueprint(identity.tenant_id, recording_id)
            .await?
            .map(|blueprint| {
                let artifact = blueprint
                    .artifact
                    .as_ref()
                    .context("sealed recording Blueprint has no artifact")?;
                artifact_reference(artifact)
            })
            .transpose()?;
        Ok(SealRecordingOutputBuilder {
            recording_id: crate::contract::RecordingId::try_from(recording_id.as_uuid())?,
            manifest_artifact_uri: artifact_reference(manifest)?,
            layer_artifact_uris,
            blueprint_artifact_uri,
        }
        .build()?)
    }

    fn archive_path(&self, relative: &str) -> Result<PathBuf> {
        let path = confined_layer_path(&self.spool_root, relative)?;
        let canonical = path
            .canonicalize()
            .with_context(|| format!("canonicalizing recording file {}", path.display()))?;
        ensure!(
            canonical.starts_with(&self.spool_root) && canonical.is_file(),
            "recording file escapes the configured spool root"
        );
        Ok(canonical)
    }

    fn remove_spool_staging_file(&self, relative: &str) -> Result<()> {
        let path = confined_layer_path(&self.spool_root, relative)?;
        self.remove_spool_file(&path)
    }

    fn remove_recording_static_context(
        &self,
        recording_id: RecordingId,
        dataset_key: &str,
    ) -> Result<()> {
        let path = recording_static_context_path(&self.spool_root, dataset_key, recording_id)?;
        self.remove_spool_file(&path)
    }

    fn remove_spool_file(&self, path: &Path) -> Result<()> {
        ensure!(
            path.starts_with(&self.spool_root),
            "recording staging file escapes the configured spool root"
        );
        match std::fs::remove_file(path) {
            Ok(()) => {
                File::open(path.parent().context("staging file has no parent")?)?.sync_all()?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }
}

fn recording_static_context_path(
    spool_root: &Path,
    dataset_key: &str,
    recording_id: RecordingId,
) -> Result<PathBuf> {
    let dataset_path = confined_layer_path(spool_root, dataset_key)?;
    Ok(dataset_path.join(format!(".recording-{recording_id}.static-context")))
}

fn ensure_scope(grants: &BTreeSet<ScopeName>, required: RecordingScope) -> Result<()> {
    ensure!(
        grants.contains(required.name()),
        "Missing Recording scope `{required}`."
    );
    Ok(())
}

fn labels(values: &[String]) -> Result<BTreeSet<DataLabelId>> {
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

fn catalog_revision(
    dataset_revision: i64,
    recording_revision: i64,
    layers: &[RecordingLayerRecord],
) -> String {
    let mut digest = Sha256::new();
    digest.update(dataset_revision.to_be_bytes());
    digest.update(recording_revision.to_be_bytes());
    for layer in layers {
        digest.update(layer.layer_name.as_bytes());
        digest.update([0]);
        digest.update(layer.revision.to_be_bytes());
        if let Some(value) = &layer.sha256 {
            digest.update(value.as_bytes());
        }
    }
    hex::encode(digest.finalize())
}

pub fn catalog_set_revision(plans: &[RecordingPlaybackPlan]) -> String {
    let mut revisions = plans
        .iter()
        .map(|plan| (plan.recording_id, plan.catalog_revision.as_str()))
        .collect::<Vec<_>>();
    revisions.sort_unstable_by_key(|(recording_id, _)| *recording_id);
    let mut digest = Sha256::new();
    for (recording_id, revision) in revisions {
        digest.update(recording_id.as_uuid().as_bytes());
        digest.update(revision.as_bytes());
    }
    hex::encode(digest.finalize())
}

fn source_layer_manifest_digest(
    dataset_id: RecordingDatasetId,
    recording_id: RecordingId,
    layers: &[RecordingLayerRecord],
) -> String {
    let mut digest = Sha256::new();
    digest.update(dataset_id.to_string());
    digest.update([0]);
    digest.update(recording_id.to_string());
    for layer in layers
        .iter()
        .filter(|layer| layer.kind != RecordingLayerKind::Properties)
    {
        digest.update([0]);
        digest.update(layer.layer_name.as_bytes());
        digest.update(layer.byte_len.to_be_bytes());
        digest.update(layer.message_count.to_be_bytes());
        if let Some(sha256) = &layer.sha256 {
            digest.update(sha256.as_bytes());
        }
    }
    hex::encode(digest.finalize())
}

pub(super) fn recording_state(state: RecordingState) -> crate::contract::RecordingState {
    use crate::contract::RecordingState as Public;
    match state {
        RecordingState::Live => Public::Live,
        RecordingState::Ready => Public::Ready,
        RecordingState::Sealing => Public::Sealing,
        RecordingState::Sealed => Public::Sealed,
        RecordingState::Interrupted => Public::Interrupted,
        RecordingState::Failed => Public::Failed,
    }
}

pub(super) fn layer_kind(kind: RecordingLayerKind) -> crate::contract::RecordingLayerKind {
    use crate::contract::RecordingLayerKind as Public;
    match kind {
        RecordingLayerKind::Capture => Public::Capture,
        RecordingLayerKind::Properties => Public::Properties,
        RecordingLayerKind::Derived => Public::Derived,
    }
}

fn layer_kind_order(kind: RecordingLayerKind) -> u8 {
    match kind {
        RecordingLayerKind::Properties => 0,
        RecordingLayerKind::Capture => 1,
        RecordingLayerKind::Derived => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealing_requires_its_own_scope_even_for_an_administrator() {
        let mut grants = BTreeSet::from([
            ScopeName::new("admin:manage").unwrap(),
            ScopeName::new("recording:ingest").unwrap(),
        ]);
        assert!(ensure_scope(&grants, RecordingScope::Seal).is_err());
        grants.insert(RecordingScope::Seal.into());
        ensure_scope(&grants, RecordingScope::Seal).unwrap();
        grants.remove(RecordingScope::Seal.name());
        assert!(ensure_scope(&grants, RecordingScope::Seal).is_err());
    }

    #[test]
    fn sealed_static_context_path_is_confined_to_the_spool() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let recording_id = RecordingId::new();

        assert_eq!(
            recording_static_context_path(&root, "recordings", recording_id).unwrap(),
            root.join("recordings")
                .join(format!(".recording-{recording_id}.static-context"))
        );
        assert!(recording_static_context_path(&root, "../outside.rrd", recording_id).is_err());
    }

    #[test]
    fn viewer_materializes_archive_only_after_live_capture_ends() {
        assert!(!PlaybackArchiveSelection::SealedViewer.materializes(RecordingState::Live));
        for state in [
            RecordingState::Ready,
            RecordingState::Sealing,
            RecordingState::Sealed,
            RecordingState::Interrupted,
            RecordingState::Failed,
        ] {
            assert!(PlaybackArchiveSelection::SealedViewer.materializes(state));
        }
        assert!(PlaybackArchiveSelection::Complete.materializes(RecordingState::Live));
        assert!(!PlaybackArchiveSelection::Omit.materializes(RecordingState::Sealed));
    }
}
