use std::process::Stdio;

use anyhow::ensure;

use super::*;

const NAMESPACE: &str = "veoveo";
const SESSION_ID: &str = "bioma-uav";
const FRAME_URI: &str = "frames://frame/bioma-uav-origin";
const GOOGLE_PHOTOREALISTIC_3D_TILES_ASSET_ID: u64 = 2_275_207;
const ACCEPTANCE_ALTITUDE_M: f64 = 120.0;

pub(crate) async fn uav_sim_verify(
    conformance: &Path,
    context: &str,
    public_base_url: &str,
) -> Result<()> {
    if conformance == Path::new("target/debug/conformance") {
        run_checked(
            Path::new("cargo"),
            [
                "build".into(),
                "-p".into(),
                "veoveo-mcp-conformance".into(),
                "--bin".into(),
                "conformance".into(),
            ],
            [],
        )?;
    }
    assert_executable(conformance)?;
    let public_base_url = public_base_url.trim_end_matches('/');
    let public = url::Url::parse(public_base_url).context("parsing public Bioma URL")?;
    ensure!(
        public.scheme() == "https",
        "UAV live acceptance requires public HTTPS"
    );

    run_checked(
        Path::new("kubectl"),
        ["--context", context, "cluster-info"].map(OsString::from),
        [],
    )
    .context("UAV live acceptance requires the Bioma Kubernetes cluster")?;
    assert_concurrent_gpu_workloads(context)?;

    let token = gateway_token(conformance, public_base_url).await?;
    let info = gateway_conformance(
        conformance,
        public_base_url,
        &token,
        &["info"],
        Duration::from_secs(60),
    )
    .await?;
    for tool in [
        "uav-sim__get_simulation_state",
        "uav-sim__execute_mission",
        "perception__analyze_recording",
        "recording__query_recording",
    ] {
        contains(&info, tool)?;
    }

    let frame = gateway_conformance(
        conformance,
        public_base_url,
        &token,
        &["resource", FRAME_URI],
        Duration::from_secs(60),
    )
    .await?;
    for expected in ["bioma-uav-origin", "13.6929", "-89.2182", "700.0", "enu"] {
        contains(&frame, expected)?;
    }

    let mut state = simulation_state(conformance, public_base_url, &token).await?;
    assert_world_ready(&state)?;
    let recording_uri = json_string(&state, "/recordings/0/recording_uri")?.to_owned();
    let recording_id = recording_uri
        .strip_prefix("recording://recordings/")
        .context("UAV state returned a non-canonical recording URI")?;
    ensure!(
        uuid::Uuid::parse_str(recording_id)?.get_version_num() == 7,
        "UAV recording identity must be UUIDv7"
    );
    let camera_entity = json_string(&state, "/recordings/0/camera_streams/0")?.to_owned();

    call_tool(
        conformance,
        public_base_url,
        &token,
        "uav-sim__arm_vehicle",
        serde_json::json!({"session_id": SESSION_ID, "vehicle_id": "uav-1"}),
    )
    .await?;
    wait_for_flight_state(
        conformance,
        public_base_url,
        &token,
        &["armed"],
        Duration::from_secs(60),
    )
    .await?;
    call_tool(
        conformance,
        public_base_url,
        &token,
        "uav-sim__takeoff_vehicle",
        serde_json::json!({
            "session_id": SESSION_ID,
            "vehicle_id": "uav-1",
            "relative_altitude_m": ACCEPTANCE_ALTITUDE_M
        }),
    )
    .await?;
    state = wait_for_flight_state(
        conformance,
        public_base_url,
        &token,
        &["flying"],
        Duration::from_secs(180),
    )
    .await?;
    ensure!(
        state
            .pointer("/vehicles/0/enu/up_m")
            .and_then(Value::as_f64)
            .is_some_and(|up_m| up_m >= ACCEPTANCE_ALTITUDE_M - 5.0),
        "UAV did not reach the 120 m aerial-tiles acceptance altitude: {state}"
    );
    state = wait_for_aerial_camera_content(
        conformance,
        public_base_url,
        &token,
        Duration::from_secs(60),
    )
    .await?;

    let origin = state
        .get("georeference_origin")
        .and_then(Value::as_object)
        .context("UAV state omitted georeference_origin")?;
    let latitude = json_number(origin, "latitude_degrees")?;
    let longitude = json_number(origin, "longitude_degrees")?;
    let height = json_number(origin, "ellipsoid_height_m")?;
    let mission = serde_json::json!({
        "session_id": SESSION_ID,
        "mission_id": format!("acceptance-{}", uuid::Uuid::now_v7()),
        "frame_uri": FRAME_URI,
        "vehicles": [{
            "vehicle_id": "uav-1",
            "waypoints": [{
                "position": {
                    "latitude_degrees": latitude,
                    "longitude_degrees": longitude + 0.00002,
                    "ellipsoid_height_m": height + ACCEPTANCE_ALTITUDE_M
                },
                "speed_mps": 3.0,
                "hold_seconds": 0.5
            }]
        }]
    });
    let mission_output = task_tool(
        conformance,
        public_base_url,
        &token,
        "uav-sim__execute_mission",
        mission,
        Duration::from_secs(1_200),
    )
    .await?;
    ensure!(
        json_string(&mission_output, "/lifecycle")? == "completed"
            && mission_output
                .get("completed_waypoints")
                .and_then(Value::as_u64)
                .is_some_and(|count| count >= 1),
        "UAV mission did not complete a waypoint: {mission_output}"
    );

    let recording = call_tool(
        conformance,
        public_base_url,
        &token,
        "recording__query_recording",
        serde_json::json!({
            "recording_id": recording_id,
            "entities": "/world/uav-sim/**",
            "timeline": "physics_step",
            "max_rows": 100
        }),
    )
    .await?;
    ensure!(
        recording
            .get("rows_by_recording")
            .and_then(Value::as_object)
            .is_some_and(|rows| rows
                .values()
                .any(|count| count.as_u64().is_some_and(|count| count > 0))),
        "Recording Hub returned no UAV world rows: {recording}"
    );

    state = simulation_state(conformance, public_base_url, &token).await?;
    let simulation_time_s = state
        .get("simulation_time_s")
        .and_then(Value::as_f64)
        .context("UAV state omitted simulation_time_s")?;
    let range_start = ((simulation_time_s + 5.0) * 1_000_000_000.0) as i64;
    let range_end = ((simulation_time_s + 30.0) * 1_000_000_000.0) as i64;
    let perception = task_tool(
        conformance,
        public_base_url,
        &token,
        "perception__analyze_recording",
        serde_json::json!({
            "video": {
                "recording_uri": recording_uri,
                "entity_path": camera_entity,
                "timeline": "simulation_time",
                "range": {"start": range_start, "end": range_end},
                "source": {"mode": "recent_proxy", "idle_ms": 5_000, "capture_ms": 30_000}
            },
            "pipeline_id": "traffic-object-detection",
            "sampling": {"mode": "maximum_frames", "count": 8},
            "include_source_clip": false
        }),
        Duration::from_secs(600),
    )
    .await?;
    ensure!(
        perception
            .pointer("/summary/processed_frames")
            .and_then(Value::as_u64)
            .is_some_and(|count| count > 0),
        "Perception processed no Isaac camera frames: {perception}"
    );

    call_tool(
        conformance,
        public_base_url,
        &token,
        "uav-sim__land_vehicle",
        serde_json::json!({"session_id": SESSION_ID, "vehicle_id": "uav-1"}),
    )
    .await?;
    wait_for_flight_state(
        conformance,
        public_base_url,
        &token,
        &["landed", "standby"],
        Duration::from_secs(300),
    )
    .await?;
    assert_concurrent_gpu_workloads(context)?;

    println!(
        "UAV simulation acceptance ok: Google Photorealistic 3D Tiles were resident in Isaac, PX4 completed a mission, Recording Hub retained the world, Perception processed the camera stream, and View remained available"
    );
    Ok(())
}

