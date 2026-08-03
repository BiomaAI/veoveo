use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{
    ArtifactPlane, LiveCameraSource, LiveSessionId, LiveViewOwner, PlaneCaller, PutArtifactRequest,
};
use veoveo_simulation_pose::{EntityId, FrameRevision, Sha256Digest, entity_identity_table_digest};
use veoveo_simulation_scene::{
    GeospatialLayerId, GovernedArtifact, InterpolationPolicy, LocalTransform, PrototypeId,
    QuaternionXyzw, RendererMode, SCENE_SCHEMA, SceneAttribution, SceneDeclaration,
    SceneDeclarationBody, SceneEntity, SceneLighting, SceneQualityPolicy, Vector3,
    VisualAssetFormat, VisualPrototype,
};

const PHOTOREALISTIC_DAYLIGHT_INTENSITY_LUX: f32 = 20_000.0;
const PHOTOREALISTIC_DAYLIGHT_TEMPERATURE_KELVIN: u32 = 5_500;

use crate::contract::{PreparedViewScene, SessionId, SimulationState};

const ASSET_SCHEMA: &str = "veoveo.io/uav-simulation-view-asset/v1";
const ENVIRONMENT_FILENAME: &str = "uav-reference-environment.usda";
const PROTOTYPE_FILENAME: &str = "uav-prototype.usda";
const ENVIRONMENT_USDA: &[u8] = include_bytes!("../assets/view/environment.usda");
const PROTOTYPE_USDA: &[u8] = include_bytes!("../assets/view/uav.usda");
const MAXIMUM_TEXTURE_DIMENSION: u32 = 4096;
const MAXIMUM_POSE_AGE_MS: u32 = 500;

type PreparedKey = (String, SessionId);

#[derive(Clone)]
pub(crate) struct ViewSceneService {
    artifacts: HttpArtifactPlane,
    prepared: Arc<RwLock<BTreeMap<PreparedKey, PreparedViewScene>>>,
}

