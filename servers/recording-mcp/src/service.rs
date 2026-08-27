use std::collections::BTreeSet;
use std::fs::File;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, ensure};
use chrono::{TimeDelta, Utc};
use sha2::{Digest as _, Sha256};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{DataLabelId, GatewayInternalIdentity, PlaneCaller, PutArtifactRequest};
use veoveo_platform_store::{
    ArtifactId as PlatformArtifactId, PlatformIdentity, PlatformStore, PrincipalKind, RecordId,
    RecordIdKey, RecordingBlueprintRecord, RecordingDatasetId, RecordingId, RecordingLayerDraft,
    RecordingLayerId, RecordingLayerKind, RecordingLayerRecord, RecordingLayerState,
    RecordingReadGrantClass, RecordingReadGrantDraft, RecordingReadGrantId,
    RecordingReadGrantRecord, RecordingRecord, RecordingSeal, RecordingState,
};
use veoveo_recording_hub::{
    GatewayLayerPublisher, ingest_recording_static_context_path, ingest_segment_parts_directory,
    invocation_authority_record, live_segment_byte_len,
};
use veoveo_rrd::properties_layer::{RecordingProperties, build_properties_layer};

use crate::contract::{
    LayerView, ManifestBlueprint, ManifestLayer, PlaybackLiveReceiver, RecordingManifest,
    RecordingView, SealRecordingOutput,
};
use crate::layer_cache::{CachedLayer, LayerCache, LayerCacheLimits, LayerCacheStats};

mod projection;
mod read;
pub use projection::{ProjectionDownload, ProjectionRuntimeLimits, ProjectionRuntimeStats};
pub use read::{
    MaterializedRecordingReadSnapshot, RecordingReadAuthority, RecordingReadLayer,
    RecordingReadPlan, RecordingReadSnapshot, RecordingReadSource, RecordingReadSourceKind,
};

const MAX_LAYERS: u32 = 10_000;
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

#[derive(Clone)]
pub struct PlaybackArchiveLayerPlan {
    pub layer_id: RecordingLayerId,
    pub layer_name: String,
    pub kind: RecordingLayerKind,
    pub ordinal: Option<i64>,
    pub byte_len: u64,
    pub sha256: String,
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
    pub sha256: String,
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

    pub async fn issue_read_grant(
        &self,
        identity: &GatewayInternalIdentity,
        dataset_id: RecordingDatasetId,
        grant_class: RecordingReadGrantClass,
        mut recording_ids: Vec<RecordingId>,
        catalog_revision: String,
        requested_grant: Option<&str>,
    ) -> Result<RecordingReadGrantRecord> {
        recording_ids.sort_unstable();
        recording_ids.dedup();
        ensure!(
            !recording_ids.is_empty(),
            "recording grant must admit at least one recording"
        );
        let platform_identity = self.platform_identity(identity).await?;
        if let Some(requested_grant) = requested_grant.filter(|value| value.len() <= 128)
            && let Ok(grant_id) = requested_grant.parse::<RecordingReadGrantId>()
            && grant_id.as_uuid().get_version_num() == 7
            && let Some(grant) = self
                .store
                .recording_read_grant(platform_identity.tenant_id, grant_id)
                .await?
        {
            let expected_recordings = recording_ids
                .iter()
                .copied()
                .map(RecordingId::record_id)
                .collect::<Vec<_>>();
            if grant.dataset == dataset_id.record_id()
                && grant.grant_class == grant_class
                && grant.recordings == expected_recordings
                && grant.actor == platform_identity.principal_id.record_id()
                && grant.policy_revision == identity.authority.policy_revision.as_str()
                && grant.catalog_revision == catalog_revision
            {
                return Ok(grant);
            }
        }
        Ok(self
            .store
            .create_recording_read_grant(RecordingReadGrantDraft {
                identity: platform_identity,
                authority: invocation_authority_record(&identity.authority),
                dataset_id,
                grant_class,
                recording_ids,
                catalog_revision,
                expires_at: Utc::now() + VIEWER_GRANT_TTL,
            })
            .await?)
    }

    pub async fn list_visible(
        &self,
        identity: &GatewayInternalIdentity,
    ) -> Result<Vec<RecordingView>> {
        let platform_identity = self.platform_identity(identity).await?;
        let mut views = Vec::new();
        for recording in self
            .store
            .list_recordings(platform_identity.tenant_id, 500)
            .await?
        {
            if visible(&recording, identity) {
                views.push(self.view(platform_identity.tenant_id, recording).await?);
            }
        }
        Ok(views)
    }

