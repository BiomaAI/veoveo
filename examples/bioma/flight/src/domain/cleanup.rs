//! Flight-owned remote cleanup, retained before dispatch and polled within owner G.
use super::*;
use std::sync::{Arc, Mutex, atomic::AtomicBool};
use veoveo_stream_mcp::contract::SessionId as StreamSessionId;
use veoveo_testing_support::lifecycle::owner::{self, CleanupKind, CleanupRegistration};

/// The same domain checks run with either the ordinary caller or the retained
/// cleanup-only SDK peer; cleanup never launches an ordinary-effect subprocess.
pub trait FlightPeer: Sync {
    fn call_tool_with_timeout(
        &self,
        tool: &str,
        arguments: Value,
        timeout: Duration,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn resource(
        &self,
        uri: &str,
        timeout: Duration,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn call_tool(
        &self,
        tool: &str,
        arguments: Value,
    ) -> impl std::future::Future<Output = Result<Value>> + Send {
        self.call_tool_with_timeout(tool, arguments, Duration::from_secs(120))
    }
}
impl FlightPeer for OperatorClient<'_> {
    async fn call_tool_with_timeout(
        &self,
        tool: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value> {
        OperatorClient::call_tool_with_timeout(self, tool, arguments, timeout).await
    }
    async fn resource(&self, uri: &str, timeout: Duration) -> Result<Value> {
        OperatorClient::resource(self, uri, timeout).await
    }
}

pub(super) struct CleanupPeer {
    endpoint: String,
    bearer: String,
    ordinary: bool,
}
impl CleanupPeer {
    async fn capture(operator: &OperatorClient<'_>) -> Result<Self> {
        Ok(Self {
            endpoint: operator.installation.operator.resource.to_string(),
            bearer: operator.installation.token().await?,
            ordinary: false,
        })
    }
    #[cfg(test)]
    pub(super) fn controlled(endpoint: &str, ordinary: bool) -> Self {
        Self {
            endpoint: endpoint.into(),
            bearer: "private-fixture-bearer".into(),
            ordinary,
        }
    }
    fn remaining(&self, requested: Duration) -> Result<Duration> {
        if self.ordinary {
            owner::check_effect()?;
            return Ok(requested);
        }
        let remaining = owner::cleanup_deadline()?
            .saturating_duration_since(std::time::Instant::now())
            .min(requested);
        ensure!(!remaining.is_zero(), "Flight cleanup deadline expired");
        Ok(remaining)
    }
}
impl FlightPeer for CleanupPeer {
    async fn call_tool_with_timeout(
        &self,
        tool: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value> {
        tokio::time::timeout(self.remaining(timeout)?, async {
            let session =
                veoveo_testing_support::connect_mcp_client(&self.endpoint, &self.bearer).await?;
            let response = session
                .call_tool_once(
                    rmcp::model::CallToolRequestParams::new(tool.to_owned()).with_arguments(
                        arguments
                            .as_object()
                            .context("Flight cleanup arguments must be object")?
                            .clone(),
                    ),
                )
                .await;
            let closed = session.cancel().await;
            let response = response?;
            closed?;
            let rmcp::model::CallToolResponse::Complete(result) = response else {
                bail!("Flight cleanup must complete synchronously")
            };
            ensure!(
                result.is_error != Some(true),
                "Flight cleanup tool refused its operation"
            );
            result
                .structured_content
                .context("Flight cleanup tool omitted structured content")
        })
        .await
        .context("Flight cleanup request exceeded original grace")?
    }
    async fn resource(&self, uri: &str, timeout: Duration) -> Result<Value> {
        tokio::time::timeout(self.remaining(timeout)?, async {
            let session =
                veoveo_testing_support::connect_mcp_client(&self.endpoint, &self.bearer).await?;
            let value = veoveo_testing_support::read_mcp_resource_json(&session, uri).await;
            let closed = session.cancel().await;
            let value = value?;
            closed?;
            Ok(value)
        })
        .await
        .context("Flight cleanup resource read exceeded original grace")?
    }
}

pub struct StreamOwnership {
    registration: CleanupRegistration,
    session: Arc<Mutex<Option<StreamSessionId>>>,
}
impl StreamOwnership {
    pub async fn before_start(
        operator: &OperatorClient<'_>,
        pipeline_id: &veoveo_stream_mcp::contract::PipelineId,
    ) -> Result<Self> {
        let peer = CleanupPeer::capture(operator).await?;
        let endpoint = peer.endpoint.clone();
        Self::register(peer, &endpoint, pipeline_id)
    }
    pub(super) fn register(
        peer: impl FlightPeer + Send + 'static,
        endpoint: &str,
        pipeline_id: &veoveo_stream_mcp::contract::PipelineId,
    ) -> Result<Self> {
        let session = Arc::new(Mutex::new(None));
        let observed = Arc::clone(&session);
        let identity = serde_json::to_string(&StreamIntent {
            endpoint,
            pipeline_id,
        })?;
        let registration = owner::register_cleanup(
            CleanupKind::Remote,
            "Flight live Stream",
            &identity,
            move || async move {
                let id = observed
                    .lock()
                    .expect("Flight Stream identity")
                    .as_ref()
                    .copied()
                    .context("Stream dispatch outcome unresolved; retain intent")?;
                stop_live_stream_session(&peer, &id, "interrupted Flight cleanup").await
            },
        )?;
        Ok(Self {
            registration,
            session,
        })
    }
    pub fn observed(&self, session: StreamSessionId) -> Result<()> {
        let mut retained = self.session.lock().expect("Flight Stream identity");
        ensure!(
            retained.is_none_or(|prior| prior == session),
            "Flight Stream identity changed"
        );
        self.registration.observed_identity(&session.to_string())?;
        *retained = Some(session);
        Ok(())
    }
    pub fn settled(&self) -> Result<()> {
        self.registration.settled()
    }
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamIntent<'a> {
    endpoint: &'a str,
    pipeline_id: &'a veoveo_stream_mcp::contract::PipelineId,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct VehicleIntent<'a> {
    endpoint: &'a str,
    session_id: &'a veoveo_uav_sim_mcp::contract::SessionId,
    vehicle_id: &'a veoveo_uav_sim_mcp::contract::VehicleId,
    cleanup_command: veoveo_uav_sim_mcp::contract::SimulationCommand,
}
pub async fn before_takeoff(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
) -> Result<VehicleOwnership> {
    let peer = CleanupPeer::capture(operator).await?;
    let endpoint = peer.endpoint.clone();
    register_vehicle(peer, &endpoint, scenario)
}
pub(super) fn register_vehicle(
    peer: impl FlightPeer + Send + 'static,
    endpoint: &str,
    scenario: &UavAcceptanceScenario,
) -> Result<VehicleOwnership> {
    let identity = serde_json::to_string(&VehicleIntent {
        endpoint,
        session_id: &scenario.session_id,
        vehicle_id: &scenario.vehicle_id,
        cleanup_command: veoveo_uav_sim_mcp::contract::SimulationCommand::Land(
            veoveo_uav_sim_mcp::contract::VehicleRequest {
                session_id: scenario.session_id.clone(),
                vehicle_id: scenario.vehicle_id.clone(),
            },
        ),
    })?;
    let scenario = scenario.clone();
    let landing_dispatched = Arc::new(AtomicBool::new(false));
    let retained = Arc::clone(&landing_dispatched);
    let registration = owner::register_cleanup(
        CleanupKind::Remote,
        "Flight vehicle",
        &identity,
        move || async move {
            // Query the retained parent before asking it to land. Unknown dispatch is
            // retained if the parent cannot authoritatively establish safe settlement.
            ensure_vehicle_landed_with_dispatch(
                &peer,
                &scenario,
                "interrupted Flight landing",
                &retained,
            )
            .await
        },
    )?;
    Ok(VehicleOwnership {
        registration,
        landing_dispatched,
    })
}

pub(super) struct VehicleOwnership {
    registration: CleanupRegistration,
    landing_dispatched: Arc<AtomicBool>,
}
impl VehicleOwnership {
    pub async fn land(
        &self,
        peer: &impl FlightPeer,
        scenario: &UavAcceptanceScenario,
    ) -> Result<()> {
        ensure_vehicle_landed_with_dispatch(
            peer,
            scenario,
            "postflight recovery",
            &self.landing_dispatched,
        )
        .await
    }
    pub fn settled(&self) -> Result<()> {
        self.registration.settled()
    }
}

/// Bind the authoritative acknowledgement before publishing an observed effect.
pub(super) fn admit_takeoff_acknowledgement(
    value: Value,
    session: &veoveo_uav_sim_mcp::contract::SessionId,
    vehicle: &veoveo_uav_sim_mcp::contract::VehicleId,
) -> Result<veoveo_uav_sim_mcp::contract::CommandAcknowledgement> {
    admit_vehicle_acknowledgement(value, session, vehicle, "takeoff")
}
pub(super) fn admit_vehicle_acknowledgement(
    value: Value,
    session: &veoveo_uav_sim_mcp::contract::SessionId,
    vehicle: &veoveo_uav_sim_mcp::contract::VehicleId,
    operation: &str,
) -> Result<veoveo_uav_sim_mcp::contract::CommandAcknowledgement> {
    use veoveo_uav_sim_mcp::contract::{CommandAcknowledgement, UavResource};
    let acknowledgement: CommandAcknowledgement = serde_json::from_value(value)?;
    ensure!(acknowledgement.accepted, "Flight {operation} was refused");
    ensure!(
        acknowledgement.resource_uri
            == UavResource::Vehicle {
                session: session.clone(),
                vehicle: vehicle.clone(),
            },
        "Flight {operation} acknowledgement belongs to another vehicle"
    );
    Ok(acknowledgement)
}

/// The actual Flight takeoff effect follows its already retained cleanup intent.
pub(super) async fn dispatch_takeoff(
    peer: &impl FlightPeer,
    scenario: &UavAcceptanceScenario,
    ownership: &VehicleOwnership,
) -> Result<()> {
    let acknowledgement = peer
        .call_tool(
            "uav-sim__takeoff_vehicle",
            serde_json::json!({
                "sessionId": scenario.session_id,
                "vehicleId": scenario.vehicle_id,
                "relativeAltitudeM": scenario.takeoff.relative_altitude_m,
            }),
        )
        .await?;
    let acknowledgement =
        admit_takeoff_acknowledgement(acknowledgement, &scenario.session_id, &scenario.vehicle_id)?;
    ownership
        .registration
        .observed_identity(&serde_json::to_string(&acknowledgement.resource_uri)?)
}

/// Ordinary postflight effects are success-only. Errors enter owner reverse
/// teardown, which owns retained Task cancellation before physical cleanup.
pub(super) async fn finish_flight<T, Land, LandFuture, Stop, StopFuture>(
    result: Result<T>,
    land: Land,
    stop: Stop,
) -> Result<T>
where
    Land: FnOnce() -> LandFuture,
    LandFuture: std::future::Future<Output = Result<()>>,
    Stop: FnOnce() -> StopFuture,
    StopFuture: std::future::Future<Output = Result<()>>,
{
    let value = result?;
    land().await?;
    stop().await?;
    Ok(value)
}