impl ViewSceneService {
    pub(crate) fn new(artifact_service_url: impl Into<String>) -> Self {
        Self {
            artifacts: HttpArtifactPlane::new(artifact_service_url),
            prepared: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub(crate) async fn prepare(
        &self,
        caller: &PlaneCaller,
        state: &SimulationState,
        geospatial_layer_id: Option<GeospatialLayerId>,
    ) -> Result<PreparedViewScene> {
        let key = prepared_key(caller, &state.session_id)?;
        if let Some(prepared) = self.prepared.read().await.get(&key)
            && prepared_matches_state(prepared, state)
            && prepared.scene.body.geospatial_layer_id == geospatial_layer_id
        {
            return Ok(prepared.clone());
        }

        validate_scene_state(state)?;

        let (environment, prototype) = tokio::try_join!(
            self.publish_asset(
                caller,
                ENVIRONMENT_FILENAME,
                "environment",
                ENVIRONMENT_USDA
            ),
            self.publish_asset(
                caller,
                PROTOTYPE_FILENAME,
                "vehicle_prototype",
                PROTOTYPE_USDA
            ),
        )?;
        let prepared = build_prepared_scene(state, environment, prototype, geospatial_layer_id)?;
        self.prepared.write().await.insert(key, prepared.clone());
        Ok(prepared)
    }

    pub(crate) async fn get(
        &self,
        caller: &PlaneCaller,
        session_id: &SessionId,
    ) -> Result<Option<PreparedViewScene>> {
        let key = prepared_key(caller, session_id)?;
        Ok(self.prepared.read().await.get(&key).cloned())
    }

    pub(crate) async fn sessions_for(&self, caller: &PlaneCaller) -> Result<Vec<SessionId>> {
        let owner = owner_key(caller)?;
        Ok(self
            .prepared
            .read()
            .await
            .keys()
            .filter(|(candidate, _)| candidate == &owner)
            .map(|(_, session_id)| session_id.clone())
            .collect())
    }

    async fn publish_asset(
        &self,
        caller: &PlaneCaller,
        filename: &str,
        role: &str,
        bytes: &'static [u8],
    ) -> Result<GovernedArtifact> {
        let digest = Sha256Digest::from_bytes(Sha256::digest(bytes).into());
        let metadata = self
            .artifacts
            .put(
                caller,
                PutArtifactRequest {
                    mime_type: Some("model/vnd.usd".to_owned()),
                    filename: Some(filename.to_owned()),
                    classification: None,
                    data_labels: BTreeSet::new(),
                    retention_expires_at: None,
                    metadata: serde_json::json!({
                        "schema": ASSET_SCHEMA,
                        "role": role,
                        "sha256": digest.as_str(),
                    }),
                },
                bytes.to_vec(),
            )
            .await
            .with_context(|| format!("publishing governed UAV view asset {filename}"))?;
        ensure!(
            metadata.byte_len == bytes.len() as u64,
            "artifact service changed the UAV view asset byte length"
        );
        Ok(GovernedArtifact {
            artifact_uri: metadata.artifact_uri,
            digest,
            format: VisualAssetFormat::Usd,
            byte_length: metadata.byte_len,
        })
    }
}

fn build_prepared_scene(
    state: &SimulationState,
    environment: GovernedArtifact,
    prototype: GovernedArtifact,
    geospatial_layer_id: Option<GeospatialLayerId>,
) -> Result<PreparedViewScene> {
    let (world, entity_ids) = validate_scene_state(state)?;
    let prototype_id = PrototypeId::new("uav")?;
    let maximum_asset_bytes = environment
        .byte_length
        .checked_add(prototype.byte_length)
        .context("view-scene asset size overflow")?;
    let frame_revision = FrameRevision {
        uri: world.revision_uri.to_string(),
        digest: Sha256Digest::new(format!("sha256:{}", world.spec_sha256))?,
    };
    let scene = SceneDeclaration::from_body(SceneDeclarationBody {
        schema_version: SCENE_SCHEMA.to_owned(),
        session_id: LiveSessionId::new(state.session_id.as_str())?,
        epoch_id: state.pose_publication.epoch_id.clone(),
        frame_revision,
        simulation_frame: world.simulation_frame_uri.clone(),
        geospatial_layer_id,
        environment,
        prototypes: vec![VisualPrototype {
            prototype_id: prototype_id.clone(),
            asset: prototype,
            local_alignment: identity_transform(),
        }],
        entities: entity_ids
            .into_iter()
            .map(|entity_id| SceneEntity {
                entity_id,
                prototype_id: prototype_id.clone(),
                static_transform: None,
            })
            .collect(),
        allowed_camera_kinds: vec![
            LiveCameraSource::Orbit,
            LiveCameraSource::FollowEntity,
            LiveCameraSource::ChaseEntity,
            LiveCameraSource::MountedEntity,
            LiveCameraSource::FormationOverview,
        ],
        lighting: SceneLighting {
            intensity_lux: PHOTOREALISTIC_DAYLIGHT_INTENSITY_LUX,
            color_temperature_kelvin: PHOTOREALISTIC_DAYLIGHT_TEMPERATURE_KELVIN,
        },
        quality: SceneQualityPolicy {
            renderer: RendererMode::RaytracedLighting,
            maximum_texture_dimension: MAXIMUM_TEXTURE_DIMENSION,
            maximum_asset_bytes,
            interpolation: InterpolationPolicy::Linear,
            maximum_pose_age_ms: MAXIMUM_POSE_AGE_MS,
        },
        attribution: vec![SceneAttribution {
            source: "VeoVeo UAV showcase".to_owned(),
            license: "Apache-2.0".to_owned(),
            attribution_url: Some("https://www.apache.org/licenses/LICENSE-2.0".to_owned()),
        }],
    })?;
    scene.validate(state.vehicles.len() as u32, maximum_asset_bytes)?;
    Ok(PreparedViewScene {
        scene,
        producer_id: state.pose_publication.producer_id.clone(),
        producer_spiffe_id: state.pose_publication.producer_spiffe_id.clone(),
        entity_table_revision: state.pose_publication.entity_table_revision,
        entity_table_digest: state.pose_publication.entity_table_digest.clone(),
    })
}

fn validate_scene_state(
    state: &SimulationState,
) -> Result<(&crate::contract::SimulationWorldBinding, Vec<EntityId>)> {
    let world = state
        .world
        .as_ref()
        .context("simulation world must be configured before preparing its view scene")?;
    let entity_ids = state
        .vehicles
        .iter()
        .map(|vehicle| EntityId::new(vehicle.vehicle_id.as_str()))
        .collect::<Result<Vec<_>, _>>()?;
    ensure!(
        !entity_ids.is_empty(),
        "view scene requires at least one UAV"
    );
    ensure!(
        entity_ids.windows(2).all(|pair| pair[0] < pair[1]),
        "UAV entity identities must be strictly ordered"
    );
    let computed_entity_digest =
        entity_identity_table_digest(state.pose_publication.entity_table_revision, &entity_ids);
    ensure!(
        computed_entity_digest == state.pose_publication.entity_table_digest,
        "UAV vehicle inventory does not match the published pose entity table"
    );
    Ok((world, entity_ids))
}

fn prepared_key(caller: &PlaneCaller, session_id: &SessionId) -> Result<PreparedKey> {
    Ok((owner_key(caller)?, session_id.clone()))
}

fn owner_key(caller: &PlaneCaller) -> Result<String> {
    serde_json::to_string(&LiveViewOwner::from_identity(&caller.identity))
        .context("serializing the governed view-scene owner")
}

fn prepared_matches_state(prepared: &PreparedViewScene, state: &SimulationState) -> bool {
    let Some(world) = &state.world else {
        return false;
    };
    prepared.scene.body.session_id.as_str() == state.session_id.as_str()
        && prepared.scene.body.epoch_id == state.pose_publication.epoch_id
        && prepared.scene.body.frame_revision.uri == world.revision_uri.as_str()
        && prepared.scene.body.simulation_frame == world.simulation_frame_uri
        && prepared.producer_id == state.pose_publication.producer_id
        && prepared.producer_spiffe_id == state.pose_publication.producer_spiffe_id
        && prepared.entity_table_revision == state.pose_publication.entity_table_revision
        && prepared.entity_table_digest == state.pose_publication.entity_table_digest
}

fn identity_transform() -> LocalTransform {
    LocalTransform {
        translation_m: Vector3 {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        orientation_xyzw: QuaternionXyzw {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        },
        scale: Vector3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_mcp_contract::ArtifactId;

    fn governed(bytes: &[u8]) -> GovernedArtifact {
        GovernedArtifact {
            artifact_uri: ArtifactId::new().plane_uri(),
            digest: Sha256Digest::from_bytes(Sha256::digest(bytes).into()),
            format: VisualAssetFormat::Usd,
            byte_length: bytes.len() as u64,
        }
    }

    #[test]
    fn prepared_scene_is_bound_to_the_authoritative_pose_table() {
        let state = crate::server::fake_state().unwrap();
        let prepared = build_prepared_scene(
            &state,
            governed(ENVIRONMENT_USDA),
            governed(PROTOTYPE_USDA),
            None,
        )
        .unwrap();
        assert_eq!(
            prepared.scene.body.frame_revision.uri,
            state.world.as_ref().unwrap().revision_uri.as_str()
        );
        assert_eq!(
            prepared.scene.body.entities[0].entity_id.as_str(),
            state.vehicles[0].vehicle_id.as_str()
        );
        assert_eq!(
            prepared.entity_table_digest,
            state.pose_publication.entity_table_digest
        );
        assert_eq!(
            prepared.scene.body.allowed_camera_kinds,
            [
                LiveCameraSource::Orbit,
                LiveCameraSource::FollowEntity,
                LiveCameraSource::ChaseEntity,
                LiveCameraSource::MountedEntity,
                LiveCameraSource::FormationOverview,
            ]
        );
        assert_eq!(
            prepared.scene.body.lighting,
            SceneLighting {
                intensity_lux: PHOTOREALISTIC_DAYLIGHT_INTENSITY_LUX,
                color_temperature_kelvin: PHOTOREALISTIC_DAYLIGHT_TEMPERATURE_KELVIN,
            }
        );
    }

    #[test]
    fn prepared_scene_rejects_a_pose_table_that_disagrees_with_vehicles() {
        let mut state = crate::server::fake_state().unwrap();
        state.pose_publication.entity_table_digest =
            Sha256Digest::from_bytes(Sha256::digest(b"different").into());
        let error = build_prepared_scene(
            &state,
            governed(ENVIRONMENT_USDA),
            governed(PROTOTYPE_USDA),
            None,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("does not match the published pose entity table")
        );
    }
}
