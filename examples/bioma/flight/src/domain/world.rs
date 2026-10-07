use super::*;
use veoveo_uav_sim_mcp::contract::{SimulationState, VehicleFlightState, VehicleId};

pub struct WorldBinding {
    pub revision_uri: FrameWorldRevisionUri,
    pub simulation_frame_uri: WorldFrameUri,
}

pub async fn ensure_vehicle_landed(
    operator: &impl FlightPeer,
    scenario: &UavAcceptanceScenario,
    phase: &str,
) -> Result<()> {
    let dispatched = std::sync::atomic::AtomicBool::new(false);
    ensure_vehicle_landed_with_dispatch(operator, scenario, phase, &dispatched).await
}

pub(super) async fn ensure_vehicle_landed_with_dispatch(
    operator: &impl FlightPeer,
    scenario: &UavAcceptanceScenario,
    phase: &str,
    dispatched: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    let timeout = Duration::from_secs(scenario.landing_timeout_seconds);
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let access_error = match simulation_state(operator, scenario).await {
            Ok(state) => {
                let state: SimulationState =
                    serde_json::from_value(state).context("admitting Flight landing state")?;
                ensure!(
                    state.session_id == scenario.session_id,
                    "Flight landing state belongs to another session"
                );
                let flight_state = &state
                    .vehicles
                    .iter()
                    .find(|vehicle| vehicle.vehicle_id == scenario.vehicle_id)
                    .context("Flight landing state omitted the selected vehicle")?
                    .flight_state;
                if matches!(
                    flight_state,
                    VehicleFlightState::Landed | VehicleFlightState::Standby
                ) {
                    return Ok(());
                }
                ensure!(
                    flight_state != &VehicleFlightState::Failed,
                    "PX4 entered the failed state during {phase}"
                );
                if flight_state != &VehicleFlightState::Landing
                    && !dispatched.swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    eprintln!("{phase}: landing UAV from `{flight_state:?}`");
                    let acknowledgement = operator
                        .call_tool(
                            "uav-sim__land_vehicle",
                            serde_json::json!({
                                "sessionId": scenario.session_id,
                                "vehicleId": scenario.vehicle_id
                            }),
                        )
                        .await
                        .context("landing dispatch outcome unresolved; retain intent")?;
                    cleanup::admit_vehicle_acknowledgement(
                        acknowledgement,
                        &scenario.session_id,
                        &scenario.vehicle_id,
                        "landing",
                    )?;
                    None
                } else {
                    None
                }
            }
            Err(error) => {
                eprintln!(
                    "{phase}: UAV control plane is temporarily unavailable while ensuring \
                     landing: {error:#}"
                );
                Some(error)
            }
        };
        if tokio::time::Instant::now() >= deadline {
            if let Some(error) = access_error {
                return Err(error.context(format!(
                    "{phase} could not reach the UAV control plane within {timeout:?}"
                )));
            }
            bail!("PX4 did not reach landed or standby during {phase} within {timeout:?}");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub async fn ensure_world_configured(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
) -> Result<WorldBinding> {
    let initial_state = simulation_state(operator, scenario).await?;
    if initial_state
        .pointer("/world")
        .is_some_and(Value::is_object)
    {
        let revision_uri = json_string(&initial_state, "/world/revisionUri")?.parse()?;
        let simulation_frame_uri =
            json_string(&initial_state, "/world/simulationFrameUri")?.parse()?;
        verify_published_world(
            operator,
            scenario,
            &revision_uri,
            &simulation_frame_uri,
            None,
        )
        .await.context("installed UAV world must resolve through Frames; after a database reset, run uav-world-publish and deploy its new binding before flight acceptance")?;
        return Ok(WorldBinding {
            revision_uri,
            simulation_frame_uri,
        });
    }
    ensure!(
        json_string(&initial_state, "/lifecycle")? == "unconfigured",
        "UAV session must begin unconfigured or retain the same immutable binding: {initial_state}"
    );
    let revision = super::world_publication::publish_world_revision(operator, scenario).await?;
    let revision_uri = revision.revision_uri().clone();
    let simulation_frame_uri =
        WorldFrameUri::new(&revision_uri, &scenario.world.simulation_frame_id);
    operator
        .call_tool(
            "uav-sim__configure_world",
            serde_json::json!({
                "sessionId": scenario.session_id,
                "worldRevision": revision,
                "simulationFrameUri": simulation_frame_uri,
            }),
        )
        .await?;
    verify_published_world(
        operator,
        scenario,
        &revision_uri,
        &simulation_frame_uri,
        Some(&revision_uri.world_id()),
    )
    .await?;
    Ok(WorldBinding {
        revision_uri,
        simulation_frame_uri,
    })
}

pub async fn verify_published_world(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    revision_uri: &FrameWorldRevisionUri,
    simulation_frame_uri: &WorldFrameUri,
    expected_world_id: Option<&FrameWorldId>,
) -> Result<FrameWorldRevision> {
    ensure!(
        simulation_frame_uri
            == &WorldFrameUri::new(revision_uri, &scenario.world.simulation_frame_id),
        "UAV immutable binding selects the wrong simulation frame: {simulation_frame_uri}"
    );
    let frame: FrameNode = serde_json::from_str(
        &operator
            .conformance(
                &["resource", simulation_frame_uri.as_str()],
                Duration::from_secs(60),
            )
            .await?,
    )
    .context("decoding the published simulation frame resource")?;
    let expected_frame = scenario
        .world
        .tree
        .frames
        .iter()
        .find(|candidate| candidate.frame_id == scenario.world.simulation_frame_id)
        .expect("validated simulation frame");
    ensure!(
        &frame == expected_frame,
        "published simulation frame disagrees with the scenario: {frame:?}"
    );
    let published_revision: FrameWorldRevision = serde_json::from_str(
        &operator
            .conformance(
                &["resource", revision_uri.as_str()],
                Duration::from_secs(60),
            )
            .await?,
    )
    .context("decoding the published Frames world revision resource")?;
    let mut expected_frames = scenario.world.tree.frames.clone();
    expected_frames.sort_by(|left, right| left.frame_id.cmp(&right.frame_id));
    let mut published_frames = published_revision.tree().frames.clone();
    published_frames.sort_by(|left, right| left.frame_id.cmp(&right.frame_id));
    ensure!(
        published_revision.revision_uri() == revision_uri && published_frames == expected_frames,
        "published Frames world revision disagrees with the complete scenario hierarchy: \
         {published_revision:?}"
    );
    if let Some(expected_world_id) = expected_world_id {
        ensure!(
            &published_revision.world_id() == expected_world_id,
            "published Frames world revision changed its run-scoped identity: \
             {published_revision:?}"
        );
    }
    Ok(published_revision)
}

pub fn camera_product_set_matches_contract(products: &[LiveStreamProductState]) -> bool {
    if products.is_empty() {
        return false;
    }
    let product_ids = products
        .iter()
        .map(|product| &product.stream_product_id)
        .collect::<std::collections::BTreeSet<_>>();
    let camera_ids = products
        .iter()
        .flat_map(|product| {
            product
                .camera_regions
                .iter()
                .map(|region| &region.camera_id)
        })
        .collect::<std::collections::BTreeSet<_>>();
    let camera_region_count = products
        .iter()
        .map(|product| product.camera_regions.len())
        .sum::<usize>();
    product_ids.len() == products.len()
        && camera_ids.len() == camera_region_count
        && products.iter().all(LiveStreamProductState::validate)
        && products.iter().all(|product| match product.lifecycle {
            LiveStreamProductLifecycle::Starting => {
                product.active_viewers == 0
                    && product.connected_viewers == 0
                    && product.nvenc_sessions <= 1
            }
            LiveStreamProductLifecycle::Ready => {
                product.connected_viewers <= product.active_viewers
                    && product.nvenc_sessions == 1
                    && product.encoded_frames > 0
                    && product.last_frame_at.is_some()
                    && product.visible != Some(false)
                    && product.diagnostic.is_none()
            }
            LiveStreamProductLifecycle::Inactive | LiveStreamProductLifecycle::Failed => false,
        })
}

pub fn ready_camera_product_set_matches_contract(products: &[LiveStreamProductState]) -> bool {
    camera_product_set_matches_contract(products)
        && products
            .iter()
            .all(|product| product.lifecycle == LiveStreamProductLifecycle::Ready)
}

pub fn sensor_camera_is_started(camera: &Value) -> bool {
    camera.get("lifecycle").and_then(Value::as_str) == Some("ready")
        && camera.get("transport").and_then(Value::as_str) == Some("rtsp_rtp")
        && camera.get("codec").and_then(Value::as_str) == Some("h264")
        && camera.get("encoder").and_then(Value::as_str) == Some("nvidia_nvenc")
        && camera
            .get("framesObserved")
            .and_then(Value::as_u64)
            .is_some_and(|count| count >= 3)
        && camera
            .get("lastAccessUnitBytes")
            .and_then(Value::as_u64)
            .is_some_and(|bytes| bytes > 0)
}

pub async fn wait_for_world_ready(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    revision_uri: &FrameWorldRevisionUri,
    simulation_frame_uri: &WorldFrameUri,
    timeout: Duration,
) -> Result<Value> {
    let mut pending = "waiting for the first world observation";
    let observation = async {
        loop {
            let state = simulation_state(operator, scenario).await?;
            let typed = serde_json::from_value(state.clone())
                .context("decoding the UAV-owned simulation state")?;
            match super::readiness::world_readiness(
                &typed,
                scenario,
                revision_uri,
                simulation_frame_uri,
            )? {
                super::readiness::WorldReadiness::Ready => return Ok(state),
                super::readiness::WorldReadiness::Warming(reason) => {
                    if pending != reason {
                        eprintln!("UAV world warmup: {reason}");
                    }
                    pending = reason;
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    };
    tokio::time::timeout(timeout, observation)
        .await
        .with_context(|| format!("UAV world was not ready within {timeout:?}: {pending}"))?
}

pub async fn simulation_state(
    operator: &impl FlightPeer,
    scenario: &UavAcceptanceScenario,
) -> Result<Value> {
    const ATTEMPTS: usize = 3;
    let mut last_error = None;
    for attempt in 1..=ATTEMPTS {
        match operator
            .call_tool_with_timeout(
                "uav-sim__get_simulation_state",
                serde_json::json!({"sessionId": scenario.session_id}),
                Duration::from_secs(30),
            )
            .await
        {
            Ok(state) => return Ok(state),
            Err(error) if attempt < ATTEMPTS => {
                eprintln!(
                    "UAV state read attempt {attempt}/{ATTEMPTS} failed; retrying: {error:#}"
                );
                last_error = Some(error);
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.context("UAV state read exhausted its retry budget")?)
}

pub async fn wait_for_recording_catalog(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    timeout: Duration,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let state = simulation_state(operator, scenario).await?;
        let recording = state
            .pointer("/recordings/0")
            .context("UAV state omitted its active recording")?;
        match json_string(recording, "/catalogLifecycle")? {
            "ready" => return Ok(state),
            "pending" | "unavailable" if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            lifecycle => {
                bail!(
                    "UAV recording catalog did not become ready within {timeout:?}: lifecycle={lifecycle}, recording={recording}"
                );
            }
        }
    }
}

pub fn takeoff_is_ready(
    state: &SimulationState,
    vehicle_id: &VehicleId,
    minimum_altitude_m: f64,
) -> Result<bool> {
    let vehicle = state
        .vehicles
        .iter()
        .find(|vehicle| vehicle.vehicle_id == *vehicle_id)
        .context("takeoff observation omitted the selected UAV")?;
    ensure!(
        vehicle.flight_state != VehicleFlightState::Failed,
        "selected UAV entered the failed state during takeoff"
    );
    ensure!(
        vehicle.enu.up_m.is_finite(),
        "selected UAV returned a nonfinite takeoff altitude"
    );
    Ok(
        vehicle.flight_state == VehicleFlightState::Flying
            && vehicle.enu.up_m >= minimum_altitude_m,
    )
}

pub async fn wait_for_takeoff(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    revision: &FrameWorldRevisionUri,
) -> Result<()> {
    let timeout = Duration::from_secs(scenario.takeoff.state_timeout_seconds);
    tokio::time::timeout(timeout, async {
        loop {
            let state: SimulationState =
                serde_json::from_value(simulation_state(operator, scenario).await?)?;
            ensure!(
                state.session_id == scenario.session_id
                    && state
                        .world
                        .as_ref()
                        .is_some_and(|world| world.revision_uri == *revision),
                "takeoff observation belongs to another session or world revision"
            );
            if takeoff_is_ready(
                &state,
                &scenario.vehicle_id,
                scenario.takeoff.minimum_reached_altitude_m,
            )? {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    })
    .await
    .with_context(|| {
        format!(
            "selected UAV did not reach flying at {} m within {timeout:?}",
            scenario.takeoff.minimum_reached_altitude_m
        )
    })?
}

pub async fn wait_for_native_camera_stream(
    operator: &OperatorClient<'_>,
    timeout: Duration,
    scenario: &UavAcceptanceScenario,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let state = simulation_state(operator, scenario).await?;
        let camera = state
            .pointer("/cameras/0")
            .context("UAV state omitted its sensor camera")?;
        if sensor_camera_is_started(camera) {
            return Ok(state);
        }
        ensure!(
            json_string(&state, "/cameras/0/lifecycle")? != "failed",
            "Isaac nadir camera failed before native NVENC output became available: {state}"
        );
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "Isaac nadir camera did not produce native NVENC access units within {timeout:?}; \
                 final state: {state}"
            );
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub async fn ensure_operator_control_grant(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
) -> Result<veoveo_uav_sim_mcp::contract::VehicleControlGrant> {
    let principal_key = operator.installation.operator.principal.to_string();
    let administrator = operator.installation.administrator()?;
    let admin_token = administrator.token().await?;
    use veoveo_uav_sim_mcp::contract::{
        ControlGrantId, GrantVehicleControlRequest, VehicleControlGrant, VehicleControlPermission,
    };
    let request = GrantVehicleControlRequest {
        grant_id: ControlGrantId::parse(format!("acceptance-operator-{}", scenario.vehicle_id))?,
        session_id: scenario.session_id.clone(),
        vehicle_id: scenario.vehicle_id.clone(),
        principal_key: principal_key.clone(),
        permissions: BTreeSet::from([
            VehicleControlPermission::Inspect,
            VehicleControlPermission::Plan,
            VehicleControlPermission::Execute,
            VehicleControlPermission::Abort,
        ]),
        map_mobility_profile_uri: scenario.map_mobility_profile_uri.clone(),
        allow_planning_advisory: true,
        valid_from: "2026-08-13T00:00:00Z".parse()?,
        valid_until: None,
    };
    let arguments = serde_json::to_string(&request)?;
    let granted = gateway_conformance(
        operator.conformance,
        &administrator.resource,
        &admin_token,
        &[
            "call",
            "--tool-name",
            "uav-sim__grant_vehicle_control",
            "--arguments",
            &arguments,
        ],
        Duration::from_secs(120),
    )
    .await?;
    let granted: VehicleControlGrant = serde_json::from_value(
        structured_output(&granted).context("admin control grant returned invalid output")?,
    )?;
    ensure!(
        granted.grant_id == request.grant_id,
        "admin returned a different control grant"
    );
    super::control_grants::find(operator, scenario, &granted.grant_id, &principal_key).await
}

pub fn assert_georeference_origin(state: &Value, scenario: &UavAcceptanceScenario) -> Result<()> {
    let origin = state
        .pointer("/world/georeferenceOrigin")
        .and_then(Value::as_object)
        .context("UAV state omitted georeference_origin")?;
    let expected_origin = scenario.world.origin()?;
    for (key, expected) in [
        ("latitudeDegrees", expected_origin.latitude_degrees),
        ("longitudeDegrees", expected_origin.longitude_degrees),
        ("ellipsoidHeightM", expected_origin.ellipsoid_height_m),
    ] {
        let actual = json_number(origin, key)?;
        ensure!(
            (actual - expected).abs() <= 1e-9,
            "UAV state {key} {actual} disagrees with scenario origin {expected}"
        );
    }
    Ok(())
}

pub fn nearby_mission_position(
    current: &Wgs84Position,
    longitude_offset_degrees: f64,
) -> Result<Wgs84Position> {
    current
        .validate()
        .map_err(anyhow::Error::msg)
        .context("validating the current mission position")?;
    let target = Wgs84Position {
        latitude_degrees: current.latitude_degrees,
        longitude_degrees: current.longitude_degrees + longitude_offset_degrees,
        ellipsoid_height_m: current.ellipsoid_height_m,
    };
    target
        .validate()
        .map_err(anyhow::Error::msg)
        .context("the nearby mission offset left the WGS84 envelope")?;
    Ok(target)
}

pub fn governed_mission_timeout(
    cost: &veoveo_map_mcp::contract::RouteCost,
    speed_mps: f64,
    limit_seconds: u64,
) -> Result<Duration> {
    let distance_m = cost.distance.get();
    let modeled_duration_s = cost.duration.get();
    ensure!(
        speed_mps.is_finite() && speed_mps > 0.0,
        "mission speed must be finite and positive"
    );
    let minimum_flight_duration_s = distance_m / speed_mps;
    let required_seconds = modeled_duration_s
        .max(minimum_flight_duration_s)
        .mul_add(2.0, 60.0)
        .ceil() as u64;
    ensure!(
        required_seconds <= limit_seconds,
        "governed route requires a {required_seconds}-second flight budget, above the configured {limit_seconds}-second acceptance limit"
    );
    Ok(Duration::from_secs(required_seconds.max(1)))
}
