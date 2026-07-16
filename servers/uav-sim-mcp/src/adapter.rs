use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode, Url};
use serde::Deserialize;
use thiserror::Error;
use veoveo_platform_store::{
    PlatformStore, RecordIdKey, RecordingId as PlatformRecordingId, TenantId,
    deterministic_tenant_id,
};

use crate::{
    contract::{
        CaptureDatasetResult, CommandAcknowledgement, DurableOperation, DurableOperationResult,
        MissionId, MissionLifecycle, MissionResult, RecordingId, RecordingState, ScenarioResult,
        SessionId, SimulationCommand, SimulationLifecycle, SimulationState, TileState,
        VehicleFlightState, VehicleState, Wgs84Position,
    },
    uris,
};

const RECORDING_APPLICATION_ID: &str = "veoveo-uav-sim";
const RECORDING_CATALOG_ATTEMPTS: usize = 100;
const RECORDING_CATALOG_RETRY: Duration = Duration::from_millis(100);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdapterRecordingState {
    application_id: String,
    recording_key: String,
    active: bool,
    camera_streams: Vec<String>,
    started_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdapterSimulationState {
    session_id: SessionId,
    lifecycle: SimulationLifecycle,
    simulation_time_s: f64,
    physics_step: u64,
    frame_uri: String,
    georeference_origin: Wgs84Position,
    tiles: TileState,
    vehicles: Vec<VehicleState>,
    recordings: Vec<AdapterRecordingState>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdapterScenarioResult {
    session_id: SessionId,
    elapsed_seconds: f64,
    final_simulation_time_s: f64,
    collision_count: u64,
    recording_keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdapterMissionResult {
    mission_id: MissionId,
    lifecycle: MissionLifecycle,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    completed_waypoints: u64,
    recording_keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdapterCaptureDatasetResult {
    session_id: SessionId,
    elapsed_seconds: f64,
    recording_keys: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "result", content = "output", rename_all = "snake_case")]
enum AdapterDurableOperationResult {
    RunScenario(AdapterScenarioResult),
    ExecuteMission(AdapterMissionResult),
    CaptureDataset(AdapterCaptureDatasetResult),
}

#[derive(Clone)]
pub struct HttpAdapter {
    client: Client,
    base_url: Url,
    operation_timeout: Duration,
    platform_store: PlatformStore,
    recording_tenant_id: TenantId,
}

impl HttpAdapter {
    pub fn new(
        base_url: Url,
        timeout: Duration,
        operation_timeout: Duration,
        platform_store: PlatformStore,
        recording_tenant_key: &str,
    ) -> Result<Self, AdapterError> {
        if base_url.scheme() != "http" {
            return Err(AdapterError::Configuration(
                "simulator adapter URL must use cluster-private HTTP".to_owned(),
            ));
        }
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(AdapterError::Transport)?;
        Ok(Self {
            client,
            base_url,
            operation_timeout,
            platform_store,
            recording_tenant_id: deterministic_tenant_id(recording_tenant_key)
                .map_err(AdapterError::Catalog)?,
        })
    }

    pub async fn state(&self) -> Result<SimulationState, AdapterError> {
        let state: AdapterSimulationState = self.get("v1/state").await?;
        let mut recordings = Vec::with_capacity(state.recordings.len());
        for recording in state.recordings {
            if recording.application_id != RECORDING_APPLICATION_ID {
                return Err(AdapterError::InvalidRecordingCatalog(format!(
                    "adapter returned application_id {:?}, expected {RECORDING_APPLICATION_ID}",
                    recording.application_id
                )));
            }
            let (recording_id, recording_uri) =
                self.resolve_recording(&recording.recording_key).await?;
            recordings.push(RecordingState {
                recording_id,
                recording_uri,
                active: recording.active,
                camera_streams: recording.camera_streams,
                started_at: recording.started_at,
            });
        }
        Ok(SimulationState {
            session_id: state.session_id,
            lifecycle: state.lifecycle,
            simulation_time_s: state.simulation_time_s,
            physics_step: state.physics_step,
            frame_uri: state.frame_uri,
            georeference_origin: state.georeference_origin,
            tiles: state.tiles,
            vehicles: state.vehicles,
            recordings,
            updated_at: state.updated_at,
        })
    }

    pub async fn command(
        &self,
        command: &SimulationCommand,
    ) -> Result<CommandAcknowledgement, AdapterError> {
        self.post("v1/commands", command).await
    }

    pub async fn execute(
        &self,
        operation: &DurableOperation,
    ) -> Result<DurableOperationResult, AdapterError> {
        let simulated_duration = match operation {
            DurableOperation::RunScenario(request) => Some(request.duration_seconds),
            DurableOperation::CaptureDataset(request) => Some(request.duration_seconds),
            DurableOperation::ExecuteMission(_) => None,
        };
        let timeout = simulated_duration
            .map(|duration| Duration::from_secs_f64(duration.mul_add(20.0, 120.0)))
            .map_or(self.operation_timeout, |duration| {
                duration.max(self.operation_timeout)
            });
        let result: AdapterDurableOperationResult = self
            .post_with_timeout("v1/operations", operation, timeout)
            .await?;
        Ok(match result {
            AdapterDurableOperationResult::RunScenario(value) => {
                DurableOperationResult::RunScenario(ScenarioResult {
                    session_id: value.session_id,
                    elapsed_seconds: value.elapsed_seconds,
                    final_simulation_time_s: value.final_simulation_time_s,
                    collision_count: value.collision_count,
                    recording_uris: self.resolve_recording_keys(value.recording_keys).await?,
                })
            }
            AdapterDurableOperationResult::ExecuteMission(value) => {
                DurableOperationResult::ExecuteMission(MissionResult {
                    mission_id: value.mission_id,
                    lifecycle: value.lifecycle,
                    started_at: value.started_at,
                    finished_at: value.finished_at,
                    completed_waypoints: value.completed_waypoints,
                    recording_uris: self.resolve_recording_keys(value.recording_keys).await?,
                })
            }
            AdapterDurableOperationResult::CaptureDataset(value) => {
                DurableOperationResult::CaptureDataset(CaptureDatasetResult {
                    session_id: value.session_id,
                    elapsed_seconds: value.elapsed_seconds,
                    recording_uris: self.resolve_recording_keys(value.recording_keys).await?,
                })
            }
        })
    }

    async fn resolve_recording_keys(
        &self,
        recording_keys: Vec<String>,
    ) -> Result<Vec<String>, AdapterError> {
        let mut recording_uris = Vec::with_capacity(recording_keys.len());
        for recording_key in recording_keys {
            recording_uris.push(self.resolve_recording(&recording_key).await?.1);
        }
        Ok(recording_uris)
    }

    async fn resolve_recording(
        &self,
        recording_key: &str,
    ) -> Result<(RecordingId, String), AdapterError> {
        for _ in 0..RECORDING_CATALOG_ATTEMPTS {
            if let Some(recording) = self
                .platform_store
                .recording_by_key(
                    self.recording_tenant_id,
                    RECORDING_APPLICATION_ID,
                    recording_key,
                )
                .await
                .map_err(AdapterError::Catalog)?
            {
                let uuid = match recording.id.key {
                    RecordIdKey::Uuid(value) => *value,
                    RecordIdKey::String(value) => {
                        uuid::Uuid::parse_str(&value).map_err(|error| {
                            AdapterError::InvalidRecordingCatalog(error.to_string())
                        })?
                    }
                    key => {
                        return Err(AdapterError::InvalidRecordingCatalog(format!(
                            "recording catalog returned unsupported record key {key:?}"
                        )));
                    }
                };
                if recording.id.table.as_str() != PlatformRecordingId::TABLE
                    || uuid.get_version_num() != 7
                {
                    return Err(AdapterError::InvalidRecordingCatalog(format!(
                        "recording key {recording_key:?} resolved to a non-UUIDv7 recording"
                    )));
                }
                let id = RecordingId::new(uuid.to_string())
                    .map_err(|error| AdapterError::InvalidRecordingCatalog(error.to_string()))?;
                return Ok((id, format!("recording://recordings/{uuid}")));
            }
            tokio::time::sleep(RECORDING_CATALOG_RETRY).await;
        }
        Err(AdapterError::RecordingCatalogTimeout(
            recording_key.to_owned(),
        ))
    }

    async fn get<T>(&self, path: &str) -> Result<T, AdapterError>
    where
        T: serde::de::DeserializeOwned,
    {
        let response = self
            .client
            .get(self.endpoint(path)?)
            .send()
            .await
            .map_err(AdapterError::Transport)?;
        decode(response).await
    }

    async fn post<I, O>(&self, path: &str, input: &I) -> Result<O, AdapterError>
    where
        I: serde::Serialize + ?Sized,
        O: serde::de::DeserializeOwned,
    {
        let response = self
            .client
            .post(self.endpoint(path)?)
            .json(input)
            .send()
            .await
            .map_err(AdapterError::Transport)?;
        decode(response).await
    }

    async fn post_with_timeout<I, O>(
        &self,
        path: &str,
        input: &I,
        timeout: Duration,
    ) -> Result<O, AdapterError>
    where
        I: serde::Serialize + ?Sized,
        O: serde::de::DeserializeOwned,
    {
        let response = self
            .client
            .post(self.endpoint(path)?)
            .timeout(timeout)
            .json(input)
            .send()
            .await
            .map_err(AdapterError::Transport)?;
        decode(response).await
    }

    fn endpoint(&self, path: &str) -> Result<Url, AdapterError> {
        self.base_url.join(path).map_err(AdapterError::InvalidUrl)
    }
}

async fn decode<T>(response: reqwest::Response) -> Result<T, AdapterError>
where
    T: serde::de::DeserializeOwned,
{
    let status = response.status();
    if !status.is_success() {
        let detail = response
            .text()
            .await
            .unwrap_or_else(|_| "adapter response body unavailable".to_owned());
        return Err(AdapterError::Rejected { status, detail });
    }
    response.json().await.map_err(AdapterError::Transport)
}

pub struct FakeAdapter {
    state: SimulationState,
}

impl FakeAdapter {
    pub fn new(state: SimulationState) -> Self {
        Self { state }
    }

    pub fn state(&self) -> SimulationState {
        self.state.clone()
    }

    pub fn command(
        &mut self,
        command: &SimulationCommand,
    ) -> Result<CommandAcknowledgement, AdapterError> {
        let (detail, resource_uri) = match command {
            SimulationCommand::Pause(request) => {
                self.require_session(&request.session_id)?;
                self.state.lifecycle = SimulationLifecycle::Paused;
                (
                    "simulation paused".to_owned(),
                    uris::session(&request.session_id),
                )
            }
            SimulationCommand::Resume(request) => {
                self.require_session(&request.session_id)?;
                self.state.lifecycle = SimulationLifecycle::Running;
                (
                    "simulation resumed".to_owned(),
                    uris::session(&request.session_id),
                )
            }
            SimulationCommand::Reset(request) => {
                self.require_session(&request.session_id)?;
                self.state.lifecycle = SimulationLifecycle::Ready;
                self.state.simulation_time_s = 0.0;
                self.state.physics_step = 0;
                (
                    "simulation reset".to_owned(),
                    uris::session(&request.session_id),
                )
            }
            SimulationCommand::Step(request) => {
                self.require_session(&request.session_id)?;
                if self.state.lifecycle != SimulationLifecycle::Paused {
                    return Err(AdapterError::InvalidState(
                        "simulation must be paused before stepping".to_owned(),
                    ));
                }
                self.state.physics_step += u64::from(request.steps);
                self.state.simulation_time_s += f64::from(request.steps) / 250.0;
                (
                    format!("advanced {} physics step(s)", request.steps),
                    uris::world(&request.session_id),
                )
            }
            SimulationCommand::Arm(request) => {
                self.require_session(&request.session_id)?;
                let vehicle = self.vehicle_mut(&request.vehicle_id)?;
                vehicle.flight_state = VehicleFlightState::Armed;
                (
                    "vehicle armed".to_owned(),
                    uris::vehicle(&request.session_id, &request.vehicle_id),
                )
            }
            SimulationCommand::Takeoff(request) => {
                self.require_session(&request.session_id)?;
                let vehicle = self.vehicle_mut(&request.vehicle_id)?;
                if vehicle.flight_state != VehicleFlightState::Armed {
                    return Err(AdapterError::InvalidState(
                        "vehicle must be armed before takeoff".to_owned(),
                    ));
                }
                vehicle.flight_state = VehicleFlightState::Flying;
                vehicle.enu.up_m = request.relative_altitude_m;
                vehicle.ned.down_m = -request.relative_altitude_m;
                (
                    "vehicle took off".to_owned(),
                    uris::vehicle(&request.session_id, &request.vehicle_id),
                )
            }
            SimulationCommand::Land(request) => {
                self.require_session(&request.session_id)?;
                let vehicle = self.vehicle_mut(&request.vehicle_id)?;
                vehicle.flight_state = VehicleFlightState::Landed;
                vehicle.enu.up_m = 0.0;
                vehicle.ned.down_m = 0.0;
                (
                    "vehicle landed".to_owned(),
                    uris::vehicle(&request.session_id, &request.vehicle_id),
                )
            }
        };
        self.state.updated_at = Utc::now();
        Ok(CommandAcknowledgement {
            accepted: true,
            detail,
            resource_uri,
        })
    }

    pub fn execute(
        &mut self,
        operation: &DurableOperation,
    ) -> Result<DurableOperationResult, AdapterError> {
        match operation {
            DurableOperation::RunScenario(request) => {
                self.require_session(&request.session_id)?;
                self.state.simulation_time_s += request.duration_seconds;
                self.state.physics_step += (request.duration_seconds * 250.0) as u64;
                self.state.updated_at = Utc::now();
                Ok(DurableOperationResult::RunScenario(ScenarioResult {
                    session_id: request.session_id.clone(),
                    elapsed_seconds: request.duration_seconds,
                    final_simulation_time_s: self.state.simulation_time_s,
                    collision_count: self
                        .state
                        .vehicles
                        .iter()
                        .map(|vehicle| vehicle.collision_count)
                        .sum(),
                    recording_uris: self.recording_uris(),
                }))
            }
            DurableOperation::ExecuteMission(request) => {
                self.require_session(&request.session_id)?;
                let now = Utc::now();
                let completed_waypoints = request
                    .vehicles
                    .iter()
                    .map(|vehicle| vehicle.waypoints.len() as u64)
                    .sum();
                Ok(DurableOperationResult::ExecuteMission(MissionResult {
                    mission_id: request.mission_id.clone(),
                    lifecycle: MissionLifecycle::Completed,
                    started_at: now,
                    finished_at: now,
                    completed_waypoints,
                    recording_uris: self.recording_uris(),
                }))
            }
            DurableOperation::CaptureDataset(request) => {
                self.require_session(&request.session_id)?;
                Ok(DurableOperationResult::CaptureDataset(
                    crate::contract::CaptureDatasetResult {
                        session_id: request.session_id.clone(),
                        elapsed_seconds: request.duration_seconds,
                        recording_uris: self.recording_uris(),
                    },
                ))
            }
        }
    }

    fn require_session(&self, session_id: &crate::contract::SessionId) -> Result<(), AdapterError> {
        if &self.state.session_id == session_id {
            Ok(())
        } else {
            Err(AdapterError::UnknownSession(session_id.to_string()))
        }
    }

    fn vehicle_mut(
        &mut self,
        vehicle_id: &crate::contract::VehicleId,
    ) -> Result<&mut crate::contract::VehicleState, AdapterError> {
        self.state
            .vehicles
            .iter_mut()
            .find(|vehicle| &vehicle.vehicle_id == vehicle_id)
            .ok_or_else(|| AdapterError::UnknownVehicle(vehicle_id.to_string()))
    }

    fn recording_uris(&self) -> Vec<String> {
        self.state
            .recordings
            .iter()
            .map(|recording| recording.recording_uri.clone())
            .collect()
    }
}

pub enum Adapter {
    Http(HttpAdapter),
    Fake(FakeAdapter),
}

impl Adapter {
    pub async fn state(&mut self) -> Result<SimulationState, AdapterError> {
        match self {
            Self::Http(adapter) => adapter.state().await,
            Self::Fake(adapter) => Ok(adapter.state()),
        }
    }

    pub async fn command(
        &mut self,
        command: &SimulationCommand,
    ) -> Result<CommandAcknowledgement, AdapterError> {
        match self {
            Self::Http(adapter) => adapter.command(command).await,
            Self::Fake(adapter) => adapter.command(command),
        }
    }

    pub async fn execute(
        &mut self,
        operation: &DurableOperation,
    ) -> Result<DurableOperationResult, AdapterError> {
        match self {
            Self::Http(adapter) => adapter.execute(operation).await,
            Self::Fake(adapter) => adapter.execute(operation),
        }
    }
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("adapter configuration error: {0}")]
    Configuration(String),
    #[error("invalid adapter URL: {0}")]
    InvalidUrl(url::ParseError),
    #[error("adapter transport failed: {0}")]
    Transport(reqwest::Error),
    #[error("adapter rejected the request with {status}: {detail}")]
    Rejected { status: StatusCode, detail: String },
    #[error("unknown simulation session `{0}`")]
    UnknownSession(String),
    #[error("unknown vehicle `{0}`")]
    UnknownVehicle(String),
    #[error("invalid simulator state: {0}")]
    InvalidState(String),
    #[error("recording catalog failed: {0}")]
    Catalog(#[source] veoveo_platform_store::StoreError),
    #[error("recording catalog returned invalid data: {0}")]
    InvalidRecordingCatalog(String),
    #[error("recording key `{0}` was not cataloged within 10 seconds")]
    RecordingCatalogTimeout(String),
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::contract::{
        EnuVector, NedVector, QuaternionXyzw, SessionId, StepSimulationRequest, TileLifecycle,
        TileState, VehicleId, VehicleState, Wgs84Position,
    };

    fn fake_state() -> SimulationState {
        SimulationState {
            session_id: SessionId::new("session-alpha").unwrap(),
            lifecycle: SimulationLifecycle::Running,
            simulation_time_s: 1.0,
            physics_step: 250,
            frame_uri: "frames://frame/enu-alpha".to_owned(),
            georeference_origin: Wgs84Position {
                latitude_degrees: 13.6929,
                longitude_degrees: -89.2182,
                ellipsoid_height_m: 700.0,
            },
            tiles: TileState {
                lifecycle: TileLifecycle::Ready,
                source: "google_photorealistic_3d_tiles".to_owned(),
                ion_asset_id: 2_275_207,
                resident_tiles: 20,
                loading_tiles: 0,
                failed_tiles: 0,
                diagnostic: None,
            },
            vehicles: vec![VehicleState {
                vehicle_id: VehicleId::new("uav-1").unwrap(),
                flight_state: VehicleFlightState::Standby,
                wgs84: Wgs84Position {
                    latitude_degrees: 13.6929,
                    longitude_degrees: -89.2182,
                    ellipsoid_height_m: 700.0,
                },
                enu: EnuVector {
                    east_m: 0.0,
                    north_m: 0.0,
                    up_m: 0.0,
                },
                ned: NedVector {
                    north_m: 0.0,
                    east_m: 0.0,
                    down_m: 0.0,
                },
                attitude_xyzw: QuaternionXyzw {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 1.0,
                },
                linear_velocity_enu_mps: EnuVector {
                    east_m: 0.0,
                    north_m: 0.0,
                    up_m: 0.0,
                },
                battery_percent: 100.0,
                collision_count: 0,
                px4_connected: true,
            }],
            recordings: Vec::new(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn fake_adapter_serializes_lifecycle_and_steps() {
        let mut adapter = FakeAdapter::new(fake_state());
        let session_id = SessionId::new("session-alpha").unwrap();
        adapter
            .command(&SimulationCommand::Pause(crate::contract::SessionRequest {
                session_id: session_id.clone(),
            }))
            .unwrap();
        adapter
            .command(&SimulationCommand::Step(StepSimulationRequest {
                session_id,
                steps: 25,
            }))
            .unwrap();
        assert_eq!(adapter.state().lifecycle, SimulationLifecycle::Paused);
        assert_eq!(adapter.state().physics_step, 275);
        assert!((adapter.state().simulation_time_s - 1.1).abs() < f64::EPSILON);
    }

    #[test]
    fn fake_adapter_enforces_arm_before_takeoff() {
        let mut adapter = FakeAdapter::new(fake_state());
        let error = adapter
            .command(&SimulationCommand::Takeoff(
                crate::contract::TakeoffRequest {
                    session_id: SessionId::new("session-alpha").unwrap(),
                    vehicle_id: VehicleId::new("uav-1").unwrap(),
                    relative_altitude_m: 10.0,
                },
            ))
            .unwrap_err();
        assert!(matches!(error, AdapterError::InvalidState(_)));
    }

    #[test]
    fn private_adapter_recording_wire_uses_catalog_key() {
        let recording: AdapterRecordingState = serde_json::from_value(serde_json::json!({
            "application_id": "veoveo-uav-sim",
            "recording_key": "019f7122-3d89-7d21-8312-8940d1e0f510",
            "active": true,
            "camera_streams": ["/world/uav-sim/session-alpha/vehicle/uav-1/camera/front"],
            "started_at": "2026-07-16T18:00:00Z"
        }))
        .unwrap();

        assert_eq!(recording.application_id, RECORDING_APPLICATION_ID);
        assert_eq!(
            recording.recording_key,
            "019f7122-3d89-7d21-8312-8940d1e0f510"
        );
    }

    #[test]
    fn private_adapter_rejects_claimed_public_recording_uri() {
        let error = serde_json::from_value::<AdapterRecordingState>(serde_json::json!({
            "application_id": "veoveo-uav-sim",
            "recording_key": "019f7122-3d89-7d21-8312-8940d1e0f510",
            "recording_uri": "recording://recordings/not-cataloged",
            "active": true,
            "camera_streams": [],
            "started_at": "2026-07-16T18:00:00Z"
        }))
        .unwrap_err();

        assert!(error.to_string().contains("unknown field `recording_uri`"));
    }
}
