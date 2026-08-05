use std::{fmt, str::FromStr};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{
    LiveCameraHealth, LiveCameraId, LiveCameraSource, LiveSessionId, LiveViewConnection,
    LiveViewId, LiveViewOwner, LiveViewState,
};
use veoveo_simulation_pose::{EntityId, EpochId};
pub use veoveo_simulation_scene::{
    GeospatialLayerHealth, GeospatialLayerId, GovernedArtifact, InterpolationPolicy,
    LayerFailureCode, LayerFailureDiagnostic, LayerLifecycle, LocalTransform, PrototypeId,
    QuaternionXyzw, RendererMode, SCENE_SCHEMA, SceneAttribution, SceneContractError,
    SceneDeclaration, SceneDeclarationBody, SceneEntity, SceneLighting, SceneQualityPolicy,
    Vector3, VisualAssetFormat, VisualPrototype,
};

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

id_type!(ProducerId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionLifecycle {
    Created,
    SceneBound,
    Ready,
    Closing,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationPhase {
    Pending,
    RendererSession,
    Scene,
    PoseAuthorization,
    Cameras,
    Healthy,
    Blocked,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PoseAuthorizationRenewalState {
    Scheduled,
    Renewing,
    Current,
    Expired,
    Revoked,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationFailureCode {
    StoreUnavailable,
    RendererUnavailable,
    SceneUnavailable,
    PoseIngressUnavailable,
    PoseAuthorizationExpired,
    PoseAuthorizationRevisionConflict,
    PoseProducerIdentityMismatch,
    SceneRevisionMismatch,
    CameraRejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReconciliationStatus {
    pub desired_revision: u64,
    pub realized_revision: u64,
    pub authorization_revision: u64,
    pub phase: ReconciliationPhase,
    pub renewal_state: PoseAuthorizationRenewalState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_id: Option<ProducerId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_spiffe_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_expires_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pose_authorization_renewal_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_successful_reconciliation_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_dependency: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<ReconciliationFailureCode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

impl ReconciliationStatus {
    pub fn pending(revision: u64) -> Self {
        Self {
            desired_revision: revision,
            realized_revision: 0,
            authorization_revision: 0,
            phase: ReconciliationPhase::Pending,
            renewal_state: PoseAuthorizationRenewalState::Scheduled,
            producer_id: None,
            producer_spiffe_id: None,
            authorization_expires_at: None,
            pose_authorization_renewal_at: None,
            retry_at: None,
            last_successful_reconciliation_at: None,
            failed_dependency: None,
            failure_code: None,
            diagnostic: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoseSourceState {
    pub producer_id: ProducerId,
    pub spiffe_id: String,
    pub authorization_revision: u64,
    pub authorization_lifetime_seconds: u64,
    pub authorized_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_snapshot_at: Option<DateTime<Utc>>,
    pub stale: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PoseInterpolationRuntimeState {
    Unavailable,
    Reset,
    Warming,
    HoldLatest,
    Interpolating,
    Holding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PoseInterpolationResetReason {
    PoseSourceChanged,
    AuthorizationRevisionChanged,
    EntityTableChanged,
    SequenceGap,
    SequenceRepeated,
    SequenceReversed,
    TimestampNotIncreasing,
    Stale,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoseInterpolationStatus {
    pub policy: InterpolationPolicy,
    pub state: PoseInterpolationRuntimeState,
    pub previous_source_sequence: Option<u64>,
    pub current_source_sequence: Option<u64>,
    pub previous_simulation_timestamp_ns: Option<i64>,
    pub current_simulation_timestamp_ns: Option<i64>,
    pub rendered_simulation_timestamp_ns: Option<i64>,
    pub interpolation_alpha: Option<f64>,
    pub interpolation_delay_ns: u64,
    pub discontinuity_reset_count: u64,
    pub repeated_source_sample_count: u64,
    pub skipped_source_sample_count: u64,
    pub last_reset_reason: Option<PoseInterpolationResetReason>,
}

impl PoseInterpolationStatus {
    pub fn unavailable(policy: InterpolationPolicy) -> Self {
        Self {
            policy,
            state: PoseInterpolationRuntimeState::Unavailable,
            previous_source_sequence: None,
            current_source_sequence: None,
            previous_simulation_timestamp_ns: None,
            current_simulation_timestamp_ns: None,
            rendered_simulation_timestamp_ns: None,
            interpolation_alpha: None,
            interpolation_delay_ns: 0,
            discontinuity_reset_count: 0,
            repeated_source_sample_count: 0,
            skipped_source_sample_count: 0,
            last_reset_reason: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PoseSourceResource {
    #[serde(flatten)]
    pub source: PoseSourceState,
    pub interpolation: PoseInterpolationStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationViewSession {
    pub session_id: LiveSessionId,
    pub epoch_id: EpochId,
    pub owner: LiveViewOwner,
    pub lifecycle: SessionLifecycle,
    pub revision: u64,
    pub reconciliation: ReconciliationStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<SceneDeclaration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geospatial_layer: Option<GeospatialLayerHealth>,
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
    pub viewer_instance_id: veoveo_mcp_contract::LiveViewerInstanceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenewLiveViewRequest {
    pub session_id: LiveSessionId,
    pub live_view_id: LiveViewId,
    pub viewer_instance_id: veoveo_mcp_contract::LiveViewerInstanceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseLiveViewRequest {
    pub session_id: LiveSessionId,
    pub live_view_id: LiveViewId,
    pub viewer_instance_id: veoveo_mcp_contract::LiveViewerInstanceId,
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
    #[error("geospatial layer binding failed: {0}")]
    GeospatialLayer(String),
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
    #[error("pose producer authorization is revoked")]
    ProducerRevoked,
    #[error("pose producer authorization expired")]
    ProducerAuthorizationExpired,
    #[error("simulation-view reconciliation is blocked at {dependency}: {code:?}")]
    ReconciliationBlocked {
        dependency: String,
        code: ReconciliationFailureCode,
    },
    #[error("live view token is invalid or expired")]
    Access,
    #[error("system time overflow")]
    Time,
}

impl From<SceneContractError> for SimulationViewError {
    fn from(error: SceneContractError) -> Self {
        match error {
            SceneContractError::InvalidIdentifier(value) => Self::InvalidIdentifier(value),
            SceneContractError::InvalidTransform => Self::InvalidTransform,
            SceneContractError::InvalidArtifact => Self::InvalidArtifact,
            SceneContractError::InvalidScene => Self::InvalidScene,
            SceneContractError::DuplicatePrototype => Self::DuplicatePrototype,
            SceneContractError::InvalidEntity => Self::InvalidEntity,
            SceneContractError::SceneDigest => Self::SceneDigest,
        }
    }
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
    fn interpolation_status_is_typed_bounded_and_credential_free() {
        let status = PoseInterpolationStatus {
            policy: InterpolationPolicy::Linear,
            state: PoseInterpolationRuntimeState::Interpolating,
            previous_source_sequence: Some(41),
            current_source_sequence: Some(42),
            previous_simulation_timestamp_ns: Some(2_050_000_000),
            current_simulation_timestamp_ns: Some(2_100_000_000),
            rendered_simulation_timestamp_ns: Some(2_075_000_000),
            interpolation_alpha: Some(0.5),
            interpolation_delay_ns: 50_000_000,
            discontinuity_reset_count: 2,
            repeated_source_sample_count: 1,
            skipped_source_sample_count: 3,
            last_reset_reason: Some(PoseInterpolationResetReason::SequenceGap),
        };

        let value = serde_json::to_value(&status).unwrap();

        assert_eq!(value["policy"], "linear");
        assert_eq!(value["state"], "interpolating");
        assert_eq!(value["interpolationAlpha"], 0.5);
        assert_eq!(value["lastResetReason"], "sequence_gap");
        let encoded = value.to_string();
        assert!(!encoded.contains("producer"));
        assert!(!encoded.contains("spiffe"));
        assert_eq!(
            serde_json::from_value::<PoseInterpolationStatus>(value).unwrap(),
            status
        );
    }
}