fn assert_concurrent_gpu_workloads(context: &str) -> Result<()> {
    for deployment in ["uav-sim", "view-mcp", "perception-mcp"] {
        run_checked(
            Path::new("kubectl"),
            [
                "--context".into(),
                context.into(),
                "-n".into(),
                NAMESPACE.into(),
                "rollout".into(),
                "status".into(),
                format!("deployment/{deployment}").into(),
                "--timeout=30m".into(),
            ],
            [],
        )
        .with_context(|| format!("{deployment} is not concurrently available"))?;
    }
    Ok(())
}

fn assert_world_ready(state: &Value) -> Result<()> {
    ensure!(
        matches!(
            json_string(state, "/lifecycle")?,
            "ready" | "running" | "paused"
        ),
        "UAV session is not ready: {state}"
    );
    ensure!(
        json_string(state, "/frame_uri")? == FRAME_URI,
        "UAV session uses the wrong Frames identity: {state}"
    );
    ensure!(
        json_string(state, "/tiles/source")? == "google_photorealistic_3d_tiles"
            && state.pointer("/tiles/ion_asset_id").and_then(Value::as_u64)
                == Some(GOOGLE_PHOTOREALISTIC_3D_TILES_ASSET_ID)
            && json_string(state, "/tiles/lifecycle")? == "ready"
            && state
                .pointer("/tiles/resident_tiles")
                .and_then(Value::as_u64)
                .is_some_and(|count| count > 0),
        "Google Photorealistic 3D Tiles are not resident inside Isaac: {state}"
    );
    ensure!(
        state
            .pointer("/vehicles/0/px4_connected")
            .and_then(Value::as_bool)
            == Some(true),
        "PX4 is not connected: {state}"
    );
    ensure!(
        json_string(state, "/cameras/0/lifecycle")? == "ready"
            && state
                .pointer("/cameras/0/frames_observed")
                .and_then(Value::as_u64)
                .is_some_and(|count| count >= 3)
            && state
                .pointer("/cameras/0/mean_luma")
                .and_then(Value::as_f64)
                .is_some_and(|value| value >= 2.0)
            && state
                .pointer("/cameras/0/non_black_fraction")
                .and_then(Value::as_f64)
                .is_some_and(|value| value >= 0.02),
        "Isaac nadir camera is not operational: {state}"
    );
    Ok(())
}

