use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    LiveCameraHealth, LiveCameraId, LiveCameraSource, LiveSessionId, LiveViewConnection,
    LiveViewId, LiveViewOwner, LiveViewState, WorldFrameUri, parse_artifact_plane_uri,
};
use veoveo_simulation_pose::{EntityId, EpochId, FrameRevision, Sha256Digest};

pub const SCENE_SCHEMA: &str = "veoveo.io/simulation-view-scene/v1";

fn validate_id(value: &str) -> Result<(), SimulationViewError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(SimulationViewError::InvalidIdentifier(value.to_owned()));
    }
    Ok(())
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, SimulationViewError> {
                let value = value.into();
                validate_id(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = SimulationViewError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = SimulationViewError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

id_type!(PrototypeId);
id_type!(ProducerId);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Vector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vector3 {
    fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuaternionXyzw {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalTransform {
    pub translation_m: Vector3,
    pub orientation_xyzw: QuaternionXyzw,
    pub scale: Vector3,
}

impl LocalTransform {
    pub fn validate(&self) -> Result<(), SimulationViewError> {
        let quaternion = [
            self.orientation_xyzw.x,
            self.orientation_xyzw.y,
            self.orientation_xyzw.z,
            self.orientation_xyzw.w,
        ];
        let norm = quaternion
            .into_iter()
            .map(|value| value * value)
            .sum::<f64>();
        if !self.translation_m.finite()
            || !self.scale.finite()
            || self.scale.x <= 0.0
            || self.scale.y <= 0.0
            || self.scale.z <= 0.0
            || quaternion.into_iter().any(|value| !value.is_finite())
            || (norm - 1.0).abs() > 1.0e-3
        {
            return Err(SimulationViewError::InvalidTransform);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VisualAssetFormat {
    Usd,
    Usdz,
    Glb,
    Gltf,
    Ktx2,
    Png,
    Jpeg,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GovernedArtifact {
    pub artifact_uri: String,
    pub digest: Sha256Digest,
    pub format: VisualAssetFormat,
    pub byte_length: u64,
}

impl GovernedArtifact {
    pub fn validate(&self) -> Result<(), SimulationViewError> {
        if self.artifact_uri.len() > 512
            || parse_artifact_plane_uri(&self.artifact_uri).is_none()
            || self.byte_length == 0
        {
            return Err(SimulationViewError::InvalidArtifact);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisualPrototype {
    pub prototype_id: PrototypeId,
    pub asset: GovernedArtifact,
    pub local_alignment: LocalTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneEntity {
    pub entity_id: EntityId,
    pub prototype_id: PrototypeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_transform: Option<LocalTransform>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneAttribution {
    pub source: String,
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribution_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneLighting {
    pub intensity_lux: f32,
    pub color_temperature_kelvin: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RendererMode {
    RaytracedLighting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InterpolationPolicy {
    HoldLatest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneQualityPolicy {
    pub renderer: RendererMode,
    pub maximum_texture_dimension: u32,
    pub maximum_asset_bytes: u64,
    pub interpolation: InterpolationPolicy,
    pub maximum_pose_age_ms: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneDeclarationBody {
    pub schema_version: String,
    pub session_id: LiveSessionId,
    pub epoch_id: EpochId,
    pub frame_revision: FrameRevision,
    pub simulation_frame: WorldFrameUri,
    pub environment: GovernedArtifact,
    pub prototypes: Vec<VisualPrototype>,
    pub entities: Vec<SceneEntity>,
    pub allowed_camera_kinds: Vec<LiveCameraSource>,
    pub lighting: SceneLighting,
    pub quality: SceneQualityPolicy,
    pub attribution: Vec<SceneAttribution>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SceneDeclaration {
    pub body: SceneDeclarationBody,
    pub digest: Sha256Digest,
}

impl SceneDeclaration {
    pub fn validate(
        &self,
        maximum_entities: u32,
        maximum_asset_bytes: u64,
    ) -> Result<(), SimulationViewError> {
        if self.body.schema_version != SCENE_SCHEMA
            || self.body.prototypes.is_empty()
            || self.body.entities.is_empty()
            || self.body.entities.len() > maximum_entities as usize
            || self.body.allowed_camera_kinds.is_empty()
            || self.body.attribution.is_empty()
            || self.body.lighting.intensity_lux <= 0.0
            || !self.body.lighting.intensity_lux.is_finite()
            || !(1_000..=20_000).contains(&self.body.lighting.color_temperature_kelvin)
            || self.body.quality.maximum_pose_age_ms == 0
            || self.body.quality.maximum_texture_dimension == 0
            || self.body.quality.maximum_asset_bytes == 0
            || self.body.quality.maximum_asset_bytes > maximum_asset_bytes
        {
            return Err(SimulationViewError::InvalidScene);
        }
        self.body
            .frame_revision
            .validate()
            .map_err(|_| SimulationViewError::InvalidScene)?;
        if self.body.simulation_frame.revision_uri().as_str() != self.body.frame_revision.uri {
            return Err(SimulationViewError::InvalidScene);
        }
        self.body.environment.validate()?;
        if !matches!(
            self.body.environment.format,
            VisualAssetFormat::Usd
                | VisualAssetFormat::Usdz
                | VisualAssetFormat::Glb
                | VisualAssetFormat::Gltf
        ) {
            return Err(SimulationViewError::InvalidArtifact);
        }
        let mut camera_kinds = std::collections::BTreeSet::new();
        if self
            .body
            .allowed_camera_kinds
            .iter()
            .any(|kind| !camera_kinds.insert(*kind))
        {
            return Err(SimulationViewError::InvalidScene);
        }
        for attribution in &self.body.attribution {
            if attribution.source.trim().is_empty()
                || attribution.license.trim().is_empty()
                || attribution
                    .attribution_url
                    .as_ref()
                    .is_some_and(|url| !is_https_url(url))
            {
                return Err(SimulationViewError::InvalidScene);
            }
        }
        let mut prototypes = std::collections::BTreeSet::new();
        let mut total_bytes = self.body.environment.byte_length;
        for prototype in &self.body.prototypes {
            if !prototypes.insert(prototype.prototype_id.clone()) {
                return Err(SimulationViewError::DuplicatePrototype);
            }
            prototype.asset.validate()?;
            if !matches!(
                prototype.asset.format,
                VisualAssetFormat::Usd
                    | VisualAssetFormat::Usdz
                    | VisualAssetFormat::Glb
                    | VisualAssetFormat::Gltf
            ) {
                return Err(SimulationViewError::InvalidArtifact);
            }
            prototype.local_alignment.validate()?;
            total_bytes = total_bytes
                .checked_add(prototype.asset.byte_length)
                .ok_or(SimulationViewError::InvalidArtifact)?;
        }
        if total_bytes > self.body.quality.maximum_asset_bytes || total_bytes > maximum_asset_bytes
        {
            return Err(SimulationViewError::InvalidArtifact);
        }
        let mut entities = std::collections::BTreeSet::new();
        for entity in &self.body.entities {
            if !entities.insert(entity.entity_id.clone())
                || !prototypes.contains(&entity.prototype_id)
            {
                return Err(SimulationViewError::InvalidEntity);
            }
            if let Some(transform) = entity.static_transform {
                transform.validate()?;
            }
        }
        let canonical =
            serde_json::to_vec(&self.body).map_err(|_| SimulationViewError::InvalidScene)?;
        let computed = Sha256Digest::from_bytes(Sha256::digest(canonical).into());
        if computed != self.digest {
            return Err(SimulationViewError::SceneDigest);
        }
        Ok(())
    }

    pub fn from_body(body: SceneDeclarationBody) -> Result<Self, SimulationViewError> {
        let canonical = serde_json::to_vec(&body).map_err(|_| SimulationViewError::InvalidScene)?;
        Ok(Self {
            body,
            digest: Sha256Digest::from_bytes(Sha256::digest(canonical).into()),
        })
    }
}

fn is_https_url(value: &str) -> bool {
    value.strip_prefix("https://").is_some_and(|authority| {
        !authority.is_empty() && !authority.chars().any(char::is_whitespace)
    }) && value.len() <= 2048
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionLifecycle {
    Created,
    SceneBound,
    Ready,
    Closing,
    Closed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoseSourceState {
    pub producer_id: ProducerId,
    pub spiffe_id: String,
    pub authorized_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_snapshot_at: Option<DateTime<Utc>>,
    pub stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationViewSession {
    pub session_id: LiveSessionId,
    pub epoch_id: EpochId,
    pub owner: LiveViewOwner,
    pub lifecycle: SessionLifecycle,
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<SceneDeclaration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pose_source: Option<PoseSourceState>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateSessionRequest {
    pub session_id: LiveSessionId,
    pub epoch_id: EpochId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetSessionStateRequest {
    pub session_id: LiveSessionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetCapacityRequest {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BindSceneRequest {
    pub session_id: LiveSessionId,
    pub expected_revision: u64,
    pub scene: SceneDeclaration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseSessionRequest {
    pub session_id: LiveSessionId,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthorizePoseProducerRequest {
    pub session_id: LiveSessionId,
    pub expected_revision: u64,
    pub producer_id: ProducerId,
    pub spiffe_id: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokePoseProducerRequest {
    pub session_id: LiveSessionId,
    pub expected_revision: u64,
    pub producer_id: ProducerId,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalPose {
    pub position_m: Vector3,
    pub orientation_xyzw: QuaternionXyzw,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum CameraRig {
    Fixed {
        pose: LocalPose,
    },
    LookAt {
        eye_m: Vector3,
        target_m: Vector3,
    },
    Orbit {
        target_entity: EntityId,
        radius_m: f32,
        azimuth_degrees: f32,
        elevation_degrees: f32,
    },
    FollowEntity {
        target_entity: EntityId,
        offset_flu_m: Vector3,
        smoothing_seconds: f32,
    },
    ChaseEntity {
        target_entity: EntityId,
        distance_m: f32,
        height_m: f32,
        smoothing_seconds: f32,
    },
    MountedEntity {
        target_entity: EntityId,
        mount: LocalTransform,
    },
    FormationOverview {
        target_entities: Vec<EntityId>,
        padding_m: f32,
    },
}

impl CameraRig {
    pub fn source(&self) -> LiveCameraSource {
        match self {
            Self::Fixed { .. } => LiveCameraSource::Fixed,
            Self::LookAt { .. } => LiveCameraSource::LookAt,
            Self::Orbit { .. } => LiveCameraSource::Orbit,
            Self::FollowEntity { .. } => LiveCameraSource::FollowEntity,
            Self::ChaseEntity { .. } => LiveCameraSource::ChaseEntity,
            Self::MountedEntity { .. } => LiveCameraSource::MountedEntity,
            Self::FormationOverview { .. } => LiveCameraSource::FormationOverview,
        }
    }

    pub fn validate(&self) -> Result<(), SimulationViewError> {
        let valid = match self {
            Self::Fixed { pose } => {
                pose.position_m.finite()
                    && LocalTransform {
                        translation_m: pose.position_m,
                        orientation_xyzw: pose.orientation_xyzw,
                        scale: Vector3 {
                            x: 1.0,
                            y: 1.0,
                            z: 1.0,
                        },
                    }
                    .validate()
                    .is_ok()
            }
            Self::LookAt { eye_m, target_m } => {
                eye_m.finite() && target_m.finite() && eye_m != target_m
            }
            Self::Orbit {
                radius_m,
                azimuth_degrees,
                elevation_degrees,
                ..
            } => {
                radius_m.is_finite()
                    && *radius_m > 0.1
                    && azimuth_degrees.is_finite()
                    && elevation_degrees.is_finite()
                    && (-89.9..=89.9).contains(elevation_degrees)
            }
            Self::FollowEntity {
                offset_flu_m,
                smoothing_seconds,
                ..
            } => {
                offset_flu_m.finite() && smoothing_seconds.is_finite() && *smoothing_seconds >= 0.0
            }
            Self::ChaseEntity {
                distance_m,
                height_m,
                smoothing_seconds,
                ..
            } => {
                distance_m.is_finite()
                    && *distance_m > 0.1
                    && height_m.is_finite()
                    && smoothing_seconds.is_finite()
                    && *smoothing_seconds >= 0.0
            }
            Self::MountedEntity { mount, .. } => mount.validate().is_ok(),
            Self::FormationOverview {
                target_entities,
                padding_m,
            } => {
                !target_entities.is_empty()
                    && target_entities.len() <= 256
                    && target_entities.windows(2).all(|pair| pair[0] < pair[1])
                    && padding_m.is_finite()
                    && *padding_m >= 0.0
            }
        };
        valid
            .then_some(())
            .ok_or(SimulationViewError::InvalidCameraRig)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CameraStreamPolicy {
    Disabled,
    OnDemand,
    Continuous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CameraRecordingPolicy {
    Disabled,
    OnCapture,
    Continuous,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CameraDefinition {
    pub rig: CameraRig,
    pub width_px: u32,
    pub height_px: u32,
    pub frame_rate_millihertz: u32,
    pub vertical_fov_degrees: f32,
    pub near_clip_m: f32,
    pub far_clip_m: f32,
    pub stream_policy: CameraStreamPolicy,
    pub recording_policy: CameraRecordingPolicy,
}

impl CameraDefinition {
    pub fn validate(&self) -> Result<(), SimulationViewError> {
        self.rig.validate()?;
        if self.width_px == 0
            || self.height_px == 0
            || self.frame_rate_millihertz == 0
            || !self.vertical_fov_degrees.is_finite()
            || !(1.0..=160.0).contains(&self.vertical_fov_degrees)
            || !self.near_clip_m.is_finite()
            || !self.far_clip_m.is_finite()
            || self.near_clip_m <= 0.0
            || self.far_clip_m <= self.near_clip_m
        {
            return Err(SimulationViewError::InvalidCamera);
        }
        Ok(())
    }

    pub fn pixels_per_second(&self) -> u64 {
        u64::from(self.width_px)
            .saturating_mul(u64::from(self.height_px))
            .saturating_mul(u64::from(self.frame_rate_millihertz))
            / 1_000
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CameraRecord {
    pub camera_id: LiveCameraId,
    pub session_id: LiveSessionId,
    pub owner: LiveViewOwner,
    pub revision: u64,
    pub definition: CameraDefinition,
    pub health: LiveCameraHealth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_pose_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_frame_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateCameraRequest {
    pub session_id: LiveSessionId,
    pub definition: CameraDefinition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetCameraRequest {
    pub session_id: LiveSessionId,
    pub camera_id: LiveCameraId,
    pub expected_revision: u64,
    pub definition: CameraDefinition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseCameraRequest {
    pub session_id: LiveSessionId,
    pub camera_id: LiveCameraId,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CapacityDimension {
    LogicalCameras,
    RenderedCameras,
    StreamedCameras,
    RenderPixelsPerSecond,
    NvencSessions,
    GpuMemoryBytes,
    EntityInstances,
    OwnerQuota,
    WorkContextQuota,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapacityRejection {
    pub dimension: CapacityDimension,
    pub requested: u64,
    pub available: u64,
    pub profile: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CameraAdmission {
    Admitted { camera: Box<CameraRecord> },
    Rejected { rejection: CapacityRejection },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapacityProfile {
    pub profile: String,
    pub maximum_logical_cameras: u32,
    pub maximum_rendered_cameras: u32,
    pub maximum_streamed_cameras: u32,
    pub maximum_render_pixels_per_second: u64,
    pub maximum_nvenc_sessions: u32,
    pub gpu_memory_budget_bytes: u64,
    pub maximum_entity_instances: u32,
    pub maximum_cameras_per_owner: u32,
    pub maximum_cameras_per_work_context: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapacityUsage {
    pub logical_cameras: u32,
    pub rendered_cameras: u32,
    pub streamed_cameras: u32,
    pub render_pixels_per_second: u64,
    pub nvenc_sessions: u32,
    pub reserved_gpu_memory_bytes: u64,
    pub entity_instances: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapacityState {
    pub limits: CapacityProfile,
    pub usage: CapacityUsage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenLiveViewRequest {
    pub session_id: LiveSessionId,
    pub camera_id: LiveCameraId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenewLiveViewRequest {
    pub session_id: LiveSessionId,
    pub live_view_id: LiveViewId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseLiveViewRequest {
    pub session_id: LiveSessionId,
    pub live_view_id: LiveViewId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseResult {
    pub resource_uri: String,
    pub closed: bool,
}

pub type LiveViewRecord = LiveViewState;
pub type OpenLiveViewResult = LiveViewConnection;

#[derive(Debug, thiserror::Error)]
pub enum SimulationViewError {
    #[error("invalid simulation-view identifier {0:?}")]
    InvalidIdentifier(String),
    #[error("invalid local transform")]
    InvalidTransform,
    #[error("invalid governed artifact")]
    InvalidArtifact,
    #[error("invalid scene declaration")]
    InvalidScene,
    #[error("duplicate visual prototype")]
    DuplicatePrototype,
    #[error("invalid scene entity or prototype binding")]
    InvalidEntity,
    #[error("scene declaration digest does not match its canonical body")]
    SceneDigest,
    #[error("invalid camera rig")]
    InvalidCameraRig,
    #[error("invalid camera definition")]
    InvalidCamera,
    #[error("session {0} was not found")]
    SessionNotFound(LiveSessionId),
    #[error("session {0} already exists with a different epoch")]
    SessionAlreadyExists(LiveSessionId),
    #[error("camera {0} was not found")]
    CameraNotFound(LiveCameraId),
    #[error("live view {0} was not found")]
    LiveViewNotFound(LiveViewId),
    #[error("caller does not own the requested resource")]
    Ownership,
    #[error("expected revision {expected}, current revision is {actual}")]
    Revision { expected: u64, actual: u64 },
    #[error("session lifecycle does not permit this operation")]
    Lifecycle,
    #[error("session scene is already bound to a different declaration")]
    SceneAlreadyBound,
    #[error("camera rig kind is not admitted by the scene")]
    CameraKind,
    #[error("pose producer identity is invalid")]
    Producer,
    #[error("live view token is invalid or expired")]
    Access,
    #[error("system time overflow")]
    Time,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn camera_rig_uses_camel_case_fields_in_json_and_schema() {
        let rig = CameraRig::FollowEntity {
            target_entity: EntityId::new("entity-1").unwrap(),
            offset_flu_m: Vector3 {
                x: -6.0,
                y: 0.0,
                z: 2.5,
            },
            smoothing_seconds: 0.15,
        };
        let value = serde_json::to_value(&rig).unwrap();
        assert_eq!(
            value
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["kind", "offsetFluM", "smoothingSeconds", "targetEntity",])
        );
        assert_eq!(value["kind"], "follow_entity");
        assert_eq!(value["targetEntity"], "entity-1");
        assert_eq!(
            value["offsetFluM"],
            serde_json::json!({"x": -6.0, "y": 0.0, "z": 2.5})
        );
        assert_eq!(serde_json::from_value::<CameraRig>(value).unwrap(), rig);

        let schema = serde_json::to_string(&schemars::schema_for!(CameraRig)).unwrap();
        for field in [
            "eyeM",
            "targetM",
            "targetEntity",
            "radiusM",
            "azimuthDegrees",
            "elevationDegrees",
            "offsetFluM",
            "smoothingSeconds",
            "distanceM",
            "heightM",
            "targetEntities",
            "paddingM",
        ] {
            assert!(schema.contains(&format!("\"{field}\"")));
        }
        for field in [
            "eye_m",
            "target_m",
            "target_entity",
            "radius_m",
            "azimuth_degrees",
            "elevation_degrees",
            "offset_flu_m",
            "smoothing_seconds",
            "distance_m",
            "height_m",
            "target_entities",
            "padding_m",
        ] {
            assert!(!schema.contains(&format!("\"{field}\"")));
        }
    }

    #[test]
    fn python_scene_fixture_uses_the_rust_canonical_digest() {
        let body: SceneDeclarationBody = serde_json::from_str(include_str!(
            "../../../platform/simulation/fixtures/anonymous-scene-body.json"
        ))
        .unwrap();
        let declaration = SceneDeclaration::from_body(body).unwrap();
        assert_eq!(
            declaration.digest.as_str(),
            "sha256:67291c10c39898b2ea11ac9bbe12643148b0112bd868ed44579aaa818fea48e4"
        );
    }
}
