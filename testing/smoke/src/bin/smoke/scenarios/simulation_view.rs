use std::{collections::BTreeMap, process::Stdio};

use anyhow::ensure;
use chrono::{SecondsFormat, Utc};

use super::*;

#[path = "simulation_view/browser.rs"]
pub(super) mod browser;

use browser::{BrowserFixture, GenericAppCaptureEvidence, verify_live_app_in_hardware_browser};

const SIMULATION_VIEW_SCOPES: &[&str] = &[
    "operator:use",
    "simulation-view:read",
    "simulation-view:write",
    "simulation-view:stream",
];

#[derive(Default)]
struct AcceptanceResources {
    session_id: Option<String>,
    session_revision: Option<u64>,
    cameras: Vec<(String, u64)>,
    live_view_id: Option<String>,
    producer_started: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SimulationViewAcceptanceEvidence {
    schema: &'static str,
    completed_at: chrono::DateTime<chrono::Utc>,
    run_id: String,
    context: String,
    namespace: String,
    session_id: String,
    camera_id: String,
    capture: GenericAppCaptureEvidence,
}

pub(crate) struct SimulationViewVerifyRequest<'a> {
    pub(crate) conformance: &'a Path,
    pub(crate) context: &'a str,
    pub(crate) namespace: &'a str,
    pub(crate) public_base_url: &'a str,
    pub(crate) work_context: &'a str,
    pub(crate) chrome_cdp_url: &'a str,
    pub(crate) evidence_root: &'a Path,
    pub(crate) timeout: Duration,
}

pub(crate) async fn simulation_view_verify(request: SimulationViewVerifyRequest<'_>) -> Result<()> {
    let SimulationViewVerifyRequest {
        conformance,
        context,
        namespace,
        public_base_url,
        work_context,
        chrome_cdp_url,
        evidence_root,
        timeout,
    } = request;
    ensure!(
        secure_or_loopback_origin(public_base_url)?,
        "Simulation View live acceptance requires public HTTPS or an exact loopback HTTP origin"
    );
    verify_workloads_and_gpu(context, namespace, timeout)?;

    let token =
        gateway_token_for_simulation_view(conformance, public_base_url, work_context).await?;
    let mcp_url = format!("{public_base_url}/mcp/operator");
    let session = connect_mcp_session(&mcp_url, &token).await?;
    verify_surface(&session).await?;

    let mut resources = AcceptanceResources::default();
    let acceptance = run_acceptance(
        &session,
        context,
        namespace,
        chrome_cdp_url,
        evidence_root,
        timeout,
        &mut resources,
    )
    .await;
    let cleanup = cleanup_acceptance(&session, &mut resources).await;
    let cancellation = session.cancel().await;

    acceptance?;
    cleanup?;
    cancellation?;
    println!(
        "simulation view ok: anonymous poses, multiple admitted cameras, typed capacity rejection, \
         rotating leases, NVIDIA RTX/NVENC WebRTC, and the generic App in headed hardware Chrome"
    );
    Ok(())
}