async fn simulation_state(conformance: &Path, base: &str, token: &str) -> Result<Value> {
    call_tool(
        conformance,
        base,
        token,
        "uav-sim__get_simulation_state",
        serde_json::json!({"session_id": SESSION_ID}),
    )
    .await
}

async fn wait_for_flight_state(
    conformance: &Path,
    base: &str,
    token: &str,
    accepted: &[&str],
    timeout: Duration,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let state = simulation_state(conformance, base, token).await?;
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

async fn wait_for_aerial_camera_content(
    conformance: &Path,
    base: &str,
    token: &str,
    timeout: Duration,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let state = simulation_state(conformance, base, token).await?;
        let camera_has_detail = state
            .pointer("/cameras/0/mean_luma")
            .and_then(Value::as_f64)
            .is_some_and(|value| value >= 2.0)
            && state
                .pointer("/cameras/0/dynamic_range")
                .and_then(Value::as_u64)
                .is_some_and(|value| value >= 8)
            && state
                .pointer("/cameras/0/non_black_fraction")
                .and_then(Value::as_f64)
                .is_some_and(|value| value >= 0.02);
        if camera_has_detail {
            return Ok(state);
        }
        ensure!(
            json_string(&state, "/cameras/0/lifecycle")? != "failed",
            "Isaac nadir camera failed before aerial content became visible: {state}"
        );
        if tokio::time::Instant::now() >= deadline {
            bail!(
                "Isaac nadir camera did not show detailed Google tiles within {timeout:?}; \
                 final state: {state}"
            );
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn call_tool(
    conformance: &Path,
    base: &str,
    token: &str,
    tool: &str,
    arguments: Value,
) -> Result<Value> {
    let arguments = serde_json::to_string(&arguments)?;
    let output = gateway_conformance(
        conformance,
        base,
        token,
        &["call", "--tool-name", tool, "--arguments", &arguments],
        Duration::from_secs(120),
    )
    .await?;
    structured_output(&output).with_context(|| format!("tool {tool} returned invalid output"))
}

async fn task_tool(
    conformance: &Path,
    base: &str,
    token: &str,
    tool: &str,
    arguments: Value,
    timeout: Duration,
) -> Result<Value> {
    let arguments = serde_json::to_string(&arguments)?;
    let output = gateway_conformance(
        conformance,
        base,
        token,
        &["task-call", "--tool-name", tool, "--arguments", &arguments],
        timeout,
    )
    .await?;
    structured_output(&output).with_context(|| format!("task tool {tool} returned invalid output"))
}

async fn gateway_token(conformance: &Path, base: &str) -> Result<String> {
    let token_url = format!("{base}/oauth/token");
    let resource = format!("{base}/mcp/operator");
    let mut command = tokio::process::Command::new(conformance);
    command
        .args([
            "gateway-token-exchange",
            "--token-url",
            &token_url,
            "--audience",
            &token_url,
            "--resource",
            &resource,
            "--scope",
            "operator:use",
        ])
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(Duration::from_secs(60), command.output())
        .await
        .context("gateway token exchange timed out")??;
    ensure!(
        output.status.success(),
        "gateway token exchange failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let token = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(!token.is_empty(), "gateway returned an empty access token");
    Ok(token)
}

async fn gateway_conformance(
    conformance: &Path,
    base: &str,
    token: &str,
    operation: &[&str],
    timeout: Duration,
) -> Result<String> {
    let url = format!("{base}/mcp/operator");
    let mut command = tokio::process::Command::new(conformance);
    command
        .args(["--url", &url, "--scheme", "uav-sim"])
        .args(operation)
        .env_remove("VEOVEO_INTERNAL_SIGNING_KEY_DER_B64")
        .env("MCP_BEARER_TOKEN", token)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .with_context(|| format!("conformance operation {operation:?} timed out"))??;
    ensure!(
        output.status.success(),
        "conformance operation {operation:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).context("decoding conformance output")
}

fn structured_output(output: &str) -> Result<Value> {
    let encoded = output
        .lines()
        .find_map(|line| line.strip_prefix("structured: "))
        .with_context(|| format!("conformance output omitted structured content:\n{output}"))?;
    serde_json::from_str(encoded).context("decoding structured MCP output")
}

fn json_string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("JSON output omitted string {pointer}: {value}"))
}

fn json_number(object: &serde_json::Map<String, Value>, key: &str) -> Result<f64> {
    object
        .get(key)
        .and_then(Value::as_f64)
        .with_context(|| format!("georeference_origin omitted numeric {key}"))
}
