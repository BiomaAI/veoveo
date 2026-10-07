use std::sync::Arc;
use std::time::Duration;
use veoveo_recording_store::RecordingRepository;

use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode, Url};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::Mutex;

use crate::contract::{LiveCameraDescriptor, LiveStreamProductState};
use veoveo_platform_store::{PlatformStore, RecordIdKey, TenantId, deterministic_tenant_id};
use veoveo_recording_contract::{RecordingId, RecordingUri};
use veoveo_recording_store::RecordingId as PlatformRecordingId;

use crate::{
    contract::{
        CameraState, CommandAcknowledgement, ConfigureWorldOutput, ConfigureWorldRequest,
        DurableOperation, DurableOperationResult, MissionLifecycle, MissionResult,
        RecordingCatalog, RecordingKey, RecordingPublisherLifecycle, RecordingState,
        RuntimeTimingState, ScenarioResult, SessionId, SimulationCommand, SimulationLifecycle,
        SimulationState, SimulationWorldBinding, TileState, VehicleFlightState, VehicleState,
    },
    uris,
};

mod completion;
use completion::AdapterDurableOperationResult;
pub use completion::CompletedOperation;

const RECORDING_APPLICATION_ID: &str = "veoveo-uav-sim";
const RECORDING_CATALOG_ATTEMPTS: usize = 100;
const RECORDING_CATALOG_RETRY: Duration = Duration::from_millis(100);

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct AdapterRecordingState {
    application_id: String,
    recording_key: RecordingKey,
    active: bool,
    publisher_lifecycle: RecordingPublisherLifecycle,
    queue_capacity: u32,
    queued_events: u32,
    dropped_events: u64,
    #[serde(default)]
    diagnostic: Option<String>,
    camera_streams: Vec<String>,
    started_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct AdapterSimulationState {
    session_id: SessionId,
    lifecycle: SimulationLifecycle,
    simulation_time_s: f64,
    physics_step: u64,
    timing: RuntimeTimingState,
    world: Option<SimulationWorldBinding>,
    tiles: TileState,
    cameras: Vec<CameraState>,
    live_cameras: Vec<LiveCameraDescriptor>,
    stream_products: Vec<LiveStreamProductState>,
    vehicles: Vec<VehicleState>,
    recordings: Vec<AdapterRecordingState>,
    updated_at: DateTime<Utc>,
}

#[derive(serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct AdapterWorldRequest<'a> {
    session_id: &'a SessionId,
    world: &'a SimulationWorldBinding,
}

#[derive(Clone)]
pub struct HttpAdapter {
    client: Client,
    event_client: Client,
    base_url: Url,
    bearer_token: SecretString,
    operation_timeout: Duration,
    platform_store: PlatformStore,
    recording_tenant_id: TenantId,
}

impl HttpAdapter {
    pub fn new(
        base_url: Url,
        timeout: Duration,
        operation_timeout: Duration,
        bearer_token: SecretString,
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
            .retry(reqwest::retry::never())
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(AdapterError::Transport)?;
        let event_client = Client::builder()
            .connect_timeout(timeout)
            .build()
            .map_err(AdapterError::Transport)?;
        Ok(Self {
            client,
            event_client,
            base_url,
            bearer_token,
            operation_timeout,
            platform_store,
            recording_tenant_id: deterministic_tenant_id(recording_tenant_key)
                .map_err(|error| AdapterError::Catalog(error.into()))?,
        })
    }

    pub async fn state(&self) -> Result<SimulationState, AdapterError> {
        let state: AdapterSimulationState = self.get("v2/state").await?;
        if state
            .stream_products
            .iter()
            .any(|product| !product.validate())
        {
            return Err(AdapterError::InvalidState(
                "simulator returned an invalid tiled stream product".to_owned(),
            ));
        }
        let mut recordings = Vec::with_capacity(state.recordings.len());
        for recording in state.recordings {
            if recording.application_id != RECORDING_APPLICATION_ID {
                return Err(AdapterError::InvalidRecordingCatalog(format!(
                    "adapter returned application_id {:?}, expected {RECORDING_APPLICATION_ID}",
                    recording.application_id
                )));
            }
            let recording_key = recording.recording_key;
            let catalog = match self.resolve_recording_once(&recording_key).await {
                Ok(Some(uri)) => RecordingCatalog::Ready(uri),
                Ok(None) => RecordingCatalog::Pending {
                    diagnostic: Some("recording catalog publication is pending".to_owned()),
                },
                Err(AdapterError::Catalog(error)) => {
                    tracing::warn!(error = ?error, recording_key = %recording_key, "recording catalog lookup is unavailable; simulation state remains readable");
                    RecordingCatalog::Unavailable {
                        diagnostic: Some("recording catalog is unavailable".to_owned()),
                    }
                }
                Err(error) => {
                    tracing::error!(error = ?error, recording_key = %recording_key, "recording catalog entry is invalid; simulation state remains readable");
                    RecordingCatalog::Invalid {
                        diagnostic: Some("recording catalog entry is invalid".to_owned()),
                    }
                }
            };
            recordings.push(RecordingState {
                recording_key,
                catalog,
                active: recording.active,
                publisher_lifecycle: recording.publisher_lifecycle,
                queue_capacity: recording.queue_capacity,
                queued_events: recording.queued_events,
                dropped_events: recording.dropped_events,
                publisher_diagnostic: recording.diagnostic,
                camera_streams: recording.camera_streams,
                started_at: recording.started_at,
            });
        }
        SimulationState {
            session_id: state.session_id,
            lifecycle: state.lifecycle,
            simulation_time_s: state.simulation_time_s,
            physics_step: state.physics_step,
            timing: state.timing,
            world: state.world,
            tiles: state.tiles,
            cameras: state.cameras,
            live_cameras: state.live_cameras,
            stream_products: state.stream_products,
            vehicles: state.vehicles,
            recordings,
            updated_at: state.updated_at,
        }
        .build()
        .map_err(|error| AdapterError::InvalidState(error.to_string()))
    }

