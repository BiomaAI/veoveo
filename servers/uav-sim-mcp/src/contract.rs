use std::{collections::BTreeMap, fmt, str::FromStr};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn validate_id(value: &str) -> Result<(), IdentityError> {
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

macro_rules! domain_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdentityError> {
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
            type Err = IdentityError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentityError;

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

domain_id!(
    SessionId,
    "Stable identity of one isolated simulation world."
);
domain_id!(
    VehicleId,
    "Stable identity of one vehicle inside a session."
);
domain_id!(MissionId, "Stable identity of one submitted mission.");
domain_id!(RecordingId, "Stable identity of one governed recording.");

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SimulationLifecycle {
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
    Failed,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Wgs84Position {
    #[schemars(range(min = -90.0, max = 90.0))]
    pub latitude_degrees: f64,
    #[schemars(range(min = -180.0, max = 180.0))]
    pub longitude_degrees: f64,
    pub ellipsoid_height_m: f64,
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
pub struct TileState {
    pub lifecycle: TileLifecycle,
    pub source: String,
    pub ion_asset_id: u64,
    pub resident_tiles: u64,
    pub loading_tiles: u64,
    pub failed_tiles: u64,
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
pub struct CameraState {
    pub vehicle_id: VehicleId,
    pub entity_path: String,
    pub lifecycle: CameraLifecycle,
    pub width: u32,
    pub height: u32,
    pub frames_observed: u64,
    #[schemars(range(min = 0.0, max = 255.0))]
    pub mean_luma: f32,
    pub dynamic_range: u8,
    #[schemars(range(min = 0.0, max = 1.0))]
    pub non_black_fraction: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordingState {
    pub recording_id: RecordingId,
    pub recording_uri: String,
    pub active: bool,
    pub camera_streams: Vec<String>,
    pub started_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SimulationState {
    pub session_id: SessionId,
    pub lifecycle: SimulationLifecycle,
    pub simulation_time_s: f64,
    pub physics_step: u64,
    pub frame_uri: String,
    pub georeference_origin: Wgs84Position,
    pub tiles: TileState,
    pub cameras: Vec<CameraState>,
    pub vehicles: Vec<VehicleState>,
    pub recordings: Vec<RecordingState>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionRequest {
    pub session_id: SessionId,
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
    pub resource_uri: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case")]
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
    pub frame_uri: String,
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
#[serde(tag = "operation", content = "input", rename_all = "snake_case")]
pub enum DurableOperation {
    RunScenario(RunScenarioRequest),
    ExecuteMission(ExecuteMissionRequest),
    CaptureDataset(CaptureDatasetRequest),
}

impl DurableOperation {
    pub const fn task_type(&self) -> &'static str {
        match self {
            Self::RunScenario(_) => "run_scenario",
            Self::ExecuteMission(_) => "execute_mission",
            Self::CaptureDataset(_) => "capture_dataset",
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
    pub recording_uris: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioResult {
    pub session_id: SessionId,
    pub elapsed_seconds: f64,
    pub final_simulation_time_s: f64,
    pub collision_count: u64,
    pub recording_uris: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptureDatasetResult {
    pub session_id: SessionId,
    pub elapsed_seconds: f64,
    pub recording_uris: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "result", content = "output", rename_all = "snake_case")]
pub enum DurableOperationResult {
    RunScenario(ScenarioResult),
    ExecuteMission(MissionResult),
    CaptureDataset(CaptureDatasetResult),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers_are_strict() {
        assert_eq!(
            SessionId::new("session-alpha").unwrap().as_str(),
            "session-alpha"
        );
        assert!(SessionId::new("session/alpha").is_err());
        assert!(VehicleId::new("").is_err());
    }

    #[test]
    fn command_shape_is_tagged_and_strict() {
        let command = SimulationCommand::Step(StepSimulationRequest {
            session_id: SessionId::new("session-alpha").unwrap(),
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
            session_id: SessionId::new("session-alpha").unwrap(),
            duration_seconds: 10.0,
            sensors: vec!["front-camera".to_owned()],
        });
        assert_eq!(operation.task_type(), "capture_dataset");
    }
}
