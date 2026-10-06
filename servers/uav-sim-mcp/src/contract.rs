//! Public UAV types, available without service or MCP integration.
//!
//! Camera and session identities cannot be interchanged:
//! ```compile_fail
//! use veoveo_uav_sim_mcp::contract::{LiveCameraId, LiveViewerInstanceId, OpenLiveViewRequest};
//! let request = OpenLiveViewRequest {
//!     session_id: LiveCameraId::parse("session").unwrap(),
//!     camera_id: LiveCameraId::parse("camera").unwrap(),
//!     viewer_instance_id: LiveViewerInstanceId::parse("browser").unwrap(),
//! };
//! ```
mod world_binding;
pub use world_binding::{InstallationWorldBinding, WorldBindingError};
mod recordings;
pub use recordings::{
    RecordingCatalog, RecordingCatalogError, RecordingCatalogLifecycle, RecordingState,
};
mod task_kind;
pub use task_kind::UavTaskKind;
mod live_view;
pub use live_view::*;
mod scopes;
pub use scopes::UavScope;
mod resources;
pub use resources::*;

use std::{collections::BTreeMap, fmt};
use veoveo_recording_contract::RecordingUri;
use veoveo_types::TaskTypeDefinition;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub use veoveo_frames_mcp::contract::Wgs84Position;
use veoveo_frames_mcp::contract::{FrameWorldRevision, FrameWorldRevisionUri, WorldFrameUri};
use veoveo_map_mcp::contract::{MapMobilityProfileUri, MapRouteHandoff};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdentityError {
    value: String,
    rule: &'static str,
}

impl IdentityError {
    fn new(value: &str, rule: &'static str) -> Self {
        Self {
            value: value.to_owned(),
            rule,
        }
    }
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid UAV simulation identity {:?}: {}",
            self.value, self.rule
        )
    }
}

impl std::error::Error for IdentityError {}