async fn run_acceptance(
    session: &SmokeMcpSession,
    context: &str,
    namespace: &str,
    chrome_cdp_url: &str,
    evidence_root: &Path,
    timeout: Duration,
    cleanup: &mut AcceptanceResources,
) -> Result<()> {
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let session_id = format!("anonymous-view-{}", &suffix[..16]);
    let epoch_id = format!("epoch-{}", &suffix[..16]);
    let world_id = format!("anonymous-world-{}", &suffix[..16]);
    cleanup.session_id = Some(session_id.clone());

    mcp_call(
        session,
        "frames__create_world",
        serde_json::json!({
            "world_id": world_id,
            "display_name": "Anonymous Simulation View acceptance",
            "description": "Synthetic external producer frame world.",
        }),
    )
    .await?;
    let publication = mcp_call(
        session,
        "frames__publish_world",
        serde_json::json!({
            "world_id": world_id,
            "tree": synthetic_frame_tree(),
        }),
    )
    .await?;
    let revision_uri = json_pointer_string(&publication, "/revision/revision_uri")?.to_owned();
    let spec_sha256 = json_pointer_string(&publication, "/revision/spec_sha256")?;
    ensure!(
        spec_sha256.len() == 64
            && spec_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        "Frames returned an invalid revision digest: {spec_sha256:?}"
    );
    let frame_revision = serde_json::json!({
        "uri": revision_uri,
        "digest": format!("sha256:{spec_sha256}"),
    });
    let simulation_frame = format!("{revision_uri}/frame/simulation");

    let prepared = mcp_call(
        session,
        "anonymous-simulation__prepare_scene",
        serde_json::json!({
            "sessionId": session_id,
            "epochId": epoch_id,
            "frameRevision": frame_revision,
            "simulationFrame": simulation_frame,
        }),
    )
    .await?;
    let scene = prepared
        .get("scene")
        .cloned()
        .context("anonymous producer omitted its scene declaration")?;
    let producer_id = json_pointer_string(&prepared, "/producerId")?.to_owned();
    let producer_spiffe_id = json_pointer_string(&prepared, "/producerSpiffeId")?.to_owned();

    let created = mcp_call(
        session,
        "simulation-view__create_session",
        serde_json::json!({
            "sessionId": session_id,
            "epochId": epoch_id,
        }),
    )
    .await?;
    let mut session_revision = json_pointer_u64(&created, "/revision")?;
    ensure!(
        json_pointer_string(&created, "/lifecycle")? == "created",
        "Simulation View session did not begin in the created state: {created}"
    );

    let bound = mcp_call(
        session,
        "simulation-view__bind_scene",
        serde_json::json!({
            "sessionId": session_id,
            "expectedRevision": session_revision,
            "scene": scene,
        }),
    )
    .await?;
    session_revision = json_pointer_u64(&bound, "/revision")?;
    ensure!(
        json_pointer_string(&bound, "/lifecycle")? == "scene_bound",
        "Simulation View did not bind the governed scene: {bound}"
    );

    let expires_at =
        (Utc::now() + chrono::Duration::minutes(10)).to_rfc3339_opts(SecondsFormat::Millis, true);
    let authorized = mcp_call(
        session,
        "simulation-view__authorize_pose_producer",
        serde_json::json!({
            "sessionId": session_id,
            "expectedRevision": session_revision,
            "producerId": producer_id,
            "spiffeId": producer_spiffe_id,
            "expiresAt": expires_at,
        }),
    )
    .await?;
    ensure!(
        json_pointer_string(&authorized, "/spiffeId")? == producer_spiffe_id,
        "Simulation View authorized a different pose identity: {authorized}"
    );
    session_revision += 1;
    cleanup.session_revision = Some(session_revision);

    mcp_call(
        session,
        "anonymous-simulation__start_pose_producer",
        serde_json::json!({
            "sessionId": session_id,
            "epochId": epoch_id,
            "frameRevision": frame_revision,
            "simulationFrame": simulation_frame,
            "cadenceHz": 30,
        }),
    )
    .await?;
    cleanup.producer_started = true;
    wait_for_pose_producer(session, &session_id, timeout).await?;

    let capacity = read_mcp_resource_json(session, "simulation-view://capacity").await?;
    let limits = capacity
        .get("limits")
        .and_then(Value::as_object)
        .context("Simulation View capacity omitted limits")?;
    let maximum_rendered = object_u64(limits, "maximumRenderedCameras")?;
    let maximum_logical = object_u64(limits, "maximumLogicalCameras")?;
    let maximum_owner = object_u64(limits, "maximumCamerasPerOwner")?;
    let maximum_context = object_u64(limits, "maximumCamerasPerWorkContext")?;
    ensure!(
        maximum_rendered >= 3
            && maximum_logical >= maximum_rendered
            && maximum_owner >= maximum_rendered
            && maximum_context >= maximum_rendered,
        "the measured reference profile must admit at least three cameras and reach its rendered \
         limit before another quota: {capacity}"
    );

    for index in 0..maximum_rendered {
        let admission = mcp_call(
            session,
            "simulation-view__create_camera",
            serde_json::json!({
                "sessionId": session_id,
                "definition": follow_camera_definition(index),
            }),
        )
        .await?;
        ensure!(
            json_pointer_string(&admission, "/status")? == "admitted",
            "camera {index} was not admitted: {admission}"
        );
        let camera_id = json_pointer_string(&admission, "/camera/cameraId")?.to_owned();
        let revision = json_pointer_u64(&admission, "/camera/revision")?;
        cleanup.cameras.push((camera_id, revision));
    }

    let rejection = mcp_call(
        session,
        "simulation-view__create_camera",
        serde_json::json!({
            "sessionId": session_id,
            "definition": follow_camera_definition(maximum_rendered),
        }),
    )
    .await?;
    ensure!(
        json_pointer_string(&rejection, "/status")? == "rejected"
            && json_pointer_string(&rejection, "/rejection/dimension")? == "rendered_cameras"
            && json_pointer_u64(&rejection, "/rejection/requested")? == maximum_rendered + 1
            && json_pointer_u64(&rejection, "/rejection/available")? == maximum_rendered,
        "capacity overflow did not return the exact rendered-camera rejection: {rejection}"
    );

    let cameras_uri = format!("simulation-view://session/{session_id}/cameras");
    let cameras = wait_for_moving_rendered_cameras(session, &cameras_uri, timeout).await?;
    ensure!(
        cameras.as_array().is_some_and(|items| items.len() >= 3),
        "Simulation View did not expose several admitted cameras: {cameras}"
    );

    let primary_camera = cleanup
        .cameras
        .first()
        .map(|(camera_id, _)| camera_id.clone())
        .context("camera admission returned no camera")?;
    let opened = open_live_view_when_ready(session, &session_id, &primary_camera, timeout).await?;
    let live_view_id = json_pointer_string(&opened, "/stream/liveViewId")?.to_owned();
    let first_token = json_pointer_string(&opened, "/accessToken")?.to_owned();
    ensure_live_connection(&opened, &session_id, &primary_camera)?;
    cleanup.live_view_id = Some(live_view_id.clone());

    let stream_uri = format!("simulation-view://session/{session_id}/stream/{live_view_id}");
    assert_resources_redact_token(
        session,
        &[
            "simulation-view://sessions".to_owned(),
            cameras_uri.clone(),
            format!("simulation-view://session/{session_id}/streams"),
            stream_uri.clone(),
        ],
        &first_token,
    )
    .await?;

    let renewed = mcp_call(
        session,
        "simulation-view__renew_live_view",
        serde_json::json!({
            "sessionId": session_id,
            "liveViewId": live_view_id,
        }),
    )
    .await?;
    let renewed_token = json_pointer_string(&renewed, "/accessToken")?.to_owned();
    ensure!(
        renewed_token != first_token,
        "live-view renewal did not rotate the access token"
    );
    ensure_live_connection(&renewed, &session_id, &primary_camera)?;
    assert_resources_redact_token(session, std::slice::from_ref(&stream_uri), &renewed_token)
        .await?;
    assert_cluster_logs_redact_tokens(
        context,
        namespace,
        &[first_token.as_str(), renewed_token.as_str()],
    )?;

    let app_html = read_app_html(session).await?;
    let host_fixture = build_browser_fixture(
        session,
        &session_id,
        &primary_camera,
        app_html,
        renewed.clone(),
    )
    .await?;
    let run_id = uuid::Uuid::now_v7().to_string();
    let evidence_directory = evidence_root.join(&run_id);
    fs::create_dir_all(&evidence_directory).with_context(|| {
        format!(
            "creating Simulation View acceptance evidence directory {}",
            evidence_directory.display()
        )
    })?;
    let capture = verify_live_app_in_hardware_browser(
        chrome_cdp_url,
        host_fixture,
        &evidence_directory.join("simulation-view-app.png"),
        timeout,
    )
    .await?;

    let closed = mcp_call(
        session,
        "simulation-view__close_live_view",
        serde_json::json!({
            "sessionId": session_id,
            "liveViewId": live_view_id,
        }),
    )
    .await?;
    ensure!(
        closed.get("closed").and_then(Value::as_bool) == Some(true),
        "live view did not close: {closed}"
    );
    cleanup.live_view_id = None;
    let closed_state = read_mcp_resource_json(session, &stream_uri).await?;
    ensure!(
        json_pointer_string(&closed_state, "/lifecycle")? == "closed"
            && !serde_json::to_string(&closed_state)?.contains(&renewed_token),
        "closed stream resource was not revoked and redacted: {closed_state}"
    );

    let (camera_id, revision) = cleanup
        .cameras
        .pop()
        .context("camera cleanup inventory was empty")?;
    let closed_camera = mcp_call(
        session,
        "simulation-view__close_camera",
        serde_json::json!({
            "sessionId": session_id,
            "cameraId": camera_id,
            "expectedRevision": revision,
        }),
    )
    .await?;
    ensure!(
        closed_camera.get("closed").and_then(Value::as_bool) == Some(true),
        "logical camera did not close: {closed_camera}"
    );

    mcp_call(
        session,
        "anonymous-simulation__stop_pose_producer",
        serde_json::json!({"sessionId": session_id}),
    )
    .await?;
    cleanup.producer_started = false;
    let closed_session = mcp_call(
        session,
        "simulation-view__close_session",
        serde_json::json!({
            "sessionId": session_id,
            "expectedRevision": session_revision,
        }),
    )
    .await?;
    ensure!(
        closed_session.get("closed").and_then(Value::as_bool) == Some(true),
        "Simulation View session did not close: {closed_session}"
    );
    cleanup.session_id = None;
    cleanup.cameras.clear();
    let evidence = SimulationViewAcceptanceEvidence {
        schema: "veoveo.io/simulation-view-acceptance/v1",
        completed_at: Utc::now(),
        run_id,
        context: context.to_owned(),
        namespace: namespace.to_owned(),
        session_id: session_id.clone(),
        camera_id: primary_camera,
        capture,
    };
    let evidence_path = evidence_directory.join("evidence.json");
    fs::write(&evidence_path, serde_json::to_vec_pretty(&evidence)?).with_context(|| {
        format!(
            "writing Simulation View acceptance evidence {}",
            evidence_path.display()
        )
    })?;
    println!("Simulation View evidence: {}", evidence_path.display());
    Ok(())
}