    pub async fn configure_world(
        &self,
        request: &ConfigureWorldRequest,
    ) -> Result<ConfigureWorldOutput, AdapterError> {
        let world = SimulationWorldBinding::from_revision(
            &request.world_revision,
            &request.simulation_frame_uri,
        )
        .map_err(|error| AdapterError::InvalidState(error.to_string()))?;
        self.configure_world_binding(&request.session_id, &world)
            .await
    }

    pub async fn configure_world_binding(
        &self,
        session_id: &SessionId,
        world: &SimulationWorldBinding,
    ) -> Result<ConfigureWorldOutput, AdapterError> {
        world
            .validate()
            .map_err(|error| AdapterError::InvalidState(error.to_string()))?;
        let output: ConfigureWorldOutput = self
            .post("v2/world", &AdapterWorldRequest { session_id, world })
            .await?;
        admit_world_reply(session_id, world, &output)?;
        Ok(output)
    }

    pub async fn command(
        &self,
        command: &SimulationCommand,
    ) -> Result<CommandAcknowledgement, AdapterError> {
        let output: CommandAcknowledgement = self.post("v2/commands", command).await?;
        admit_command_reply(command, &output)?;
        Ok(output)
    }

    pub async fn execute(
        &self,
        operation: &DurableOperation,
    ) -> Result<CompletedOperation, AdapterError> {
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
            .post_with_timeout("v2/operations", operation, timeout)
            .await?;
        result.correlate(operation)
    }

    async fn resolve_recording_keys(
        &self,
        recording_keys: Vec<RecordingKey>,
    ) -> Result<Vec<RecordingUri>, AdapterError> {
        let mut recording_uris = Vec::with_capacity(recording_keys.len());
        for recording_key in recording_keys {
            recording_uris.push(self.resolve_recording(&recording_key).await?);
        }
        Ok(recording_uris)
    }

    async fn resolve_recording(
        &self,
        recording_key: &RecordingKey,
    ) -> Result<RecordingUri, AdapterError> {
        for _ in 0..RECORDING_CATALOG_ATTEMPTS {
            if let Some(recording) = self.resolve_recording_once(recording_key).await? {
                return Ok(recording);
            }
            tokio::time::sleep(RECORDING_CATALOG_RETRY).await;
        }
        Err(AdapterError::RecordingCatalogTimeout(
            recording_key.to_string(),
        ))
    }

    async fn resolve_recording_once(
        &self,
        recording_key: &RecordingKey,
    ) -> Result<Option<RecordingUri>, AdapterError> {
        let Some(recording) = RecordingRepository::new(self.platform_store.clone())
            .recording_by_key(
                self.recording_tenant_id,
                RECORDING_APPLICATION_ID,
                recording_key.as_str(),
            )
            .await
            .map_err(AdapterError::Catalog)?
        else {
            return Ok(None);
        };
        let id = catalog_recording_id(&recording.id)?;
        Ok(Some(RecordingUri::new(id)))
    }