/// Distinct vehicle and session identities cannot be mixed in commands.
/// ```compile_fail
/// use veoveo_uav_sim_mcp::contract::{SessionRequest, VehicleId};
/// SessionRequest { session_id: VehicleId::parse("vehicle").unwrap() };
/// ```
#[doc = "Stable identity of one isolated simulation world."]
#[veoveo_types::id(text(SimulationIds))]
pub struct SessionId(String);
#[doc = "Stable identity of one vehicle inside a session."]
#[veoveo_types::id(text(SimulationIds))]
pub struct VehicleId(String);
#[doc = "Stable identity of one submitted mission."]
#[veoveo_types::id(text(SimulationIds))]
pub struct MissionId(String);
#[doc = "Stable identity of one admitted single-vehicle mission plan."]
#[veoveo_types::id(text(SimulationIds))]
pub struct MissionPlanId(String);
#[doc = "Stable identity of one principal-to-vehicle control grant."]
#[veoveo_types::id(text(SimulationIds))]
pub struct ControlGrantId(String);
#[doc = "Producer identity of one recording stream."]
#[veoveo_types::id(text(SimulationIds))]
pub struct RecordingKey(String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SimulationLifecycle {
    Unconfigured,
    Starting,
    Ready,
    Running,
    Paused,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TileLifecycle {
    Connecting,
    Streaming,
    Ready,
    Refreshing,
    Degraded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TileLoadType {
    IonEndpoint,
    TilesetJson,
    TileContent,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TileFailureCode {
    ProviderSessionRejected,
    CredentialsRejected,
    AssetUnavailable,
    QuotaExceeded,
    ProviderUnavailable,
    TransportFailed,
    RequestFailed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TileFailureState {
    pub code: TileFailureCode,
    pub load_type: TileLoadType,
    pub http_status: u16,
    pub generation: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CameraLifecycle {
    Warming,
    Ready,
    Degraded,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CameraCodec {
    H264,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CameraEncoder {
    NvidiaNvenc,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CameraTransport {
    RtspRtp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordingPublisherLifecycle {
    Connecting,
    Ready,
    Degraded,
    Stopped,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VehicleFlightState {
    Initializing,
    Standby,
    Armed,
    TakingOff,
    Flying,
    Landing,
    Landed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MissionLifecycle {
    Pending,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VehicleControlPermission {
    Inspect,
    Plan,
    Execute,
    Abort,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MissionPlanLifecycle {
    Prepared,
    Executing,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EnuVector {
    pub east_m: f64,
    pub north_m: f64,
    pub up_m: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NedVector {
    pub north_m: f64,
    pub east_m: f64,
    pub down_m: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuaternionXyzw {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EnuDirection {
    #[schemars(range(min = -1.0, max = 1.0))]
    pub east: f64,
    #[schemars(range(min = -1.0, max = 1.0))]
    pub north: f64,
    #[schemars(range(min = -1.0, max = 1.0))]
    pub up: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CameraRenderPoseState {
    #[schemars(range(min = 0.0))]
    pub position_error_m: f64,
    #[schemars(range(min = 0.0, max = 180.0))]
    pub forward_error_degrees: f64,
    pub rendered_position_enu_m: EnuVector,
    pub rendered_forward_enu: EnuDirection,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TileState {
    pub lifecycle: TileLifecycle,
    pub source: String,
    pub ion_asset_id: u64,
    pub resident_tiles: u64,
    pub visible_tiles: u64,
    pub loading_tiles: u64,
    pub geometries_loaded: u64,
    pub geometries_rendered: u64,
    pub materials_loaded: u64,
    pub provider_generation: u64,
    pub event_sequence: u64,
    pub refresh_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<TileFailureState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VehicleState {
    pub vehicle_id: VehicleId,
    pub flight_state: VehicleFlightState,
    pub wgs84: Wgs84Position,
    pub enu: EnuVector,
    pub ned: NedVector,
    pub attitude_xyzw: QuaternionXyzw,
    pub linear_velocity_enu_mps: EnuVector,
    #[schemars(range(min = 0.0, max = 100.0))]
    pub battery_percent: f32,
    pub collision_count: u64,
    pub px4_connected: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrantVehicleControlRequest {
    pub grant_id: ControlGrantId,
    pub session_id: SessionId,
    pub vehicle_id: VehicleId,
    pub principal_key: String,
    pub permissions: std::collections::BTreeSet<VehicleControlPermission>,
    pub map_mobility_profile_uri: MapMobilityProfileUri,
    #[serde(default)]
    pub allow_planning_advisory: bool,
    pub valid_from: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevokeVehicleControlRequest {
    pub grant_id: ControlGrantId,
    pub expected_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VehicleControlGrant {
    pub grant_id: ControlGrantId,
    pub session_id: SessionId,
    pub vehicle_id: VehicleId,
    pub principal_key: String,
    pub permissions: std::collections::BTreeSet<VehicleControlPermission>,
    pub map_mobility_profile_uri: MapMobilityProfileUri,
    pub allow_planning_advisory: bool,
    pub valid_from: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<DateTime<Utc>>,
    pub created_by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_by: Option<String>,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrepareVehicleMissionRequest {
    pub session_id: SessionId,
    pub mission_id: MissionId,
    pub vehicle_id: VehicleId,
    pub expected_world_revision_uri: FrameWorldRevisionUri,
    pub map_route: MapRouteHandoff,
    #[schemars(range(min = 0.1, max = 100.0))]
    pub speed_mps: f64,
    #[schemars(range(min = 0.0, max = 3600.0))]
    pub hold_seconds_at_destination: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecuteVehicleMissionPlanRequest {
    pub plan_id: MissionPlanId,
    pub expected_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VehicleMissionPlan {
    pub plan_id: MissionPlanId,
    pub mission_id: MissionId,
    pub principal_key: String,
    pub session_id: SessionId,
    pub vehicle_id: VehicleId,
    pub expected_world_revision_uri: FrameWorldRevisionUri,
    pub map_route: MapRouteHandoff,
    pub speed_mps: f64,
    pub hold_seconds_at_destination: f64,
    pub state: MissionPlanLifecycle,
    pub expires_at: DateTime<Utc>,
    pub revision: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CameraState {
    pub vehicle_id: VehicleId,
    pub entity_path: String,
    pub lifecycle: CameraLifecycle,
    pub width: u32,
    pub height: u32,
    pub frame_rate_hz: u32,
    pub codec: CameraCodec,
    pub encoder: CameraEncoder,
    pub transport: CameraTransport,
    pub frames_observed: u64,
    pub last_access_unit_bytes: u64,
    pub last_frame_keyframe: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_pose: Option<CameraRenderPoseState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RuntimeTimingState {
    #[schemars(range(min = 30, max = 1000))]
    pub physics_hz: u32,
    #[schemars(range(min = 1, max = 120))]
    pub native_rendering_hz: u32,
    pub render_cycles: u64,
    pub physics_steps: u64,
    pub refresh_states_wall_seconds: f64,
    pub vehicle_update_wall_seconds: f64,
    pub state_update_wall_seconds: f64,
    pub dynamics_update_wall_seconds: f64,
    pub sensor_update_wall_seconds: f64,
    pub backend_state_wall_seconds: f64,
    pub flush_forces_wall_seconds: f64,
    pub after_step_wall_seconds: f64,
    pub native_update_wall_seconds: f64,
    pub render_cycle_wall_seconds: f64,
    pub maximum_physics_step_ms: f64,
    pub maximum_native_update_ms: f64,
    pub maximum_render_cycle_ms: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SimulationState {
    pub session_id: SessionId,
    pub lifecycle: SimulationLifecycle,
    pub simulation_time_s: f64,
    pub physics_step: u64,
    pub timing: RuntimeTimingState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world: Option<SimulationWorldBinding>,
    pub tiles: TileState,
    pub cameras: Vec<CameraState>,
    pub live_cameras: Vec<LiveCameraDescriptor>,
    pub stream_products: Vec<LiveStreamProductState>,
    pub vehicles: Vec<VehicleState>,
    pub recordings: Vec<RecordingState>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(rename = "SimulationWorldBinding")]
pub struct SimulationWorldBindingValue {
    pub revision_uri: FrameWorldRevisionUri,
    pub spec_sha256: veoveo_artifact_contract::UploadSha256,
    pub simulation_frame_uri: WorldFrameUri,
    pub georeference_origin: Wgs84Position,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigureWorldRequest {
    pub session_id: SessionId,
    pub world_revision: FrameWorldRevision,
    pub simulation_frame_uri: WorldFrameUri,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigureWorldOutput {
    pub accepted: bool,
    pub world: SimulationWorldBinding,
    pub resource_uri: UavResource,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionRequest {
    pub session_id: SessionId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenLiveViewRequest {
    pub session_id: crate::contract::LiveSessionId,
    pub camera_id: crate::contract::LiveCameraId,
    pub viewer_instance_id: crate::contract::LiveViewerInstanceId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenewLiveViewRequest {
    pub session_id: crate::contract::LiveSessionId,
    pub live_view_id: crate::contract::LiveViewId,
    pub viewer_instance_id: crate::contract::LiveViewerInstanceId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseLiveViewRequest {
    pub session_id: crate::contract::LiveSessionId,
    pub live_view_id: crate::contract::LiveViewId,
    pub viewer_instance_id: crate::contract::LiveViewerInstanceId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CloseLiveViewResult {
    pub resource_uri: String,
    pub closed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StepSimulationRequest {
    pub session_id: SessionId,
    #[schemars(range(min = 1, max = 10_000))]
    pub steps: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VehicleRequest {
    pub session_id: SessionId,
    pub vehicle_id: VehicleId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TakeoffRequest {
    pub session_id: SessionId,
    pub vehicle_id: VehicleId,
    #[schemars(range(min = 0.5, max = 500.0))]
    pub relative_altitude_m: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommandAcknowledgement {
    pub accepted: bool,
    pub detail: String,
    pub resource_uri: UavResource,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum SimulationCommand {
    Pause(SessionRequest),
    Resume(SessionRequest),
    Reset(SessionRequest),
    Step(StepSimulationRequest),
    Arm(VehicleRequest),
    Takeoff(TakeoffRequest),
    Land(VehicleRequest),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MissionWaypoint {
    pub position: Wgs84Position,
    #[schemars(range(min = 0.1, max = 100.0))]
    pub speed_mps: f64,
    #[schemars(range(min = 0.0, max = 3600.0))]
    pub hold_seconds: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VehicleMission {
    pub vehicle_id: VehicleId,
    #[schemars(length(min = 1, max = 10_000))]
    pub waypoints: Vec<MissionWaypoint>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ExecuteMissionRequest {
    pub session_id: SessionId,
    pub mission_id: MissionId,
    pub expected_world_revision_uri: FrameWorldRevisionUri,
    #[schemars(length(min = 1, max = 256))]
    pub vehicles: Vec<VehicleMission>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RunScenarioRequest {
    pub session_id: SessionId,
    #[schemars(range(min = 0.1, max = 86_400.0))]
    pub duration_seconds: f64,
    #[serde(default)]
    pub parameters: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptureDatasetRequest {
    pub session_id: SessionId,
    #[schemars(range(min = 0.1, max = 86_400.0))]
    pub duration_seconds: f64,
    #[schemars(length(min = 1, max = 128))]
    pub sensors: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "operation",
    content = "input",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum DurableOperation {
    RunScenario(RunScenarioRequest),
    ExecuteMission(ExecuteMissionRequest),
    CaptureDataset(CaptureDatasetRequest),
}

impl DurableOperation {
    pub fn task_type(&self) -> veoveo_types::TaskTypeName {
        match self {
            Self::RunScenario(_) => UavTaskKind::RunScenario.name(),
            Self::ExecuteMission(_) => UavTaskKind::ExecuteMission.name(),
            Self::CaptureDataset(_) => UavTaskKind::CaptureDataset.name(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MissionResult {
    pub mission_id: MissionId,
    pub lifecycle: MissionLifecycle,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub completed_waypoints: u64,
    pub recording_uris: Vec<RecordingUri>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioResult {
    pub session_id: SessionId,
    pub elapsed_seconds: f64,
    pub final_simulation_time_s: f64,
    pub collision_count: u64,
    pub recording_uris: Vec<RecordingUri>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptureDatasetResult {
    pub session_id: SessionId,
    pub elapsed_seconds: f64,
    pub recording_uris: Vec<RecordingUri>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "result",
    content = "output",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum DurableOperationResult {
    RunScenario(ScenarioResult),
    ExecuteMission(MissionResult),
    CaptureDataset(CaptureDatasetResult),
}

/// One live collection page. Reuse the cursor only with the same collection/filter.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CollectionPage<T> {
    pub items: Vec<T>,
    pub limit: usize,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ActiveVehicleGrantsRequest {
    pub session_id: SessionId,
    #[serde(default)]
    /// Opaque next_cursor from the previous page for this session.
    pub cursor: Option<String>,
}

use veoveo_types::{IdProfile, IdProfileSpec};

#[doc(hidden)]
pub struct SimulationIds;
impl IdProfile for SimulationIds {
    type Error = IdentityError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| validate_id(value));
}

fn validate_id(value: &str) -> Result<(), IdentityError> {
    if matches!(value, "." | "..") {
        return Err(IdentityError::new(
            value,
            "must not be a relative path segment",
        ));
    }
    if value.is_empty() || value.len() > 128 {
        return Err(IdentityError::new(value, "must be 1 to 128 characters"));
    }
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
    {
        Ok(())
    } else {
        Err(IdentityError::new(
            value,
            "must contain only ASCII letters, digits, underscore, dash, or dot",
        ))
    }
}

/// Public session collection row. World is required and nullable on this wire.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub lifecycle: SimulationLifecycle,
    #[schemars(required)]
    pub world: Option<SimulationWorldBinding>,
    pub tile_lifecycle: TileLifecycle,
    pub vehicle_count: usize,
    pub recording_count: usize,
    pub timing: RuntimeTimingState,
    pub updated_at: DateTime<Utc>,
}

/// Schemas consumed by the server-owned browser App.
pub mod app_schema;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    try_from = "SimulationWorldBindingValue",
    into = "SimulationWorldBindingValue"
)]
pub struct SimulationWorldBinding(veoveo_types::Checked<SimulationWorldBindingValue>);
impl std::ops::Deref for SimulationWorldBinding {
    type Target = SimulationWorldBindingValue;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl JsonSchema for SimulationWorldBinding {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "SimulationWorldBinding".into()
    }
    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        SimulationWorldBindingValue::json_schema(generator)
    }
}
impl TryFrom<SimulationWorldBindingValue> for SimulationWorldBinding {
    type Error = WorldBindingError;
    fn try_from(value: SimulationWorldBindingValue) -> Result<Self, Self::Error> {
        veoveo_types::Checked::new(value).map(Self)
    }
}
impl From<SimulationWorldBinding> for SimulationWorldBindingValue {
    fn from(value: SimulationWorldBinding) -> Self {
        value.0.into_inner()
    }
}
impl SimulationWorldBindingValue {
    pub fn build(self) -> Result<SimulationWorldBinding, WorldBindingError> {
        self.try_into()
    }
}
impl veoveo_types::Check for SimulationWorldBindingValue {
    type Error = WorldBindingError;
    fn check(&self) -> Result<(), Self::Error> {
        self.validate()?;
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulationStateWire {
    session_id: SessionId,
    lifecycle: SimulationLifecycle,
    simulation_time_s: f64,
    physics_step: u64,
    timing: RuntimeTimingState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    world: Option<SimulationWorldBinding>,
    tiles: TileState,
    cameras: Vec<CameraState>,
    live_cameras: Vec<LiveCameraDescriptor>,
    stream_products: Vec<LiveStreamProductState>,
    vehicles: Vec<VehicleState>,
    recordings: Vec<RecordingState>,
    updated_at: DateTime<Utc>,
}
impl<'de> Deserialize<'de> for SimulationState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = SimulationStateWire::deserialize(deserializer)?;
        Self {
            session_id: value.session_id,
            lifecycle: value.lifecycle,
            simulation_time_s: value.simulation_time_s,
            physics_step: value.physics_step,
            timing: value.timing,
            world: value.world,
            tiles: value.tiles,
            cameras: value.cameras,
            live_cameras: value.live_cameras,
            stream_products: value.stream_products,
            vehicles: value.vehicles,
            recordings: value.recordings,
            updated_at: value.updated_at,
        }
        .build()
        .map_err(serde::de::Error::custom)
    }
}
impl SimulationState {
    pub fn build(self) -> Result<Self, StateValueError> {
        veoveo_types::Check::check(&self)?;
        Ok(self)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid UAV simulation telemetry or child identity")]
pub struct StateValueError;
impl veoveo_types::Check for SimulationState {
    type Error = StateValueError;
    fn check(&self) -> Result<(), Self::Error> {
        let finite = |values: &[f64]| values.iter().all(|v| v.is_finite());
        let enu = |v: &EnuVector| finite(&[v.east_m, v.north_m, v.up_m]);
        if !self.simulation_time_s.is_finite()
            || !(30..=1000).contains(&self.timing.physics_hz)
            || !(1..=120).contains(&self.timing.native_rendering_hz)
        {
            return Err(StateValueError);
        }
        let t = &self.timing;
        if !finite(&[
            t.refresh_states_wall_seconds,
            t.vehicle_update_wall_seconds,
            t.state_update_wall_seconds,
            t.dynamics_update_wall_seconds,
            t.sensor_update_wall_seconds,
            t.backend_state_wall_seconds,
            t.flush_forces_wall_seconds,
            t.after_step_wall_seconds,
            t.native_update_wall_seconds,
            t.render_cycle_wall_seconds,
            t.maximum_physics_step_ms,
            t.maximum_native_update_ms,
            t.maximum_render_cycle_ms,
        ]) {
            return Err(StateValueError);
        }
        if let Some(world) = &self.world {
            world.validate().map_err(|_| StateValueError)?;
        }
        let mut vehicles = std::collections::BTreeSet::new();
        for v in &self.vehicles {
            if !vehicles.insert(&v.vehicle_id)
                || !v.battery_percent.is_finite()
                || !(0.0..=100.0).contains(&v.battery_percent)
                || !finite(&[
                    v.wgs84.latitude_degrees,
                    v.wgs84.longitude_degrees,
                    v.wgs84.ellipsoid_height_m,
                    v.ned.north_m,
                    v.ned.east_m,
                    v.ned.down_m,
                    v.attitude_xyzw.x,
                    v.attitude_xyzw.y,
                    v.attitude_xyzw.z,
                    v.attitude_xyzw.w,
                ])
                || !(-90.0..=90.0).contains(&v.wgs84.latitude_degrees)
                || !(-180.0..=180.0).contains(&v.wgs84.longitude_degrees)
                || !enu(&v.enu)
                || !enu(&v.linear_velocity_enu_mps)
            {
                return Err(StateValueError);
            }
        }
        let mut cameras = std::collections::BTreeSet::new();
        for c in &self.cameras {
            if !vehicles.contains(&c.vehicle_id) || !cameras.insert((&c.vehicle_id, &c.entity_path))
            {
                return Err(StateValueError);
            }
            if let Some(p) = &c.render_pose {
                let d = &p.rendered_forward_enu;
                if !p.position_error_m.is_finite()
                    || p.position_error_m < 0.0
                    || !p.forward_error_degrees.is_finite()
                    || !(0.0..=180.0).contains(&p.forward_error_degrees)
                    || !enu(&p.rendered_position_enu_m)
                    || ![d.east, d.north, d.up]
                        .iter()
                        .all(|v| v.is_finite() && (-1.0..=1.0).contains(v))
                {
                    return Err(StateValueError);
                }
            }
        }
        let mut live = std::collections::BTreeSet::new();
        for c in &self.live_cameras {
            if c.session_id.as_str() != self.session_id.as_str()
                || !live.insert(&c.camera_id)
                || c.validate().is_err()
            {
                return Err(StateValueError);
            }
        }
        let mut products = std::collections::BTreeSet::new();
        for p in &self.stream_products {
            if !products.insert(&p.stream_product_id)
                || !p.validate()
                || p.camera_regions
                    .iter()
                    .any(|r| !live.contains(&r.camera_id))
            {
                return Err(StateValueError);
            }
        }
        let mut recordings = std::collections::BTreeSet::new();
        if self
            .recordings
            .iter()
            .any(|r| !recordings.insert(&r.recording_key))
        {
            return Err(StateValueError);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_strict() {
        assert_eq!(
            SessionId::parse("session-alpha").unwrap().as_str(),
            "session-alpha"
        );
        assert!(SessionId::parse("session/alpha").is_err());
        assert!(VehicleId::parse("").is_err());
    }

    #[test]
    fn command_shape_is_tagged_and_strict() {
        let command = SimulationCommand::Step(StepSimulationRequest {
            session_id: SessionId::parse("session-alpha").unwrap(),
            steps: 4,
        });
        let value = serde_json::to_value(command).unwrap();
        assert_eq!(value["command"], "step");
        assert_eq!(value["session_id"], "session-alpha");
        assert_eq!(value["steps"], 4);
    }

    #[test]
    fn durable_operation_names_are_canonical() {
        let operation = DurableOperation::CaptureDataset(CaptureDatasetRequest {
            session_id: SessionId::parse("session-alpha").unwrap(),
            duration_seconds: 10.0,
            sensors: vec!["down-camera".to_owned()],
        });
        assert_eq!(operation.task_type(), UavTaskKind::CaptureDataset.name());
    }

    #[test]
    fn map_route_handoff_wire_shape_is_consumable_without_translation() {
        let now: DateTime<Utc> = "2026-09-28T00:00:00Z".parse().unwrap();
        let profile_uri = veoveo_map_mcp::contract::MapMobilityProfileUri::new(
            veoveo_map_mcp::contract::MobilityProfileId::new(),
            veoveo_map_mcp::contract::MobilityProfileVersion::FIRST,
        );
        let produced = veoveo_map_mcp::MapRouteHandoffBuilder {
            schema_profile: veoveo_map_mcp::MapRouteHandoffSchema::V1,
            route_uri: veoveo_map_mcp::MapRouteUri::new(veoveo_map_mcp::RouteId::new()),
            route_digest_sha256: veoveo_types::Sha256Digest::from_hex("a".repeat(64)).unwrap(),
            route_status: veoveo_map_mcp::RouteStatus::Validated,
            mobility_profile_uri: profile_uri.clone(),
            path: vec![
                veoveo_map_mcp::contract::Wgs84Position::new(-74.006, 40.7128, Some(100.0))
                    .unwrap(),
                veoveo_map_mcp::contract::Wgs84Position::new(-74.0445, 40.6892, Some(100.0))
                    .unwrap(),
            ],
            validation_id: veoveo_map_mcp::ValidationId::new(),
            validated_at: now,
            operational_snapshot_id: veoveo_map_mcp::contract::OperationalSnapshotId::new(),
            base_release_ids: vec![veoveo_map_mcp::contract::DatasetReleaseId::new()],
            restriction_ids: Vec::new(),
            prepared_at: now,
        }
        .build()
        .unwrap();

        let request: PrepareVehicleMissionRequest = serde_json::from_value(serde_json::json!({
            "session_id": "session-alpha",
            "mission_id": "mission-alpha",
            "vehicle_id": "vehicle-one",
            "expected_world_revision_uri": "frames://world/native/revision/revision-one",
            "map_route": produced,
            "speed_mps": 5.0,
            "hold_seconds_at_destination": 0.0
        }))
        .unwrap();
        assert_eq!(request.map_route, produced);
        assert_eq!(request.map_route.mobility_profile_uri(), &profile_uri);
    }
}
