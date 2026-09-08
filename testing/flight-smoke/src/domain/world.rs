use super::*;

pub(super) struct WorldBinding {
    pub(super) revision_uri: String,
    pub(super) simulation_frame_uri: String,
}

pub(super) async fn ensure_vehicle_landed(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    phase: &str,
) -> Result<()> {
    let timeout = Duration::from_secs(scenario.landing_timeout_seconds);
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let access_error = match simulation_state(operator, scenario).await {
            Ok(state) => {
                let flight_state = json_string(&state, "/vehicles/0/flight_state")?;
                if matches!(flight_state, "landed" | "standby") {
                    return Ok(());
                }
                ensure!(
                    flight_state != "failed",
                    "PX4 entered the failed state during {phase}: {state}"
                );
                if flight_state != "landing" {
                    eprintln!("{phase}: landing UAV from `{flight_state}`");
                    operator
                        .call_tool(
                            "uav-sim__land_vehicle",
                            serde_json::json!({
                                "session_id": scenario.session_id,
                                "vehicle_id": scenario.vehicle_id
                            }),
                        )
                        .await
                        .err()
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

pub(super) async fn ensure_world_configured(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
) -> Result<WorldBinding> {
    let initial_state = simulation_state(operator, scenario).await?;
    if initial_state
        .pointer("/world")
        .is_some_and(Value::is_object)
    {
        let revision_uri = json_string(&initial_state, "/world/revision_uri")?.to_owned();
        let simulation_frame_uri =
            json_string(&initial_state, "/world/simulation_frame_uri")?.to_owned();
        verify_published_world(
            operator,
            scenario,
            &revision_uri,
            &simulation_frame_uri,
            None,
        )
        .await?;
        return Ok(WorldBinding {
            revision_uri,
            simulation_frame_uri,
        });
    }
    ensure!(
        json_string(&initial_state, "/lifecycle")? == "unconfigured",
        "UAV session must begin unconfigured or retain the same immutable binding: {initial_state}"
    );
    let tree_digest = hex::encode(Sha256::digest(serde_json::to_vec(&scenario.world.tree)?));
    let world_id = FrameWorldId::new(format!(
        "{}-{}",
        scenario.world.world_id,
        &tree_digest[..16]
    ))?;
    operator
        .call_tool(
            "frames__create_world",
            serde_json::json!({
                "world_id": world_id,
                "display_name": scenario.world.display_name,
                "description": scenario.world.description,
            }),
        )
        .await?;
    let publication = operator
        .call_tool(
            "frames__publish_world",
            serde_json::json!({
                "world_id": world_id,
                "tree": scenario.world.tree,
            }),
        )
        .await?;
    let revision = publication
        .get("revision")
        .cloned()
        .context("Frames publication omitted its immutable revision")?;
    let revision_uri = json_string(&publication, "/revision/revision_uri")?.to_owned();
    let simulation_frame_uri = format!(
        "{revision_uri}/frame/{}",
        scenario.world.simulation_frame_id
    );
    operator
        .call_tool(
            "uav-sim__configure_world",
            serde_json::json!({
                "session_id": scenario.session_id,
                "world_revision": revision,
                "simulation_frame_uri": simulation_frame_uri,
            }),
        )
        .await?;
    verify_published_world(
        operator,
        scenario,
        &revision_uri,
        &simulation_frame_uri,
        Some(&world_id),
    )
    .await?;
    Ok(WorldBinding {
        revision_uri,
        simulation_frame_uri,
    })
}

pub(super) async fn verify_published_world(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    revision_uri: &str,
    simulation_frame_uri: &str,
    expected_world_id: Option<&FrameWorldId>,
) -> Result<()> {
    ensure!(
        simulation_frame_uri
            == format!(
                "{revision_uri}/frame/{}",
                scenario.world.simulation_frame_id
            ),
        "UAV immutable binding selects the wrong simulation frame: {simulation_frame_uri}"
    );
    let frame: FrameNode = serde_json::from_str(
        &operator
            .conformance(&["resource", simulation_frame_uri], Duration::from_secs(60))
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
            .conformance(&["resource", revision_uri], Duration::from_secs(60))
            .await?,
    )
    .context("decoding the published Frames world revision resource")?;
    let mut expected_frames = scenario.world.tree.frames.clone();
    expected_frames.sort_by(|left, right| left.frame_id.cmp(&right.frame_id));
    let mut published_frames = published_revision.tree.frames.clone();
    published_frames.sort_by(|left, right| left.frame_id.cmp(&right.frame_id));
    ensure!(
        published_revision.revision_uri.as_str() == revision_uri
            && published_frames == expected_frames,
        "published Frames world revision disagrees with the complete scenario hierarchy: \
         {published_revision:?}"
    );
    if let Some(expected_world_id) = expected_world_id {
        ensure!(
            &published_revision.world_id == expected_world_id,
            "published Frames world revision changed its run-scoped identity: \
             {published_revision:?}"
        );
    }
    Ok(())
}

pub(super) fn assert_world_ready(
    state: &Value,
    revision_uri: &str,
    simulation_frame_uri: &str,
) -> Result<()> {
    ensure!(
        matches!(
            json_string(state, "/lifecycle")?,
            "ready" | "running" | "paused"
        ),
        "UAV session is not ready: {state}"
    );
    ensure!(
        json_string(state, "/world/revision_uri")? == revision_uri
            && json_string(state, "/world/simulation_frame_uri")? == simulation_frame_uri
            && json_string(state, "/world/spec_sha256")?.len() == 64,
        "UAV session uses the wrong immutable Frames world: {state}"
    );
    ensure!(
        json_string(state, "/tiles/source")? == "google_photorealistic_3d_tiles"
            && state.pointer("/tiles/ion_asset_id").and_then(Value::as_u64)
                == Some(GOOGLE_PHOTOREALISTIC_3D_TILES_ASSET_ID)
            && json_string(state, "/tiles/lifecycle")? == "ready"
            && state
                .pointer("/tiles/resident_tiles")
                .and_then(Value::as_u64)
                .is_some_and(|count| count > 0)
            && state
                .pointer("/tiles/visible_tiles")
                .and_then(Value::as_u64)
                .is_some_and(|count| count > 0),
        "Google Photorealistic 3D Tiles do not cover the current Isaac viewport: {state}"
    );
    ensure!(
        state
            .pointer("/vehicles/0/px4_connected")
            .and_then(Value::as_bool)
            == Some(true),
        "PX4 is not connected: {state}"
    );
    let sensor_camera = state
        .pointer("/cameras/0")
        .context("authoritative simulator state omitted its sensor camera")?;
    ensure!(
        sensor_camera_is_started(sensor_camera),
        "Isaac nadir camera is not producing native NVENC access units: {state}"
    );
    let live_cameras: Vec<LiveCameraDescriptor> = serde_json::from_value(
        state
            .get("live_cameras")
            .cloned()
            .context("authoritative simulator state omitted live_cameras")?,
    )
    .context("authoritative simulator returned invalid live_cameras")?;
    ensure!(
        !live_cameras.is_empty()
            && live_cameras
                .iter()
                .all(|camera| camera.validate().is_ok()
                    && camera.health == LiveCameraHealth::Healthy),
        "authoritative simulator cameras are not healthy: {state}"
    );
    let products: Vec<LiveStreamProductState> = serde_json::from_value(
        state
            .get("stream_products")
            .cloned()
            .context("authoritative simulator state omitted stream_products")?,
    )
    .context("authoritative simulator returned invalid stream_products")?;
    ensure!(
        camera_product_set_matches_contract(&products),
        "authoritative simulator tiled camera product violates its shared-stream contract: {state}"
    );
    Ok(())
}

pub(super) fn camera_product_set_matches_contract(products: &[LiveStreamProductState]) -> bool {
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

pub(super) fn ready_camera_product_set_matches_contract(
    products: &[LiveStreamProductState],
) -> bool {
    camera_product_set_matches_contract(products)
        && products
            .iter()
            .all(|product| product.lifecycle == LiveStreamProductLifecycle::Ready)
}

pub(super) fn sensor_camera_is_started(camera: &Value) -> bool {
    camera.get("lifecycle").and_then(Value::as_str) == Some("ready")
        && camera.get("transport").and_then(Value::as_str) == Some("rtsp_rtp")
        && camera.get("codec").and_then(Value::as_str) == Some("h264")
        && camera.get("encoder").and_then(Value::as_str) == Some("nvidia_nvenc")
        && camera
            .get("frames_observed")
            .and_then(Value::as_u64)
            .is_some_and(|count| count >= 3)
        && camera
            .get("last_access_unit_bytes")
            .and_then(Value::as_u64)
            .is_some_and(|bytes| bytes > 0)
}

pub(super) async fn wait_for_world_ready(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
    revision_uri: &str,
    simulation_frame_uri: &str,
    timeout: Duration,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let state = simulation_state(operator, scenario).await?;
        let lifecycle = json_string(&state, "/lifecycle")?;
        ensure!(
            lifecycle != "failed",
            "UAV simulation failed while loading its frame world: {state}"
        );
        if matches!(lifecycle, "ready" | "running" | "paused") {
            assert_world_ready(&state, revision_uri, simulation_frame_uri)?;
            return Ok(state);
        }
        if tokio::time::Instant::now() >= deadline {
            bail!("UAV frame world was not ready within {timeout:?}; final state: {state}");
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

pub(super) async fn simulation_state(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
) -> Result<Value> {
    const ATTEMPTS: usize = 3;
    let mut last_error = None;
    for attempt in 1..=ATTEMPTS {
        match operator
            .call_tool_with_timeout(
                "uav-sim__get_simulation_state",
                serde_json::json!({"session_id": scenario.session_id}),
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

pub(super) async fn wait_for_recording_catalog(
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
        match json_string(recording, "/catalog_lifecycle")? {
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

pub(super) async fn wait_for_flight_state(
    operator: &OperatorClient<'_>,
    accepted: &[&str],
    timeout: Duration,
    scenario: &UavAcceptanceScenario,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let state = simulation_state(operator, scenario).await?;
        let flight_state = json_string(&state, "/vehicles/0/flight_state")?;
        if accepted.contains(&flight_state) {
            return Ok(state);
        }
        ensure!(
            flight_state != "failed",
            "PX4 entered the failed state: {state}"
        );
        if tokio::time::Instant::now() >= deadline {
            bail!("PX4 did not reach {accepted:?} within {timeout:?}; final state: {state}");
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

pub(super) async fn wait_for_native_camera_stream(
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

pub(super) async fn ensure_operator_control_grant(
    operator: &OperatorClient<'_>,
    scenario: &UavAcceptanceScenario,
) -> Result<Value> {
    let principal_key = format!("{}/oauth#operator-service", operator.base);
    let admin_token = gateway_token_for_context(
        operator.conformance,
        operator.base,
        "admin-service",
        "admin",
        &["operator:use", "admin:manage", "uav-sim:admin"],
        "operations",
    )
    .await?;
    let arguments = serde_json::to_string(&serde_json::json!({
        "grant_id": format!("acceptance-operator-{}", scenario.vehicle_id),
        "session_id": scenario.session_id,
        "vehicle_id": scenario.vehicle_id,
        "principal_key": principal_key,
        "permissions": ["inspect", "plan", "execute", "abort"],
        "map_mobility_profile_uri": scenario.map_mobility_profile_uri,
        "allow_planning_advisory": true,
        "valid_from": "2026-08-13T00:00:00Z"
    }))?;
    let granted = gateway_conformance(
        operator.conformance,
        operator.base,
        "admin",
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
    let granted =
        structured_output(&granted).context("admin control grant returned invalid output")?;

    let visible = operator
        .call_tool(
            "uav-sim__list_active_vehicle_control_grants",
            serde_json::json!({ "session_id": scenario.session_id }),
        )
        .await?;
    let grant = visible
        .as_array()
        .and_then(|grants| {
            grants.iter().find(|grant| {
                grant.get("grant_id") == granted.get("grant_id")
                    && grant.get("principal_key").and_then(Value::as_str)
                        == Some(principal_key.as_str())
                    && grant.get("vehicle_id").and_then(Value::as_str)
                        == Some(scenario.vehicle_id.as_str())
            })
        })
        .context("operator profile did not expose its active UAV control grant")?;
    ensure!(
        grant
            .get("permissions")
            .and_then(Value::as_array)
            .is_some_and(|permissions| {
                ["inspect", "plan", "execute", "abort"]
                    .into_iter()
                    .all(|required| permissions.iter().any(|permission| permission == required))
            })
            && grant
                .get("map_mobility_profile_uri")
                .and_then(Value::as_str)
                == Some(scenario.map_mobility_profile_uri.as_str()),
        "operator UAV control grant does not carry the canonical permissions and Map profile: {grant}"
    );
    Ok(grant.clone())
}

pub(super) fn parse_mobility_profile_uri(value: &str) -> Result<(&str, u64)> {
    let rest = value
        .strip_prefix("map://mobility-profile/")
        .context("mobility profile must use the canonical Map URI")?;
    let (profile_id, version) = rest
        .split_once('/')
        .context("mobility profile URI must include one exact version")?;
    ensure!(
        !profile_id.is_empty() && !version.contains('/'),
        "mobility profile URI must identify exactly one profile version"
    );
    Ok((
        profile_id,
        version
            .parse()
            .context("mobility profile URI version must be an unsigned integer")?,
    ))
}

pub(super) fn map_position(position: &Wgs84Position) -> Value {
    serde_json::json!({
        "longitude_deg": position.longitude_degrees,
        "latitude_deg": position.latitude_degrees,
        "ellipsoidal_height_m": position.ellipsoid_height_m
    })
}

pub(super) fn assert_georeference_origin(
    state: &Value,
    scenario: &UavAcceptanceScenario,
) -> Result<()> {
    let origin = state
        .pointer("/world/georeference_origin")
        .and_then(Value::as_object)
        .context("UAV state omitted georeference_origin")?;
    let expected_origin = scenario.world.origin()?;
    for (key, expected) in [
        ("latitude_degrees", expected_origin.latitude_degrees),
        ("longitude_degrees", expected_origin.longitude_degrees),
        ("ellipsoid_height_m", expected_origin.ellipsoid_height_m),
    ] {
        let actual = json_number(origin, key)?;
        ensure!(
            (actual - expected).abs() <= 1e-9,
            "UAV state {key} {actual} disagrees with scenario origin {expected}"
        );
    }
    Ok(())
}

pub(super) fn nearby_mission_position(
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

pub(super) fn governed_mission_timeout(
    route: &Value,
    speed_mps: f64,
    limit_seconds: u64,
) -> Result<Duration> {
    let distance_m = route
        .pointer("/summary/distance")
        .and_then(Value::as_f64)
        .context("governed Map route omitted its summary distance")?;
    let modeled_duration_s = route
        .pointer("/summary/duration")
        .and_then(Value::as_f64)
        .context("governed Map route omitted its summary duration")?;
    ensure!(
        distance_m.is_finite()
            && distance_m >= 0.0
            && modeled_duration_s.is_finite()
            && modeled_duration_s >= 0.0,
        "governed Map route returned invalid cost quantities: {route}"
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