    pub async fn visible_recording(
        &self,
        identity: &GatewayInternalIdentity,
        recording_id: RecordingId,
    ) -> Result<Option<(PlatformIdentity, RecordingRecord)>> {
        let platform_identity = self.platform_identity(identity).await?;
        let recording = self
            .store
            .recording(platform_identity.tenant_id, recording_id)
            .await?;
        Ok(recording
            .filter(|recording| visible(recording, identity))
            .map(|recording| (platform_identity, recording)))
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
        if let Some(artifact_caller) = artifact_caller {
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
                let artifact_id = veoveo_mcp_contract::ArtifactId::parse(
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
                let sha256 = layer
                    .sha256
                    .clone()
                    .context("committed recording layer has no digest")?;
                let cached = cache
                    .materialize(
                        artifact_caller,
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
                        layer_id: record_uuid(&layer.id, "recording_layer")?.to_string(),
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
                .materialize_blueprint(
                    artifact_caller,
                    veoveo_mcp_contract::ArtifactId::parse(
                        record_uuid(artifact, "artifact_occurrence")?.to_string(),
                    )?,
                    byte_len,
                    &blueprint.sha256,
                    &recording.application_id,
                    &blueprint.blueprint_id,
                    message_count,
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
                && hex::encode(Sha256::digest(&bytes)) == blueprint.sha256,
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
            sha256: blueprint.sha256,
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
                .playback_plan(identity, Some(artifact_caller), recording_id)
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
        ensure_seal_scope(identity)?;
        let Some((platform_identity, recording)) =
            self.visible_recording(identity, recording_id).await?
        else {
            anyhow::bail!("recording not found");
        };
        if recording.state == RecordingState::Sealed {
            let layers = self
                .store
                .recording_layers(platform_identity.tenant_id, recording_id, MAX_LAYERS)
                .await?;
            self.remove_recording_static_context(recording_id, &layers)?;
            return self.sealed_output(&platform_identity, recording).await;
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
            .map(|blueprint| {
                let artifact = blueprint
                    .artifact
                    .as_ref()
                    .context("recording Blueprint has not been published")?;
                Ok::<_, anyhow::Error>(ManifestBlueprint {
                    blueprint_id: blueprint.blueprint_id,
                    revision: blueprint.revision,
                    byte_len: blueprint.byte_len,
                    message_count: blueprint.message_count,
                    sha256: blueprint.sha256,
                    artifact_uri: artifact_uri(PlatformArtifactId::from_uuid(record_uuid(
                        artifact,
                        "artifact_occurrence",
                    )?)),
                })
            })
            .transpose()?;
        let current = self
            .store
            .recording(platform_identity.tenant_id, recording_id)
            .await?
            .context("recording disappeared while sealing")?;
        let manifest_artifact_id = if let Some(record) = current.manifest_artifact {
            PlatformArtifactId::from_uuid(record_uuid(&record, "artifact_occurrence")?)
        } else {
            let manifest = RecordingManifest {
                schema: "veoveo.io/recording-manifest/v9".to_owned(),
                dataset_id: dataset_id.to_string(),
                recording_segment_id: recording_id.to_string(),
                catalog_revision: catalog_revision(dataset.revision, current.revision, &layers),
                layers: manifest_layers.clone(),
                blueprint: manifest_blueprint.clone(),
                sealed_at: sealed_at.to_rfc3339(),
            };
            let metadata = self
                .publish_manifest(&recording, dataset_id, recording_id, &manifest)
                .await?;
            let artifact_id = PlatformArtifactId::from_uuid(metadata.artifact_id.as_uuid());
            self.store
                .stage_recording_manifest(&platform_identity, recording_id, artifact_id)
                .await?;
            artifact_id
        };
        self.store
            .complete_recording_seal(RecordingSeal {
                identity: platform_identity.clone(),
                recording_id,
                task_id: None,
                manifest_artifact_id,
                sealed_at,
            })
            .await?;
        self.remove_recording_static_context(recording_id, &layers)?;
        Ok(SealRecordingOutput {
            recording_id: recording_id.to_string(),
            manifest_artifact_uri: artifact_uri(manifest_artifact_id),
            layer_artifact_uris: manifest_layers
                .into_iter()
                .map(|layer| layer.artifact_uri)
                .collect(),
            blueprint_artifact_uri: manifest_blueprint.map(|blueprint| blueprint.artifact_uri),
        })
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
            metadata.artifact_id.as_uuid() == uuid::Uuid::parse_str(&layer_id.to_string())?
                && metadata.byte_len == byte_len,
            "published properties occurrence does not match its reserved layer"
        );
        self.store
            .commit_recording_layer(
                identity,
                layer_id,
                PlatformArtifactId::from_uuid(metadata.artifact_id.as_uuid()),
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
                veoveo_mcp_contract::ArtifactId::parse(blueprint_occurrence.to_string())?,
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
            metadata.artifact_id.as_uuid() == blueprint_occurrence && metadata.byte_len == byte_len,
            "published Blueprint occurrence does not match its reserved identity"
        );
        self.store
            .stage_recording_blueprint_artifact(
                identity,
                recording_id,
                u64::try_from(blueprint.revision)
                    .context("recording Blueprint revision is negative")?,
                PlatformArtifactId::from_uuid(metadata.artifact_id.as_uuid()),
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
    ) -> Result<veoveo_mcp_contract::ArtifactMetadata> {
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
        let artifact_id = veoveo_mcp_contract::ArtifactId::parse(recording_id.to_string())?;
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
        match std::fs::remove_file(&path) {
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
            .map(|layer| {
                let artifact = layer
                    .artifact
                    .as_ref()
                    .context("committed layer has no artifact")?;
                Ok(artifact_uri(PlatformArtifactId::from_uuid(record_uuid(
                    artifact,
                    "artifact_occurrence",
                )?)))
            })
            .collect::<Result<Vec<_>>>()?;
        let blueprint_artifact_uri =
            self.store
                .current_recording_blueprint(identity.tenant_id, recording_id)
                .await?
                .map(|blueprint| {
                    let artifact = blueprint
                        .artifact
                        .as_ref()
                        .context("sealed recording Blueprint has no artifact")?;
                    Ok::<_, anyhow::Error>(artifact_uri(PlatformArtifactId::from_uuid(
                        record_uuid(artifact, "artifact_occurrence")?,
                    )))
                })
                .transpose()?;
        Ok(SealRecordingOutput {
            recording_id: recording_id.to_string(),
            manifest_artifact_uri: artifact_uri(PlatformArtifactId::from_uuid(record_uuid(
                manifest,
                "artifact_occurrence",
            )?)),
            layer_artifact_uris,
            blueprint_artifact_uri,
        })
    }

    async fn view(
        &self,
        tenant_id: veoveo_platform_store::TenantId,
        recording: RecordingRecord,
    ) -> Result<RecordingView> {
        let recording_id = RecordingId::from_uuid(record_uuid(&recording.id, "recording")?);
        let dataset_id =
            RecordingDatasetId::from_uuid(record_uuid(&recording.dataset, "recording_dataset")?);
        let dataset = self
            .store
            .recording_dataset(tenant_id, dataset_id)
            .await?
            .context("recording dataset is missing")?;
        let layers = self
            .store
            .recording_layers(tenant_id, recording_id, MAX_LAYERS)
            .await?;
        Ok(RecordingView {
            recording_id: recording_id.to_string(),
            dataset_id: dataset_id.to_string(),
            dataset_key: dataset.dataset_key,
            application_id: recording.application_id,
            recording_key: recording.recording_key,
            state: recording_state(recording.state).to_owned(),
            classification: recording.classification,
            labels: recording.labels,
            started_at: recording.started_at.to_rfc3339(),
            last_data_at: recording.last_data_at.to_rfc3339(),
            ended_at: recording.ended_at.map(|value| value.to_rfc3339()),
            sealed_at: recording.sealed_at.map(|value| value.to_rfc3339()),
            manifest_artifact_uri: recording.manifest_artifact.map(|record| {
                artifact_uri(PlatformArtifactId::from_uuid(
                    record_uuid(&record, "artifact_occurrence")
                        .expect("validated platform artifact record"),
                ))
            }),
            layer_count: layers.len(),
            committed_layer_count: layers
                .iter()
                .filter(|layer| layer.state == RecordingLayerState::Committed)
                .count(),
        })
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
        layers: &[RecordingLayerRecord],
    ) -> Result<()> {
        let mut paths = BTreeSet::new();
        for layer in layers
            .iter()
            .filter(|layer| layer.kind == RecordingLayerKind::Capture)
        {
            let Some(relative) = layer.staging_path.as_deref() else {
                continue;
            };
            paths.insert(recording_static_context_path(
                &self.spool_root,
                relative,
                recording_id,
            )?);
        }
        for path in paths {
            self.remove_spool_file(&path)?;
        }
        Ok(())
    }

    fn remove_spool_file(&self, path: &Path) -> Result<()> {
        ensure!(
            path.starts_with(&self.spool_root),
            "recording staging file escapes the configured spool root"
        );
        match std::fs::remove_file(&path) {
            Ok(()) => {
                File::open(path.parent().context("staging file has no parent")?)?.sync_all()?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }
}

fn authorized_live_layer_path(spool_root: &Path, relative: &str) -> Result<PathBuf> {
    let path = confined_layer_path(spool_root, relative)?;
    if path.exists() {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("canonicalizing live layer {}", path.display()))?;
        ensure!(
            canonical.starts_with(spool_root) && canonical.is_file(),
            "live layer escapes the configured spool root"
        );
        return Ok(canonical);
    }
    let parts = ingest_segment_parts_directory(&path);
    if parts.exists() {
        let canonical_parts = parts
            .canonicalize()
            .with_context(|| format!("canonicalizing live layer parts {}", parts.display()))?;
        ensure!(
            canonical_parts.starts_with(spool_root) && canonical_parts.is_dir(),
            "live layer parts escape the configured spool root"
        );
        return Ok(path);
    }
    let parent = path.parent().context("live layer path has no parent")?;
    let canonical_parent = parent
        .canonicalize()
        .with_context(|| format!("canonicalizing live layer parent {}", parent.display()))?;
    ensure!(
        canonical_parent.starts_with(spool_root) && canonical_parent.is_dir(),
        "live layer parent escapes the configured spool root"
    );
    Ok(path)
}

fn confined_layer_path(spool_root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = Path::new(relative);
    ensure!(
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|component| matches!(component, Component::Normal(_))),
        "recording layer path must be a normalized relative path"
    );
    Ok(spool_root.join(relative))
}

fn recording_static_context_path(
    spool_root: &Path,
    capture_relative: &str,
    recording_id: RecordingId,
) -> Result<PathBuf> {
    let capture_path = confined_layer_path(spool_root, capture_relative)?;
    let context_path = ingest_recording_static_context_path(&capture_path, recording_id)?;
    ensure!(
        context_path.starts_with(spool_root),
        "recording static context escapes the configured spool root"
    );
    Ok(context_path)
}

fn visible(recording: &RecordingRecord, identity: &GatewayInternalIdentity) -> bool {
    labels_visible(
        recording,
        identity
            .actor
            .data_labels
            .iter()
            .map(|label| label.as_str()),
    )
}

pub(super) fn labels_visible<'a>(
    recording: &RecordingRecord,
    clearance: impl IntoIterator<Item = &'a str>,
) -> bool {
    let clearance: BTreeSet<&str> = clearance.into_iter().collect();
    recording
        .labels
        .iter()
        .all(|label| clearance.contains(label.as_str()))
}

fn ensure_seal_scope(identity: &GatewayInternalIdentity) -> Result<()> {
    ensure!(
        identity
            .actor
            .scopes
            .iter()
            .any(|scope| scope.as_str() == "admin:manage"),
        "admin:manage scope is required to seal recordings"
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

fn layer_view(layer: &RecordingLayerRecord) -> Result<LayerView> {
    Ok(LayerView {
        layer_id: record_uuid(&layer.id, "recording_layer")?.to_string(),
        layer_name: layer.layer_name.clone(),
        kind: layer_kind(layer.kind).to_owned(),
        ordinal: layer.ordinal,
        state: layer_state(layer.state).to_owned(),
        byte_len: layer.byte_len,
        message_count: layer.message_count,
        sha256: layer.sha256.clone(),
        artifact_uri: layer.artifact.as_ref().map(|artifact| {
            artifact_uri(PlatformArtifactId::from_uuid(
                record_uuid(artifact, "artifact_occurrence")
                    .expect("validated platform artifact record"),
            ))
        }),
        rrd_version: layer.rrd_version.clone(),
        schema_digest: layer.schema_digest.clone(),
        created_at: layer.created_at.to_rfc3339(),
        updated_at: layer.updated_at.to_rfc3339(),
    })
}

fn manifest_layer(layer: &RecordingLayerRecord) -> Result<ManifestLayer> {
    let artifact = layer
        .artifact
        .as_ref()
        .context("committed layer has no Artifact occurrence")?;
    Ok(ManifestLayer {
        layer_id: record_uuid(&layer.id, "recording_layer")?.to_string(),
        layer_name: layer.layer_name.clone(),
        kind: layer_kind(layer.kind).to_owned(),
        ordinal: layer.ordinal,
        byte_len: layer.byte_len,
        sha256: layer
            .sha256
            .clone()
            .context("committed layer has no digest")?,
        artifact_uri: artifact_uri(PlatformArtifactId::from_uuid(record_uuid(
            artifact,
            "artifact_occurrence",
        )?)),
        rrd_version: layer.rrd_version.clone(),
        schema_digest: layer.schema_digest.clone(),
    })
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

pub fn parse_recording_id(value: &str) -> Result<RecordingId> {
    let value = uuid::Uuid::parse_str(value).context("recording_id must be a UUIDv7")?;
    ensure!(
        value.get_version_num() == 7,
        "recording_id must be a UUIDv7"
    );
    Ok(RecordingId::from_uuid(value))
}

pub(super) fn record_uuid(record: &RecordId, table: &str) -> Result<uuid::Uuid> {
    ensure!(
        record.table.as_str() == table,
        "record has unexpected table"
    );
    let raw = match &record.key {
        RecordIdKey::Uuid(value) => value.to_string(),
        RecordIdKey::String(value) => value.clone(),
        other => anyhow::bail!("record key is not UUID: {other:?}"),
    };
    let value = uuid::Uuid::parse_str(&raw)?;
    ensure!(value.get_version_num() == 7, "record key is not UUIDv7");
    Ok(value)
}

fn artifact_uri(id: PlatformArtifactId) -> String {
    format!("artifact://{id}")
}

fn recording_state(state: RecordingState) -> &'static str {
    match state {
        RecordingState::Live => "live",
        RecordingState::Ready => "ready",
        RecordingState::Sealing => "sealing",
        RecordingState::Sealed => "sealed",
        RecordingState::Interrupted => "interrupted",
        RecordingState::Failed => "failed",
    }
}

fn layer_kind(kind: RecordingLayerKind) -> &'static str {
    match kind {
        RecordingLayerKind::Capture => "capture",
        RecordingLayerKind::Properties => "properties",
        RecordingLayerKind::Derived => "derived",
    }
}

fn layer_kind_order(kind: RecordingLayerKind) -> u8 {
    match kind {
        RecordingLayerKind::Properties => 0,
        RecordingLayerKind::Capture => 1,
        RecordingLayerKind::Derived => 2,
    }
}

fn layer_state(state: RecordingLayerState) -> &'static str {
    match state {
        RecordingLayerState::Writing => "writing",
        RecordingLayerState::Staged => "staged",
        RecordingLayerState::Committed => "committed",
        RecordingLayerState::Failed => "failed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn external_recording_ids_require_uuid_v7() {
        assert!(parse_recording_id(&uuid::Uuid::now_v7().to_string()).is_ok());
        assert!(parse_recording_id(&uuid::Uuid::new_v4().to_string()).is_err());
    }

    #[test]
    fn live_layer_path_authorizes_confined_parts_before_rollover() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let relative = "recordings/live.ingest-stream-r0.rrd";
        let final_path = root.join(relative);
        let parts = ingest_segment_parts_directory(&final_path);
        fs::create_dir_all(&parts).unwrap();

        assert_eq!(
            authorized_live_layer_path(&root, relative).unwrap(),
            final_path
        );
    }

    #[test]
    fn live_layer_path_rejects_traversal() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();

        assert!(authorized_live_layer_path(&root, "../outside.rrd").is_err());
        assert!(authorized_live_layer_path(&root, "/outside.rrd").is_err());
    }

    #[test]
    fn sealed_static_context_path_is_confined_to_the_spool() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().canonicalize().unwrap();
        let recording_id = RecordingId::new();
        let stream_id = uuid::Uuid::now_v7();
        let relative = format!("recordings/2026-08-27/source.ingest-{stream_id}-s0.rrd");

        assert_eq!(
            recording_static_context_path(&root, &relative, recording_id).unwrap(),
            root.join("recordings")
                .join(format!(".recording-{recording_id}.static-context"))
        );
        assert!(recording_static_context_path(&root, "../outside.rrd", recording_id).is_err());
    }
}