    async fn get<T>(&self, path: &str) -> Result<T, AdapterError>
    where
        T: serde::de::DeserializeOwned,
    {
        let response = self
            .client
            .get(self.endpoint(path)?)
            .bearer_auth(self.bearer_token.expose_secret())
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
            .bearer_auth(self.bearer_token.expose_secret())
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
            .bearer_auth(self.bearer_token.expose_secret())
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

    pub async fn runtime_events(&self) -> Result<reqwest::Response, AdapterError> {
        let response = self
            .event_client
            .get(self.endpoint("v1/events")?)
            .bearer_auth(self.bearer_token.expose_secret())
            .send()
            .await
            .map_err(AdapterError::Transport)?;
        if response.status().is_success() {
            Ok(response)
        } else {
            let status = response.status();
            let detail = response
                .text()
                .await
                .unwrap_or_else(|_| "adapter response body unavailable".to_owned());
            Err(AdapterError::Rejected { status, detail })
        }
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
    let body = response.bytes().await.map_err(AdapterError::Transport)?;
    serde_json::from_slice(&body).map_err(AdapterError::InvalidResponse)
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

    pub fn configure_world(
        &mut self,
        request: &ConfigureWorldRequest,
    ) -> Result<ConfigureWorldOutput, AdapterError> {
        let world = SimulationWorldBinding::from_revision(
            &request.world_revision,
            &request.simulation_frame_uri,
        )
        .map_err(|error| AdapterError::InvalidState(error.to_string()))?;
        self.configure_world_binding(&request.session_id, &world)
    }

    pub fn configure_world_binding(
        &mut self,
        session_id: &SessionId,
        world: &SimulationWorldBinding,
    ) -> Result<ConfigureWorldOutput, AdapterError> {
        self.require_session(session_id)?;
        if let Some(existing) = &self.state.world {
            if existing == world {
                return Ok(ConfigureWorldOutput {
                    accepted: true,
                    world: world.clone(),
                    resource_uri: crate::contract::UavResource::World(session_id.clone()),
                });
            }
            return Err(AdapterError::InvalidState(
                "simulation world is already configured".to_owned(),
            ));
        }
        self.state.world = Some(world.clone());
        self.state.lifecycle = SimulationLifecycle::Ready;
        self.state.updated_at = Utc::now();
        Ok(ConfigureWorldOutput {
            accepted: true,
            world: world.clone(),
            resource_uri: crate::contract::UavResource::World(session_id.clone()),
        })
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
                self.state.simulation_time_s +=
                    f64::from(request.steps) / f64::from(self.state.timing.physics_hz);
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
            resource_uri: crate::contract::UavResource::parse(resource_uri.as_str())
                .map_err(|error| AdapterError::InvalidState(error.to_string()))?,
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

    fn recording_uris(&self) -> Vec<RecordingUri> {
        self.state
            .recordings
            .iter()
            .filter_map(|recording| recording.catalog.recording_uri().cloned())
            .collect()
    }
}

fn catalog_recording_id(
    record: &veoveo_platform_store::RecordId,
) -> Result<RecordingId, AdapterError> {
    if record.table.as_str() != PlatformRecordingId::TABLE {
        return Err(AdapterError::InvalidRecordingCatalog(
            "catalog row is not a recording".to_owned(),
        ));
    }
    let RecordIdKey::Uuid(uuid) = &record.key else {
        return Err(AdapterError::InvalidRecordingCatalog(
            "catalog recording requires a native UUID key".to_owned(),
        ));
    };
    RecordingId::try_from(**uuid)
        .map_err(|error| AdapterError::InvalidRecordingCatalog(error.to_string()))
}

#[derive(Clone)]
pub enum Adapter {
    Http(Box<HttpAdapter>),
    Fake(Arc<Mutex<FakeAdapter>>),
}

impl Adapter {
    pub async fn configure_world(
        &self,
        request: &ConfigureWorldRequest,
    ) -> Result<ConfigureWorldOutput, AdapterError> {
        let output = match self {
            Self::Http(adapter) => adapter.configure_world(request).await,
            Self::Fake(adapter) => adapter.lock().await.configure_world(request),
        }?;
        let world = SimulationWorldBinding::from_revision(
            &request.world_revision,
            &request.simulation_frame_uri,
        )
        .map_err(|error| AdapterError::InvalidState(error.to_string()))?;
        admit_world_reply(&request.session_id, &world, &output)?;
        Ok(output)
    }

    pub async fn configure_world_binding(
        &self,
        session_id: &SessionId,
        world: &SimulationWorldBinding,
    ) -> Result<ConfigureWorldOutput, AdapterError> {
        let output = match self {
            Self::Http(adapter) => adapter.configure_world_binding(session_id, world).await,
            Self::Fake(adapter) => adapter
                .lock()
                .await
                .configure_world_binding(session_id, world),
        }?;
        admit_world_reply(session_id, world, &output)?;
        Ok(output)
    }

    pub async fn state(&self) -> Result<SimulationState, AdapterError> {
        match self {
            Self::Http(adapter) => adapter.state().await,
            Self::Fake(adapter) => adapter
                .lock()
                .await
                .state()
                .build()
                .map_err(|error| AdapterError::InvalidState(error.to_string())),
        }
    }

    pub async fn command(
        &self,
        command: &SimulationCommand,
    ) -> Result<CommandAcknowledgement, AdapterError> {
        let output = match self {
            Self::Http(adapter) => adapter.command(command).await,
            Self::Fake(adapter) => adapter.lock().await.command(command),
        }?;
        admit_command_reply(command, &output)?;
        Ok(output)
    }

    pub async fn execute(
        &self,
        operation: &DurableOperation,
    ) -> Result<CompletedOperation, AdapterError> {
        match self {
            Self::Http(adapter) => adapter.execute(operation).await,
            Self::Fake(adapter) => {
                CompletedOperation::new(operation, adapter.lock().await.execute(operation)?)
            }
        }
    }

    pub async fn resolve_result(
        &self,
        completed: CompletedOperation,
    ) -> Result<DurableOperationResult, AdapterError> {
        completed.resolve(self).await
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
    #[error("adapter returned a response that violates its typed contract: {0}")]
    InvalidResponse(#[source] serde_json::Error),
    #[error("adapter rejected the request with {status}: {detail}")]
    Rejected { status: StatusCode, detail: String },
    #[error("adapter completion does not match the dispatched operation")]
    UncorrelatedCompletion,
    #[error("unknown simulation session `{0}`")]
    UnknownSession(String),
    #[error("unknown vehicle `{0}`")]
    UnknownVehicle(String),
    #[error("unknown live camera `{0}`")]
    UnknownCamera(String),
    #[error("invalid simulator state: {0}")]
    InvalidState(String),
    #[error("recording catalog failed: {0}")]
    Catalog(#[source] veoveo_recording_store::RecordingStoreError),
    #[error("recording catalog returned invalid data: {0}")]
    InvalidRecordingCatalog(String),
    #[error("recording key `{0}` was not cataloged within 10 seconds")]
    RecordingCatalogTimeout(String),
}

#[cfg(all(test, feature = "mcp"))]
pub(crate) fn private_protocol_schemas() -> serde_json::Map<String, serde_json::Value> {
    fn root<T: schemars::JsonSchema>(direction: &str) -> serde_json::Value {
        let settings = schemars::generate::SchemaSettings::draft2020_12();
        let generator = if direction == "rust_to_python" {
            settings.for_serialize()
        } else {
            settings.for_deserialize()
        }
        .into_generator();
        serde_json::json!({"direction": direction, "schema": generator.into_root_schema_for::<T>()})
    }
    serde_json::json!({
        "POST /v2/world request": root::<AdapterWorldRequest<'static>>("rust_to_python"),
        "POST /v2/commands request": root::<SimulationCommand>("rust_to_python"),
        "POST /v2/operations request": root::<DurableOperation>("rust_to_python"),
        "GET /v2/state response": root::<AdapterSimulationState>("python_to_rust"),
        "POST /v2/world response": root::<ConfigureWorldOutput>("python_to_rust"),
        "POST /v2/commands response": root::<CommandAcknowledgement>("python_to_rust"),
        "POST /v2/operations response": root::<AdapterDurableOperationResult>("python_to_rust"),
    })
    .as_object()
    .unwrap()
    .clone()
}

#[cfg(all(test, feature = "mcp"))]
use crate::server::test_support::fixture;

fn admit_world_reply(
    session: &SessionId,
    world: &SimulationWorldBinding,
    output: &ConfigureWorldOutput,
) -> Result<(), AdapterError> {
    if !output.accepted
        || &output.world != world
        || output.resource_uri != crate::contract::UavResource::World(session.clone())
    {
        return Err(AdapterError::InvalidState(
            "simulator rejected or mismatched the requested world binding".into(),
        ));
    }
    Ok(())
}
fn admit_command_reply(
    command: &SimulationCommand,
    output: &CommandAcknowledgement,
) -> Result<(), AdapterError> {
    use crate::contract::UavResource;
    let expected = match command {
        SimulationCommand::Pause(r)
        | SimulationCommand::Resume(r)
        | SimulationCommand::Reset(r) => UavResource::Session(r.session_id.clone()),
        SimulationCommand::Step(r) => UavResource::World(r.session_id.clone()),
        SimulationCommand::Arm(r) | SimulationCommand::Land(r) => UavResource::Vehicle {
            session: r.session_id.clone(),
            vehicle: r.vehicle_id.clone(),
        },
        SimulationCommand::Takeoff(r) => UavResource::Vehicle {
            session: r.session_id.clone(),
            vehicle: r.vehicle_id.clone(),
        },
    };
    if !output.accepted || output.resource_uri != expected {
        return Err(AdapterError::InvalidState(
            "simulator rejected or mismatched the requested command".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;
    use crate::contract::{
        CameraCodec, CameraEncoder, CameraLifecycle, CameraState, EnuVector, NedVector,
        QuaternionXyzw, SessionId, SimulationWorldBinding, StepSimulationRequest, TileLifecycle,
        TileState, VehicleId, VehicleState, Wgs84Position,
    };
    use veoveo_frames_mcp::contract::{
        FrameId, FrameWorldId, FrameWorldRevisionId, FrameWorldRevisionUri, WorldFrameUri,
    };

    #[cfg(feature = "mcp")]
    #[tokio::test]
    async fn actual_http_acknowledgements_reject_foreign_parents_and_failed_worlds() {
        use axum::{Json, Router, extract::State, routing::post};
        use std::future::IntoFuture;
        tokio::time::timeout(Duration::from_secs(120), async {
            let _ = rustls::crypto::ring::default_provider().install_default();
            let db = super::fixture::TestDb::new().await;
            let response = Arc::new(Mutex::new(serde_json::json!({"accepted": true, "detail": "paused", "resourceUri": "uav-sim://session/session-alpha"})));
            async fn reply(State(value): State<Arc<Mutex<serde_json::Value>>>, Json(_request): Json<serde_json::Value>) -> Json<serde_json::Value> { Json(value.lock().await.clone()) }
            let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = Url::parse(&format!("http://{}/", socket.local_addr().unwrap())).unwrap();
            let server = tokio::spawn(axum::serve(socket, Router::new().route("/v2/commands", post(reply)).route("/v2/world", post(reply)).with_state(response.clone())).into_future());
            struct Server(tokio::task::JoinHandle<Result<(), std::io::Error>>);
            impl Drop for Server { fn drop(&mut self) { self.0.abort(); } }
            let _server = Server(server);
            let adapter = HttpAdapter::new(url, Duration::from_secs(5), Duration::from_secs(5), SecretString::from("fixture"), db.a.clone(), "test").unwrap();
            let session = SessionId::parse("session-alpha").unwrap();
            let command = SimulationCommand::Pause(crate::contract::SessionRequest { session_id: session.clone() });
            assert!(adapter.command(&command).await.unwrap().accepted);
            for resource in ["uav-sim://session/other", "uav-sim://session/session-alpha/world", "uav-sim://session/session-alpha/vehicle/uav-1"] {
                *response.lock().await = serde_json::json!({"accepted": true, "detail": "paused", "resourceUri": resource});
                assert!(matches!(adapter.command(&command).await, Err(AdapterError::InvalidState(_))));
            }
            *response.lock().await = serde_json::json!({"accepted": false, "detail": "command rejected", "resourceUri": "uav-sim://session/session-alpha"});
            assert!(adapter.command(&command).await.is_err());
            let world = fake_world();
            let good = ConfigureWorldOutput { accepted: true, world: world.clone(), resource_uri: crate::contract::UavResource::World(session.clone()) };
            *response.lock().await = serde_json::to_value(&good).unwrap();
            assert_eq!(adapter.configure_world_binding(&session, &world).await.unwrap(), good);
            for field in 0..3 {
                let mut wire = serde_json::to_value(&good).unwrap();
                match field {
                    0 => wire["accepted"] = false.into(),
                    1 => wire["resourceUri"] = "uav-sim://session/other/world".into(),
                    _ => wire["world"]["specSha256"] = "b".repeat(64).into(),
                }
                *response.lock().await = wire;
                assert!(adapter.configure_world_binding(&session, &world).await.is_err());
            }
        }).await.expect("UAV acknowledgement HTTP controls exceeded 120 seconds");
    }

    fn fake_world() -> SimulationWorldBinding {
        let revision_uri = FrameWorldRevisionUri::new(
            &FrameWorldId::parse("test-world").unwrap(),
            &FrameWorldRevisionId::parse("revision-1").unwrap(),
        );
        crate::contract::SimulationWorldBindingValue {
            revision_uri: revision_uri.clone(),
            spec_sha256: veoveo_artifact_contract::UploadSha256::parse("a".repeat(64)).unwrap(),
            simulation_frame_uri: WorldFrameUri::new(
                &revision_uri,
                &FrameId::parse("isaac-world").unwrap(),
            ),
            georeference_origin: Wgs84Position {
                latitude_degrees: 13.6929,
                longitude_degrees: -89.2182,
                ellipsoid_height_m: 700.0,
            },
        }
        .build()
        .unwrap()
    }

    fn fake_state() -> SimulationState {
        SimulationState {
            session_id: SessionId::parse("session-alpha").unwrap(),
            lifecycle: SimulationLifecycle::Running,
            simulation_time_s: 1.0,
            physics_step: 250,
            timing: RuntimeTimingState {
                physics_hz: 60,
                native_rendering_hz: 2,
                render_cycles: 0,
                physics_steps: 0,
                refresh_states_wall_seconds: 0.0,
                vehicle_update_wall_seconds: 0.0,
                state_update_wall_seconds: 0.0,
                dynamics_update_wall_seconds: 0.0,
                sensor_update_wall_seconds: 0.0,
                backend_state_wall_seconds: 0.0,
                flush_forces_wall_seconds: 0.0,
                after_step_wall_seconds: 0.0,
                native_update_wall_seconds: 0.0,
                render_cycle_wall_seconds: 0.0,
                maximum_physics_step_ms: 0.0,
                maximum_native_update_ms: 0.0,
                maximum_render_cycle_ms: 0.0,
            },
            world: Some(fake_world()),
            tiles: TileState {
                lifecycle: TileLifecycle::Ready,
                source: "google_photorealistic_3d_tiles".to_owned(),
                ion_asset_id: 2_275_207,
                resident_tiles: 20,
                visible_tiles: 12,
                loading_tiles: 0,
                geometries_loaded: 20,
                geometries_rendered: 12,
                materials_loaded: 20,
                provider_generation: 1,
                event_sequence: 0,
                refresh_count: 0,
                last_failure: None,
                diagnostic: None,
            },
            cameras: vec![CameraState {
                vehicle_id: VehicleId::parse("uav-1").unwrap(),
                entity_path: "/world/uav-sim/session-alpha/vehicle/uav-1/camera/down".to_owned(),
                lifecycle: CameraLifecycle::Ready,
                width: 640,
                height: 480,
                frame_rate_hz: 2,
                codec: CameraCodec::H264,
                encoder: CameraEncoder::NvidiaNvenc,
                transport: crate::contract::CameraTransport::RtspRtp,
                frames_observed: 10,
                last_access_unit_bytes: 32_768,
                last_frame_keyframe: false,
                render_pose: None,
                diagnostic: None,
            }],
            live_cameras: Vec::new(),
            stream_products: Vec::new(),
            vehicles: vec![VehicleState {
                vehicle_id: VehicleId::parse("uav-1").unwrap(),
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
    fn python_outbound_fixture_matches_private_rust_decoders() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../showcase/uav-sim/runtime/tests/fixtures/adapter_outputs.json"
        ))
        .unwrap();
        let state: AdapterSimulationState =
            serde_json::from_value(fixture["state"].clone()).unwrap();
        assert_eq!(state.live_cameras.len(), 7);
        for camera in &state.live_cameras {
            camera.validate().unwrap();
        }
        for product in &state.stream_products {
            assert!(product.validate());
        }
        serde_json::from_value::<CommandAcknowledgement>(fixture["command"].clone()).unwrap();
        serde_json::from_value::<ConfigureWorldOutput>(fixture["world"].clone()).unwrap();
        for result in fixture["results"].as_array().unwrap() {
            serde_json::from_value::<AdapterDurableOperationResult>(result.clone()).unwrap();
            let mut invalid = result.clone();
            invalid["unknown"] = serde_json::json!(true);
            assert!(serde_json::from_value::<AdapterDurableOperationResult>(invalid).is_err());
        }
        let mut missing_world = fixture["state"].clone();
        missing_world.as_object_mut().unwrap().remove("world");
        assert!(
            serde_json::from_value::<AdapterSimulationState>(missing_world)
                .unwrap()
                .world
                .is_none()
        );
        let mut null_world = fixture["state"].clone();
        null_world["world"] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<AdapterSimulationState>(null_world)
                .unwrap()
                .world
                .is_none()
        );
        for timestamp in ["20261004T120000+0000", "2026-W40-7T12:00:00+00:00"] {
            let mut invalid = fixture["state"].clone();
            invalid["updatedAt"] = serde_json::json!(timestamp);
            assert!(serde_json::from_value::<AdapterSimulationState>(invalid).is_err());
        }
        for timestamp in ["2026-10-04t12:00:00z", "2026-10-04T12:00:00+03:30"] {
            let mut valid = fixture["state"].clone();
            valid["updatedAt"] = serde_json::json!(timestamp);
            assert!(serde_json::from_value::<AdapterSimulationState>(valid).is_ok());
        }
    }

    #[test]
    fn private_current_outputs_refuse_retired_and_mixed_nested_members() {
        fn mutations(value: &serde_json::Value) -> Vec<serde_json::Value> {
            let mut result = Vec::new();
            match value {
                serde_json::Value::Object(fields) => {
                    for (key, child) in fields {
                        let retired: String = key
                            .chars()
                            .flat_map(|c| {
                                if c.is_ascii_uppercase() {
                                    vec!['_', c.to_ascii_lowercase()]
                                } else {
                                    vec![c]
                                }
                            })
                            .collect();
                        if retired != *key {
                            for mixed in [false, true] {
                                let mut bad = fields.clone();
                                if !mixed {
                                    bad.remove(key);
                                }
                                bad.insert(retired.clone(), child.clone());
                                result.push(bad.into());
                            }
                        }
                        for bad_child in mutations(child) {
                            let mut bad = fields.clone();
                            bad.insert(key.clone(), bad_child);
                            result.push(bad.into());
                        }
                    }
                }
                serde_json::Value::Array(items) => {
                    for (index, child) in items.iter().enumerate() {
                        for bad_child in mutations(child) {
                            let mut bad = items.clone();
                            bad[index] = bad_child;
                            result.push(bad.into());
                        }
                    }
                }
                _ => {}
            }
            result
        }
        fn qualify<T: serde::de::DeserializeOwned>(current: &serde_json::Value) -> usize {
            let _: T = serde_json::from_value(current.clone()).unwrap();
            let bad = mutations(current);
            for value in &bad {
                assert!(serde_json::from_value::<T>(value.clone()).is_err());
                assert!(serde_json::from_slice::<T>(&serde_json::to_vec(value).unwrap()).is_err());
            }
            bad.len()
        }
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../showcase/uav-sim/runtime/tests/fixtures/adapter_outputs.json"
        ))
        .unwrap();
        let mut count = qualify::<AdapterSimulationState>(&fixture["state"]);
        count += qualify::<CommandAcknowledgement>(&fixture["command"]);
        count += qualify::<ConfigureWorldOutput>(&fixture["world"]);
        for result in fixture["results"].as_array().unwrap() {
            count += qualify::<AdapterDurableOperationResult>(result);
        }
        assert!(count > 100);
    }

    #[test]
    fn private_battery_decoding_preserves_the_bounded_f32_interval() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../showcase/uav-sim/runtime/tests/fixtures/adapter_outputs.json"
        ))
        .unwrap();
        let preceding = f32::from_bits(100.0_f32.to_bits() - 1) as f64;
        let midpoint = (preceding + 100.0) / 2.0;
        let mut previous = 0.0_f32;
        for value in [
            0.0,
            f64::from_bits(1),
            0.1,
            preceding,
            midpoint,
            f64::from_bits(100.0_f64.to_bits() - 1),
            100.0,
        ] {
            let mut payload = fixture["state"].clone();
            payload["vehicles"][0]["batteryPercent"] = serde_json::json!(value);
            let bytes = serde_json::to_vec(&payload).unwrap();
            let decoded: AdapterSimulationState = serde_json::from_slice(&bytes).unwrap();
            let battery = decoded.vehicles[0].battery_percent;
            // Compatibility promises interval admission, not bit equality with
            // an in-memory f64 cast: decimal wire parsing can move a midpoint.
            assert!(battery.is_finite() && (0.0..=100.0).contains(&battery));
            assert!(battery >= previous);
            previous = battery;
            if value == 0.0 || value == 100.0 {
                assert_eq!(battery as f64, value);
            }
        }
        for value in [serde_json::json!(true), serde_json::json!("100")] {
            let mut payload = fixture["state"].clone();
            payload["vehicles"][0]["batteryPercent"] = value;
            assert!(
                serde_json::from_slice::<AdapterSimulationState>(
                    &serde_json::to_vec(&payload).unwrap()
                )
                .is_err()
            );
        }
        // The schema's range describes the producer contract; plain f32 Serde
        // does not enforce it. Runtime output admission owns that range check.
    }

    #[test]
    fn fake_adapter_serializes_lifecycle_and_steps() {
        let mut adapter = FakeAdapter::new(fake_state());
        let session_id = SessionId::parse("session-alpha").unwrap();
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
        let expected_time = 1.0 + 25.0 / 60.0;
        assert!((adapter.state().simulation_time_s - expected_time).abs() < f64::EPSILON);
    }

    #[test]
    fn fake_adapter_takeoff_atomically_arms_and_launches() {
        let mut adapter = FakeAdapter::new(fake_state());
        adapter
            .command(&SimulationCommand::Takeoff(
                crate::contract::TakeoffRequest {
                    session_id: SessionId::parse("session-alpha").unwrap(),
                    vehicle_id: VehicleId::parse("uav-1").unwrap(),
                    relative_altitude_m: 10.0,
                },
            ))
            .unwrap();
        assert_eq!(
            adapter.state().vehicles[0].flight_state,
            VehicleFlightState::Flying
        );
    }

    #[test]
    fn catalog_identity_requires_the_recording_table_and_native_rfc_uuidv7() {
        let id = RecordingId::new();
        let record = PlatformRecordingId::from_uuid(id.as_uuid()).record_id();
        assert_eq!(catalog_recording_id(&record).unwrap(), id);
        for record in [
            veoveo_platform_store::RecordId::new("other", record.key.clone()),
            veoveo_platform_store::RecordId::new("recording", id.to_string()),
            PlatformRecordingId::from_uuid(uuid::Uuid::nil()).record_id(),
            PlatformRecordingId::from_uuid(
                uuid::Uuid::parse_str("019f7122-3d89-7d21-0312-8940d1e0f510").unwrap(),
            )
            .record_id(),
        ] {
            let error = catalog_recording_id(&record).unwrap_err();
            assert!(!error.to_string().contains(&id.to_string()));
        }
    }

    #[test]
    fn private_adapter_recording_wire_uses_catalog_key() {
        let recording: AdapterRecordingState = serde_json::from_value(serde_json::json!({
            "applicationId": "veoveo-uav-sim",
            "recordingKey": "019f7122-3d89-7d21-8312-8940d1e0f510",
            "active": true,
            "publisherLifecycle": "ready",
            "queueCapacity": 256,
            "queuedEvents": 0,
            "droppedEvents": 0,
            "cameraStreams": ["/world/uav-sim/session-alpha/vehicle/uav-1/camera/down"],
            "startedAt": "2026-07-16T18:00:00Z"
        }))
        .unwrap();

        assert_eq!(recording.application_id, RECORDING_APPLICATION_ID);
        assert_eq!(
            recording.recording_key.as_str(),
            "019f7122-3d89-7d21-8312-8940d1e0f510"
        );
    }

    #[test]
    fn private_adapter_camera_wire_requires_exact_h264_nvenc_identity() {
        let camera: CameraState = serde_json::from_value(serde_json::json!({
            "vehicleId": "uav-1",
            "entityPath": "/world/uav-sim/session-alpha/vehicle/uav-1/camera/down",
            "lifecycle": "ready",
            "width": 640,
            "height": 480,
            "frameRateHz": 2,
            "codec": "h264",
            "encoder": "nvidia_nvenc",
            "transport": "rtsp_rtp",
            "framesObserved": 10,
            "lastAccessUnitBytes": 32768,
            "lastFrameKeyframe": false,
            "renderPose": {
                "positionErrorM": 0.02,
                "forwardErrorDegrees": 0.01,
                "renderedPositionEnuM": {
                    "eastM": 10.0,
                    "northM": 20.0,
                    "upM": 30.0
                },
                "renderedForwardEnu": {
                    "east": 0.0,
                    "north": 0.0,
                    "up": -1.0
                }
            }
        }))
        .unwrap();

        assert_eq!(camera.codec, CameraCodec::H264);
        assert_eq!(camera.encoder, CameraEncoder::NvidiaNvenc);
        assert_eq!(camera.frame_rate_hz, 2);
        assert_eq!(camera.render_pose.unwrap().position_error_m, 0.02);
    }

    #[test]
    fn private_adapter_tile_wire_requires_texture_proof_statistics() {
        let tiles: TileState = serde_json::from_value(serde_json::json!({
            "lifecycle": "ready",
            "source": "google_photorealistic_3d_tiles",
            "ionAssetId": 2_275_207,
            "residentTiles": 526,
            "visibleTiles": 18,
            "loadingTiles": 0,
            "geometriesLoaded": 470,
            "geometriesRendered": 29,
            "materialsLoaded": 470,
            "providerGeneration": 1,
            "eventSequence": 1,
            "refreshCount": 0
        }))
        .unwrap();

        assert_eq!(tiles.geometries_loaded, 470);
        assert_eq!(tiles.geometries_rendered, 29);
        assert_eq!(tiles.materials_loaded, 470);
    }

    #[test]
    fn private_adapter_timing_wire_requires_render_cadence_measurements() {
        let timing: RuntimeTimingState = serde_json::from_value(serde_json::json!({
            "physicsHz": 60,
            "nativeRenderingHz": 30,
            "renderCycles": 120,
            "physicsSteps": 240,
            "refreshStatesWallSeconds": 0.4,
            "vehicleUpdateWallSeconds": 0.8,
            "stateUpdateWallSeconds": 0.1,
            "dynamicsUpdateWallSeconds": 0.4,
            "sensorUpdateWallSeconds": 0.2,
            "backendStateWallSeconds": 0.1,
            "flushForcesWallSeconds": 0.2,
            "afterStepWallSeconds": 0.1,
            "nativeUpdateWallSeconds": 3.0,
            "renderCycleWallSeconds": 3.5,
            "maximumPhysicsStepMs": 12.0,
            "maximumNativeUpdateMs": 31.0,
            "maximumRenderCycleMs": 35.0
        }))
        .unwrap();

        assert_eq!(timing.render_cycles, 120);
        assert_eq!(timing.physics_steps, 240);
        assert_eq!(timing.maximum_physics_step_ms, 12.0);
        assert_eq!(timing.maximum_render_cycle_ms, 35.0);
    }

    #[test]
    fn private_adapter_rejects_claimed_public_recording_uri() {
        let error = serde_json::from_value::<AdapterRecordingState>(serde_json::json!({
            "applicationId": "veoveo-uav-sim",
            "recordingKey": "019f7122-3d89-7d21-8312-8940d1e0f510",
            "recordingUri": "recording://recordings/not-cataloged",
            "active": true,
            "publisherLifecycle": "ready",
            "queueCapacity": 256,
            "queuedEvents": 0,
            "droppedEvents": 0,
            "cameraStreams": [],
            "startedAt": "2026-07-16T18:00:00Z"
        }))
        .unwrap_err();

        assert!(error.to_string().contains("unknown field `recording_uri`"));
    }

    #[test]
    fn private_adapter_product_wire_preserves_visibility() {
        let product: LiveStreamProductState = serde_json::from_value(serde_json::json!({
            "streamProductId": "camera-atlas",
            "cameraRegions": [{
                "cameraId": "follow",
                "xPx": 0,
                "yPx": 0,
                "widthPx": 1280,
                "heightPx": 720
            }],
            "codedWidthPx": 1280,
            "codedHeightPx": 720,
            "lifecycle": "ready",
            "activeViewers": 1,
            "connectedViewers": 0,
            "nvencSessions": 1,
            "encodedFrames": 12,
            "sourceToRenderP95Microseconds": 18000,
            "sourceToRenderSamples": 120,
            "lastFrameAt": "2026-08-07T03:39:14Z",
            "visible": true
        }))
        .unwrap();

        assert_eq!(product.visible, Some(true));
        assert_eq!(product.source_to_render_p95_microseconds, Some(18_000));
        assert_eq!(product.source_to_render_samples, 120);
    }
}
