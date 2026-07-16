use std::time::Duration;

use chrono::Utc;
use reqwest::{Client, StatusCode, Url};
use thiserror::Error;

use crate::{
    contract::{
        CommandAcknowledgement, DurableOperation, DurableOperationResult, MissionLifecycle,
        MissionResult, ScenarioResult, SimulationCommand, SimulationLifecycle, SimulationState,
        VehicleFlightState,
    },
    uris,
};

#[derive(Clone)]
pub struct HttpAdapter {
    client: Client,
    base_url: Url,
}

impl HttpAdapter {
    pub fn new(base_url: Url, timeout: Duration) -> Result<Self, AdapterError> {
        if base_url.scheme() != "http" {
            return Err(AdapterError::Configuration(
                "simulator adapter URL must use cluster-private HTTP".to_owned(),
            ));
        }
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(AdapterError::Transport)?;
        Ok(Self { client, base_url })
    }

    pub async fn state(&self) -> Result<SimulationState, AdapterError> {
        self.get("v1/state").await
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
        self.post("v1/operations", operation).await
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
                ion_asset_id: 1,
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
}