async fn verify_surface(session: &SmokeMcpSession) -> Result<()> {
    let tools = session.list_tools(Default::default()).await?;
    let names = tools
        .tools
        .iter()
        .map(|tool| tool.name.as_ref())
        .collect::<std::collections::BTreeSet<_>>();
    for name in [
        "frames__create_world",
        "frames__publish_world",
        "anonymous-simulation__prepare_scene",
        "anonymous-simulation__start_pose_producer",
        "anonymous-simulation__stop_pose_producer",
        "simulation-view__create_session",
        "simulation-view__bind_scene",
        "simulation-view__authorize_pose_producer",
        "simulation-view__create_camera",
        "simulation-view__set_camera",
        "simulation-view__close_camera",
        "simulation-view__open_live_view",
        "simulation-view__renew_live_view",
        "simulation-view__close_live_view",
        "simulation-view__get_capacity",
        "simulation-view__get_session_state",
        "simulation-view__close_session",
    ] {
        ensure!(names.contains(name), "operator profile omitted `{name}`");
    }
    Ok(())
}

async fn mcp_call(session: &SmokeMcpSession, name: &str, arguments: Value) -> Result<Value> {
    let arguments = arguments
        .as_object()
        .cloned()
        .context("tool arguments were not an object")?;
    let result = session
        .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(arguments))
        .await?;
    ensure!(
        result.is_error != Some(true),
        "tool `{name}` failed: {:?}",
        result.content
    );
    result
        .structured_content
        .with_context(|| format!("tool `{name}` returned no structured content"))
}

async fn read_app_html(session: &SmokeMcpSession) -> Result<String> {
    let result = session
        .read_resource(ReadResourceRequestParams::new(
            "ui://simulation-view/live.html",
        ))
        .await?;
    let (html, mime_type) = result
        .contents
        .iter()
        .find_map(|content| match content {
            ResourceContents::TextResourceContents {
                text, mime_type, ..
            } => Some((text.clone(), mime_type.clone())),
            _ => None,
        })
        .context("Simulation View App returned no HTML")?;
    ensure!(
        mime_type.as_deref() == Some("text/html;profile=mcp-app"),
        "Simulation View App returned {mime_type:?}"
    );
    ensure!(
        html.contains("OVWebRTC")
            && !html.contains("/*__NVIDIA_OV_WEB_RTC__*/")
            && !html.contains("__VEOVEO_WEBRTC_STUB__=true"),
        "Simulation View App does not contain the production NVIDIA WebRTC client"
    );
    Ok(html)
}

async fn build_browser_fixture(
    session: &SmokeMcpSession,
    session_id: &str,
    primary_camera: &str,
    app_html: String,
    connection: Value,
) -> Result<BrowserFixture> {
    let session_uri = format!("simulation-view://session/{session_id}");
    let camera_uri = format!("{session_uri}/camera/{primary_camera}");
    let camera = read_mcp_resource_json(session, &camera_uri).await?;
    let mut resources = BTreeMap::new();
    resources.insert(
        "simulation-view://capacity".to_owned(),
        read_mcp_resource_json(session, "simulation-view://capacity").await?,
    );
    resources.insert(
        "simulation-view://sessions".to_owned(),
        Value::Array(vec![read_mcp_resource_json(session, &session_uri).await?]),
    );
    resources.insert(format!("{session_uri}/cameras"), Value::Array(vec![camera]));
    resources.insert(
        format!("{session_uri}/scene"),
        read_mcp_resource_json(session, &format!("{session_uri}/scene")).await?,
    );
    resources.insert(
        format!("{session_uri}/streams"),
        read_mcp_resource_json(session, &format!("{session_uri}/streams")).await?,
    );
    Ok(BrowserFixture {
        app_html,
        resources,
        connection,
        expected_camera_id: primary_camera.to_owned(),
    })
}

async fn wait_for_pose_producer(
    session: &SmokeMcpSession,
    session_id: &str,
    timeout: Duration,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let fixture = mcp_call(
            session,
            "anonymous-simulation__get_fixture_state",
            serde_json::json!({}),
        )
        .await?;
        if json_pointer_string(&fixture, "/producer/lifecycle").ok() == Some("running")
            && json_pointer_string(&fixture, "/producer/sessionId").ok() == Some(session_id)
            && fixture
                .pointer("/producer/sentSnapshots")
                .and_then(Value::as_u64)
                .is_some_and(|count| count > 0)
        {
            return Ok(());
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "anonymous pose producer did not reach running state within {timeout:?}: {fixture}"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_moving_rendered_cameras(
    session: &SmokeMcpSession,
    cameras_uri: &str,
    timeout: Duration,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut first_sequence = None;
    loop {
        let cameras = read_mcp_resource_json(session, cameras_uri).await?;
        let items = cameras
            .as_array()
            .context("camera collection was not an array")?;
        let ready = !items.is_empty()
            && items.iter().all(|camera| {
                camera.pointer("/health").and_then(Value::as_str) == Some("healthy")
                    && camera
                        .pointer("/lastPoseSequence")
                        .and_then(Value::as_u64)
                        .is_some_and(|sequence| sequence > 0)
                    && camera
                        .pointer("/lastFrameAt")
                        .and_then(Value::as_str)
                        .is_some()
            });
        if ready {
            let newest = items
                .iter()
                .filter_map(|camera| camera.pointer("/lastPoseSequence").and_then(Value::as_u64))
                .min()
                .context("ready cameras omitted pose sequences")?;
            match first_sequence {
                Some(first) if newest > first => return Ok(cameras),
                None => first_sequence = Some(newest),
                _ => {}
            }
        }
        ensure!(
            tokio::time::Instant::now() < deadline,
            "cameras did not render advancing anonymous poses within {timeout:?}: {cameras}"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn open_live_view_when_ready(
    session: &SmokeMcpSession,
    session_id: &str,
    camera_id: &str,
    timeout: Duration,
) -> Result<Value> {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        let error = match mcp_call(
            session,
            "simulation-view__open_live_view",
            serde_json::json!({
                "sessionId": session_id,
                "cameraId": camera_id,
            }),
        )
        .await
        {
            Ok(connection) => return Ok(connection),
            Err(error) => error,
        };
        if tokio::time::Instant::now() >= deadline {
            return Err(error.context(format!(
                "Simulation View did not open a ready RTX/NVENC stream within {timeout:?}"
            )));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn ensure_live_connection(connection: &Value, session_id: &str, camera_id: &str) -> Result<()> {
    ensure!(
        json_pointer_string(connection, "/stream/schemaVersion")? == "veoveo.io/live-view/v1"
            && json_pointer_string(connection, "/stream/sessionId")? == session_id
            && json_pointer_string(connection, "/stream/cameraId")? == camera_id
            && json_pointer_string(connection, "/stream/lifecycle")? == "ready"
            && json_pointer_string(connection, "/stream/codec")? == "h264"
            && json_pointer_string(connection, "/stream/hardwareEncoder")? == "nvidia_nvenc"
            && json_pointer_string(connection, "/stream/endpoint/transport")? == "web_rtc"
            && veoveo_mcp_contract::is_valid_live_signaling_url(json_pointer_string(
                connection,
                "/stream/endpoint/signalingUrl",
            )?)
            && json_pointer_u64(connection, "/stream/endpoint/mediaPort")? > 0,
        "live-view connection does not identify ready NVIDIA H.264 WebRTC: {connection}"
    );
    let token = json_pointer_string(connection, "/accessToken")?;
    ensure!(
        (32..=512).contains(&token.len()) && !token.chars().any(char::is_whitespace),
        "live-view connection returned an invalid secret token"
    );
    Ok(())
}

async fn assert_resources_redact_token(
    session: &SmokeMcpSession,
    uris: &[String],
    token: &str,
) -> Result<()> {
    for uri in uris {
        let value = read_mcp_resource_json(session, uri).await?;
        let encoded = serde_json::to_string(&value)?;
        ensure!(
            !encoded.contains(token)
                && !encoded.contains("\"accessToken\"")
                && !encoded.contains("\"access_token\""),
            "resource `{uri}` leaked a live-view access token"
        );
    }
    Ok(())
}

fn assert_cluster_logs_redact_tokens(
    context: &str,
    namespace: &str,
    tokens: &[&str],
) -> Result<()> {
    for deployment in ["simulation-view-mcp", "simulation-view-renderer"] {
        let logs = run_checked(
            Path::new("kubectl"),
            [
                "--context",
                context,
                "-n",
                namespace,
                "logs",
                &format!("deployment/{deployment}"),
                "--all-containers=true",
                "--tail=2000",
            ]
            .map(OsString::from),
            [],
        )?;
        for token in tokens {
            ensure!(
                !logs.contains(token),
                "{deployment} logs leaked a live-view access token"
            );
        }
    }
    Ok(())
}

async fn cleanup_acceptance(
    session: &SmokeMcpSession,
    resources: &mut AcceptanceResources,
) -> Result<()> {
    let Some(session_id) = resources.session_id.clone() else {
        return Ok(());
    };
    let mut first_error = None;
    if let Some(live_view_id) = resources.live_view_id.take()
        && let Err(error) = mcp_call(
            session,
            "simulation-view__close_live_view",
            serde_json::json!({
                "sessionId": session_id,
                "liveViewId": live_view_id,
            }),
        )
        .await
    {
        first_error = Some(error);
    }
    if resources.producer_started {
        if let Err(error) = mcp_call(
            session,
            "anonymous-simulation__stop_pose_producer",
            serde_json::json!({"sessionId": session_id}),
        )
        .await
        {
            first_error.get_or_insert(error);
        }
        resources.producer_started = false;
    }
    if let Some(revision) = resources.session_revision
        && let Err(error) = mcp_call(
            session,
            "simulation-view__close_session",
            serde_json::json!({
                "sessionId": session_id,
                "expectedRevision": revision,
            }),
        )
        .await
    {
        first_error.get_or_insert(error);
    }
    resources.session_id = None;
    resources.cameras.clear();
    if let Some(error) = first_error {
        return Err(error.context("Simulation View acceptance cleanup failed"));
    }
    Ok(())
}

fn verify_workloads_and_gpu(context: &str, namespace: &str, timeout: Duration) -> Result<()> {
    run_checked(
        Path::new("kubectl"),
        ["--context", context, "cluster-info"].map(OsString::from),
        [],
    )
    .context("Simulation View acceptance requires its Kubernetes cluster")?;
    let timeout_arg = format!("--timeout={}s", timeout.as_secs());
    for deployment in [
        "simulation-view-renderer",
        "simulation-view-mcp",
        "anonymous-simulation-mcp",
    ] {
        run_checked(
            Path::new("kubectl"),
            [
                "--context",
                context,
                "-n",
                namespace,
                "rollout",
                "status",
                &format!("deployment/{deployment}"),
                &timeout_arg,
            ]
            .map(OsString::from),
            [],
        )
        .with_context(|| format!("{deployment} did not become ready"))?;
    }
    let gpu = run_checked(
        Path::new("kubectl"),
        [
            "--context",
            context,
            "-n",
            namespace,
            "exec",
            "deployment/simulation-view-renderer",
            "-c",
            "simulation-view-isaac",
            "--",
            "nvidia-smi",
            "--query-gpu=name,uuid,driver_version",
            "--format=csv,noheader",
        ]
        .map(OsString::from),
        [],
    )?;
    let fingerprint = gpu.to_ascii_lowercase();
    ensure!(
        !gpu.trim().is_empty()
            && fingerprint.contains("nvidia")
            && !fingerprint.contains("software")
            && !fingerprint.contains("llvmpipe"),
        "Simulation View renderer did not expose an NVIDIA hardware GPU: {gpu}"
    );

    let deployments: Value = serde_json::from_str(&run_checked(
        Path::new("kubectl"),
        [
            "--context",
            context,
            "-n",
            namespace,
            "get",
            "deployment",
            "simulation-view-renderer",
            "anonymous-simulation-mcp",
            "-o",
            "json",
        ]
        .map(OsString::from),
        [],
    )?)?;
    let items = deployments
        .get("items")
        .and_then(Value::as_array)
        .context("Kubernetes deployment query omitted items")?;
    for item in items {
        let name = json_pointer_string(item, "/metadata/name")?;
        let gpu_limits = item
            .pointer("/spec/template/spec/containers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|container| container.pointer("/resources/limits/nvidia.com~1gpu"))
            .filter_map(|value| {
                value
                    .as_str()
                    .and_then(|value| value.parse::<u64>().ok())
                    .or_else(|| value.as_u64())
            })
            .sum::<u64>();
        match name {
            "simulation-view-renderer" => {
                ensure!(
                    gpu_limits == 1,
                    "Simulation View renderer must request exactly one NVIDIA GPU"
                );
                let renderer = item
                    .pointer("/spec/template/spec/containers")
                    .and_then(Value::as_array)
                    .and_then(|containers| {
                        containers.iter().find(|container| {
                            container.get("name").and_then(Value::as_str)
                                == Some("simulation-view-isaac")
                        })
                    })
                    .context("Simulation View deployment omitted the Isaac container")?;
                ensure!(
                    renderer
                        .pointer("/securityContext/readOnlyRootFilesystem")
                        .and_then(Value::as_bool)
                        == Some(true),
                    "Simulation View Isaac root filesystem must remain read-only"
                );
                let environment = renderer
                    .get("env")
                    .and_then(Value::as_array)
                    .context("Simulation View Isaac container omitted its environment")?;
                for (variable, expected) in [
                    ("HOME", "/var/lib/veoveo/runtime-cache/simulation-view/home"),
                    (
                        "MPLCONFIGDIR",
                        "/var/lib/veoveo/runtime-cache/simulation-view/matplotlib",
                    ),
                    (
                        "WARP_CACHE_PATH",
                        "/var/lib/veoveo/runtime-cache/simulation-view/warp",
                    ),
                    (
                        "XDG_CACHE_HOME",
                        "/var/lib/veoveo/runtime-cache/simulation-view/xdg-cache",
                    ),
                    (
                        "XDG_DATA_HOME",
                        "/var/lib/veoveo/runtime-cache/simulation-view/xdg-data",
                    ),
                ] {
                    let actual = environment.iter().find_map(|entry| {
                        (entry.get("name").and_then(Value::as_str) == Some(variable))
                            .then(|| entry.get("value").and_then(Value::as_str))
                            .flatten()
                    });
                    ensure!(
                        actual == Some(expected),
                        "Simulation View Isaac {variable} must resolve beneath its writable runtime cache"
                    );
                }
            }
            "anonymous-simulation-mcp" => ensure!(
                gpu_limits == 0,
                "anonymous pose producer must remain CPU-only"
            ),
            _ => unreachable!("Kubernetes returned an unrequested deployment"),
        }
    }
    Ok(())
}

async fn gateway_token_for_simulation_view(
    conformance: &Path,
    base: &str,
    work_context: &str,
) -> Result<String> {
    let token_url = format!("{base}/oauth/token");
    let resource = format!("{base}/mcp/operator");
    let mut command = tokio::process::Command::new(conformance);
    command
        .args([
            "gateway-token-exchange",
            "--token-url",
            &token_url,
            "--client-id",
            "operator-service",
            "--audience",
            &token_url,
            "--resource",
            &resource,
            "--work-context",
            work_context,
        ])
        .args(
            SIMULATION_VIEW_SCOPES
                .iter()
                .flat_map(|scope| ["--scope", *scope])
                .collect::<Vec<_>>(),
        )
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = tokio::time::timeout(Duration::from_secs(60), command.output())
        .await
        .context("Simulation View gateway token exchange timed out")??;
    ensure!(
        output.status.success(),
        "Simulation View gateway token exchange failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let token = String::from_utf8(output.stdout)?.trim().to_owned();
    ensure!(!token.is_empty(), "gateway returned an empty access token");
    Ok(token)
}

fn synthetic_frame_tree() -> Value {
    serde_json::json!({
        "frames": [
            {
                "frame_id": "earth-ecef",
                "basis": {"kind": "ecef_wgs84"},
                "description": "Earth-centered, Earth-fixed WGS84 root."
            },
            {
                "frame_id": "acceptance-enu",
                "basis": {"kind": "enu"},
                "parent_frame_id": "earth-ecef",
                "parent_transform": {
                    "kind": "geodetic_tangent",
                    "origin": {
                        "latitude_degrees": 13.6929,
                        "longitude_degrees": -89.2182,
                        "ellipsoid_height_m": 700.0
                    }
                },
                "description": "Local ENU frame for independent Simulation View acceptance."
            },
            {
                "frame_id": "simulation",
                "basis": {"kind": "enu"},
                "parent_frame_id": "acceptance-enu",
                "parent_transform": {
                    "kind": "static_rigid",
                    "translation_m": [0.0, 0.0, 0.0],
                    "rotation_xyzw": [0.0, 0.0, 0.0, 1.0]
                },
                "description": "Anonymous producer simulation frame."
            }
        ]
    })
}

fn follow_camera_definition(index: u64) -> Value {
    let lateral = (index as f64 - 1.5) * 1.25;
    serde_json::json!({
        "rig": {
            "kind": "follow_entity",
            "targetEntity": if index.is_multiple_of(2) {
                "entity-1"
            } else {
                "entity-2"
            },
            "offsetFluM": {"x": -6.0 - index as f64, "y": lateral, "z": 2.5},
            "smoothingSeconds": 0.15
        },
        "widthPx": 640,
        "heightPx": 360,
        "frameRateMillihertz": 30000,
        "verticalFovDegrees": 60.0,
        "nearClipM": 0.05,
        "farClipM": 1000.0,
        "streamPolicy": "on_demand",
        "recordingPolicy": "disabled"
    })
}

fn json_pointer_string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("JSON output omitted string {pointer}: {value}"))
}

fn json_pointer_u64(value: &Value, pointer: &str) -> Result<u64> {
    value
        .pointer(pointer)
        .and_then(Value::as_u64)
        .with_context(|| format!("JSON output omitted integer {pointer}: {value}"))
}

fn object_u64(value: &serde_json::Map<String, Value>, key: &str) -> Result<u64> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .with_context(|| format!("JSON object omitted integer `{key}`"))
}

fn secure_or_loopback_origin(value: &str) -> Result<bool> {
    let origin = url::Url::parse(value).context("parsing Simulation View public origin")?;
    ensure!(
        origin.username().is_empty()
            && origin.password().is_none()
            && origin.path() == "/"
            && origin.query().is_none()
            && origin.fragment().is_none(),
        "Simulation View public origin must not contain credentials, a path, query, or fragment"
    );
    if origin.scheme() == "https" {
        return Ok(true);
    }
    if origin.scheme() != "http" {
        return Ok(false);
    }
    Ok(match origin.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(address)) => address.is_loopback(),
        Some(url::Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    })
}

#[cfg(test)]
mod tests {
    use super::{follow_camera_definition, secure_or_loopback_origin};

    #[test]
    fn follow_camera_uses_the_tagged_rig_wire_shape() {
        let definition = follow_camera_definition(0);
        assert_eq!(
            definition
                .pointer("/rig/targetEntity")
                .and_then(|v| v.as_str()),
            Some("entity-1")
        );
        assert!(definition.pointer("/rig/target_entity").is_none());
        assert!(definition.pointer("/rig/offsetFluM").is_some());
        assert!(definition.pointer("/rig/smoothingSeconds").is_some());
    }

    #[test]
    fn public_origin_is_https_or_exact_loopback_http() {
        assert!(secure_or_loopback_origin("https://simulation.example").unwrap());
        assert!(secure_or_loopback_origin("http://localhost:8782").unwrap());
        assert!(secure_or_loopback_origin("http://127.0.0.1:8782").unwrap());
        assert!(secure_or_loopback_origin("http://[::1]:8782").unwrap());
        assert!(!secure_or_loopback_origin("http://simulation.example").unwrap());
        assert!(secure_or_loopback_origin("http://localhost/path").is_err());
    }
}
