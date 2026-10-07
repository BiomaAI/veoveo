use super::*;
use anyhow::ensure;
use base64::engine::general_purpose::STANDARD;
use chrono::{TimeDelta, Utc};
use glam::{DMat4, DVec3, DVec4};
use rmcp::model::{CallToolRequestParams, ContentBlock};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalSigningKey, GatewayInternalTokenIssuer,
    GatewayProfileId, Principal, PrincipalKind, ServerSlug, TokenIssuer, TokenSubject,
};
use veoveo_types::{
    AccessSubject, InvocationProvenance, PolicyVersion, PrincipalId, ScopeName, TenantId,
    WorkContextId,
};
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel, WorkContextOutputPolicy};
const LOCAL_LAYER: &str = "gpu-smoke";

const GOOGLE_LAYER: &str = "google-photorealistic";

const STATUE_LATITUDE: f64 = 40.689_249_4;

const STATUE_LONGITUDE: f64 = -74.044_500_4;

const STATUE_HEIGHT_METERS: f64 = 20.0;

pub(crate) async fn view_mcp(view_image: &str, retained_frame: Option<&Path>) -> Result<()> {
    inspect_view_image(view_image)?;
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());
    let fixture_dir = tmpdir.join("fixtures");
    let catalog = write_local_fixture(&fixture_dir)?;
    let platform = spawn_platform_store_smoke().await?;
    let running = start_view_container(
        view_image,
        &catalog,
        Some(&fixture_dir),
        &platform,
        false,
        None,
        None,
    )
    .await?;

    assert_http_status(
        &format!("{}/view/mcp", running.base),
        StatusCode::UNAUTHORIZED,
    )
    .await?;
    let token_a = issue_view_token("view-smoke-a")?;
    let token_b = issue_view_token("view-smoke-b")?;
    let session_a = connect_mcp_client(&format!("{}/view/mcp", running.base), &token_a).await?;
    let session_b = connect_mcp_client(&format!("{}/view/mcp", running.base), &token_b).await?;
    let tools = session_a.list_tools(Default::default()).await?;
    for name in [
        "create_scene_composition",
        "create_view",
        "set_camera",
        "capture_frame",
        "close_view",
    ] {
        let tool = tools
            .tools
            .iter()
            .find(|tool| tool.name.as_ref() == name)
            .with_context(|| format!("View MCP did not list `{name}`: {tools:?}"))?;
        let tool_json = serde_json::to_value(tool)?;
        ensure!(
            tool_json
                .pointer("/_meta/ui/resourceUri")
                .and_then(Value::as_str)
                == Some("ui://view/preview.html"),
            "`{name}` is not linked to the preview app: {tool_json}"
        );
        let structured_property = match name {
            "create_view" | "set_camera" => Some("camera"),
            "capture_frame" => Some("policy"),
            _ => None,
        };
        if let Some(property) = structured_property {
            let property_schema = tool_json
                .pointer(&format!("/inputSchema/properties/{property}"))
                .with_context(|| format!("`{name}` omitted `{property}` schema: {tool_json}"))?;
            ensure!(
                property_schema["type"] == "object" && property_schema.get("$ref").is_none(),
                "`{name}.{property}` did not expose an inline object schema: {property_schema}"
            );
        }
        if name == "create_scene_composition" {
            let base_layer = tool_json
                .pointer("/inputSchema/properties/baseLayer")
                .context("create_scene_composition omitted base_layer schema")?;
            ensure!(
                base_layer["enum"] == json!([LOCAL_LAYER]) && base_layer["default"] == LOCAL_LAYER,
                "create_scene_composition did not advertise the runtime layer identifier: {base_layer}"
            );
            ensure!(
                base_layer["description"]
                    .as_str()
                    .is_some_and(|description| description.contains("view://layers")),
                "base_layer schema omitted catalog recovery guidance: {base_layer}"
            );
        }
    }
    assert_preview_app_resource(&session_a).await?;

    let invalid_layer = session_a
        .call_tool(
            CallToolRequestParams::new("create_scene_composition".to_owned()).with_arguments(
                serde_json::from_value(json!({
                    "schemaVersion": 2,
                    "baseLayer": "invented-layer-name",
                    "styleId": "view-smoke:invalid-layer"
                }))?,
            ),
        )
        .await
        .expect_err("an unknown layer identifier must fail");
    let invalid_layer = invalid_layer.to_string();
    ensure!(
        invalid_layer.contains(LOCAL_LAYER) && invalid_layer.contains("view://layers"),
        "unknown-layer error omitted exact recovery guidance: {invalid_layer}"
    );

    let first_composition = create_composition(&session_a, LOCAL_LAYER, true).await?;
    let second_composition = create_composition(&session_b, LOCAL_LAYER, false).await?;
    ensure!(
        read_mcp_resource_json(
            &session_b,
            json_string(&first_composition, "/compositionUri")?,
        )
        .await
        .is_err(),
        "one owner read another owner's scene composition"
    );
    let first = call_structured(
        &session_a,
        "create_view",
        json!({
            "compositionId": first_composition["compositionId"],
            "camera": local_camera()
        }),
    )
    .await?;
    let second_camera = local_camera();
    let second = call_structured(
        &session_b,
        "create_view",
        json!({
            "compositionId": second_composition["compositionId"],
            "camera": serde_json::to_string(&second_camera)?
        }),
    )
    .await?;
    let first_id = json_string(&first, "/viewId")?;
    let second_id = json_string(&second, "/viewId")?;
    ensure!(
        first_id != second_id,
        "two owners received the same view id"
    );
    ensure!(
        read_mcp_resource_json(&session_b, &format!("view://view/{first_id}"))
            .await
            .is_err(),
        "one owner read another owner's view"
    );
    let first = call_structured(
        &session_a,
        "set_camera",
        json!({"viewId": first_id, "expectedRevision": 1, "camera": local_camera()}),
    )
    .await?;
    ensure!(
        first["revision"] == 2,
        "camera revision did not advance: {first}"
    );
    let second_resource =
        read_mcp_resource_json(&session_b, &format!("view://view/{second_id}")).await?;
    ensure!(second_resource["revision"] == 1);

    let scene_uri =
        format!("view://view/{first_id}/scene?width_px=256&height_px=256&max_screen_error_px=8");
    let scene = read_mcp_resource_json(&session_a, &scene_uri).await?;
    ensure!(
        scene["viewRevision"] == 2,
        "scene revision mismatch: {scene}"
    );
    let scene_tiles = scene["tiles"].as_array().context("scene omitted tiles")?;
    ensure!(!scene_tiles.is_empty(), "scene manifest listed no tiles");
    for tile in scene_tiles {
        let matrix = tile["ecefFromContent"]
            .as_array()
            .context("tile omitted ecef_from_content")?;
        ensure!(matrix.len() == 16);
        ensure!(
            matrix
                .iter()
                .all(|value| value.as_f64().is_some_and(f64::is_finite)),
            "tile transform is not finite: {tile}"
        );
    }
    let tile_uri = json_string(&scene_tiles[0], "/tileUri")?;
    let tile_bytes = read_blob_resource(&session_a, tile_uri, "model/gltf-binary").await?;
    ensure!(
        tile_bytes.starts_with(b"glTF"),
        "preview tile blob is not a GLB container"
    );
    ensure!(
        read_mcp_resource_json(&session_b, &scene_uri)
            .await
            .is_err(),
        "one owner read another owner's scene manifest"
    );

    let mut first_bytes = None;
    for (index, (session, token, view)) in [
        (&session_a, &token_a, &first),
        (&session_b, &token_b, &second),
    ]
    .into_iter()
    .enumerate()
    {
        let view_id = json_string(view, "/viewId")?;
        let revision = view["revision"].as_u64().context("view omitted revision")?;
        let mut reference_pixels = None;
        for (encoding, mime) in [("png", "image/png"), ("jpeg", "image/jpeg")] {
            let mut request = capture_request(view_id, revision, false);
            request["policy"]["encoding"] = json!(encoding);
            let payload =
                FinalTaskSmokeClient::new(&format!("{}/view/mcp", running.base), token.clone())
                    .run_tool("capture_frame", request, Duration::from_secs(30))
                    .await?;
            let record = payload
                .structured_content
                .as_ref()
                .context("capture task omitted frame metadata")?;
            let bytes = image_bytes(&payload, mime)?;
            assert_local_frame(record, &bytes, mime)?;
            let pixels = image::load_from_memory(&bytes)?.to_rgb8();
            if encoding == "png" {
                reference_pixels = Some(pixels);
            } else {
                let reference = reference_pixels
                    .as_ref()
                    .context("local PNG comparison reference missing")?;
                ensure!(pixels.dimensions() == reference.dimensions());
                let error: u64 = pixels
                    .as_raw()
                    .iter()
                    .zip(reference.as_raw())
                    .map(|(actual, expected)| u64::from(actual.abs_diff(*expected)))
                    .sum();
                ensure!(
                    error <= pixels.as_raw().len() as u64 * 8,
                    "GPU JPEG changed stored sRGB channels beyond compression tolerance"
                );
            }
            let expected_composition = if index == 0 {
                &first_composition
            } else {
                &second_composition
            };
            ensure!(
                record["compositionId"] == expected_composition["compositionId"]
                    && record["compositionDigestSha256"]
                        == expected_composition["compositionDigestSha256"],
                "capture did not retain exact composition provenance: {record}"
            );
            ensure!(
                record["outputDigestSha256"] == hex::encode(Sha256::digest(&bytes)),
                "capture output digest does not match image bytes"
            );
            ensure!(
                record["renderedOverlayCount"] == if index == 0 { 4 } else { 0 },
                "capture overlay count is wrong: {record}"
            );
            if index == 0 {
                let resource_bytes =
                    read_blob_resource(session, json_string(record, "/frameUri")?, mime).await?;
                ensure!(resource_bytes == bytes);
                if encoding == "png" {
                    first_bytes = Some(bytes);
                }
            }
        }
    }
    if let Some(output) = retained_frame {
        write_retained_frame(
            output,
            &first_bytes.context("first frame was not retained")?,
        )?;
        println!("retained local frame: {}", output.display());
    }
    assert_encoder_completions(&running, 2)?;
    session_a.cancel().await?;
    session_b.cancel().await?;
    qualify_view_lifecycle(
        view_image,
        &catalog,
        &fixture_dir,
        &platform,
        running,
        &first,
        &first_composition,
        &tmpdir,
    )
    .await?;
    cleanup.remove_on_drop();
    println!(
        "View MCP local hardware fixture ok: NVIDIA capture, ownership, retained-lease restart and graceful shutdown; gateway OAuth acceptance is separate"
    );
    Ok(())
}

pub(crate) async fn view_google_live(view_image: &str, output: &Path) -> Result<()> {
    ensure!(
        std::env::var_os("GOOGLE_MAPS_API_KEY").is_some(),
        "GOOGLE_MAPS_API_KEY must be set"
    );
    inspect_view_image(view_image)?;
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());
    let catalog = fs::canonicalize("configs/view/layers.json")?;
    let platform = spawn_platform_store_smoke().await?;
    let running =
        start_view_container(view_image, &catalog, None, &platform, true, None, None).await?;
    let token = issue_view_token("view-google-live")?;
    let session = connect_mcp_client(&format!("{}/view/mcp", running.base), &token).await?;
    let composition = create_composition(&session, GOOGLE_LAYER, false).await?;
    let view = call_structured(
        &session,
        "create_view",
        json!({
            "compositionId": composition["compositionId"],
            "camera": {
                "kind": "orbit_target",
                "target": {
                    "latitudeDegrees": STATUE_LATITUDE,
                    "longitudeDegrees": STATUE_LONGITUDE,
                    "ellipsoidalHeightMeters": STATUE_HEIGHT_METERS
                },
                "distanceMeters": 650.0,
                "azimuthDegrees": 210.0,
                "elevationDegrees": 40.0,
                "verticalFovDegrees": 45.0
            }
        }),
    )
    .await?;
    let payload = FinalTaskSmokeClient::new(&format!("{}/view/mcp", running.base), token.clone())
        .run_tool(
            "capture_frame",
            capture_request(
                json_string(&view, "/viewId")?,
                view["revision"].as_u64().context("view omitted revision")?,
                true,
            ),
            Duration::from_secs(300),
        )
        .await?;
    let record = payload
        .structured_content
        .as_ref()
        .context("Google capture omitted frame metadata")?;
    let bytes = image_bytes(&payload, "image/jpeg")?;
    ensure!(bytes.starts_with(&[0xff, 0xd8, 0xff]));
    ensure!(record["widthPx"] == 1280 && record["heightPx"] == 720);
    ensure!(record["visibleTileCount"].as_u64().unwrap_or_default() > 0);
    ensure!(record["pendingTileCount"].as_u64().unwrap_or_default() == 0);
    ensure!(materially_different_pixels(&bytes)? > 10_000);
    let resource_bytes =
        read_blob_resource(&session, json_string(record, "/frameUri")?, "image/jpeg").await?;
    ensure!(resource_bytes == bytes);
    write_retained_frame(output, &bytes)?;
    let digest = Sha256::digest(&bytes);
    println!(
        "{}",
        serde_json::to_string(&json!({
            "adapter": running.adapter["name"],
            "backend": running.adapter["backend"],
            "deviceType": running.adapter["deviceType"],
            "target": {
                "latitudeDegrees": STATUE_LATITUDE,
                "longitudeDegrees": STATUE_LONGITUDE,
                "ellipsoidalHeightMeters": STATUE_HEIGHT_METERS
            },
            "frame": record,
            "bytes": bytes.len(),
            "sha256": hex::encode(digest),
            "proofImage": output,
        }))?
    );
    session.cancel().await?;
    drop(running);
    cleanup.remove_on_drop();
    Ok(())
}

fn view_container_id(running: &RunningView) -> Result<String> {
    let cid = fs::read_to_string(running._container.cid_file())?;
    let cid = cid.trim();
    ensure!(
        cid.len() == 64 && cid.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "owned View CID is invalid"
    );
    Ok(cid.to_owned())
}

fn view_logs(running: &RunningView) -> Result<String> {
    let logs = run_raw(
        Path::new("docker"),
        ["logs".into(), view_container_id(running)?.into()],
        [],
    )?;
    ensure!(logs.status.success(), "owned View logs unavailable");
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&logs.stdout),
        String::from_utf8_lossy(&logs.stderr)
    ))
}

fn assert_encoder_completions(running: &RunningView, minimum: usize) -> Result<()> {
    assert_encoder_completion_logs(running, minimum, &view_logs(running)?)
}

fn assert_encoder_completion_logs(running: &RunningView, minimum: usize, logs: &str) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct EncoderCompletion {
        message: String,
        encoder: String,
        cuda_device_uuid: String,
        width: u32,
        height: u32,
        encoded_frames: u64,
    }
    let completed = logs
        .lines()
        .filter_map(|line| serde_json::from_str::<EncoderCompletion>(line).ok())
        .filter(|event| {
            event.message == "View GPU JPEG completed"
                && event.encoder == "nvjpeg_cuda_gpu"
                && Some(event.cuda_device_uuid.as_str())
                    == running.adapter["cudaDeviceUuid"].as_str()
                && event.width == 256
                && event.height == 256
                && event.encoded_frames > 0
        })
        .count();
    ensure!(
        completed >= minimum,
        "View did not report {minimum} completed GPU JPEG encodes on its admitted UUID"
    );
    Ok(())
}

// Each Docker command consumes the remaining lifecycle deadline. The maintained
// output owner kills and reaps its subprocess when this future is cancelled.
async fn lifecycle_docker(
    args: impl IntoIterator<Item = OsString>,
    deadline: tokio::time::Instant,
) -> Result<String> {
    let remaining = deadline
        .checked_duration_since(tokio::time::Instant::now())
        .context("View lifecycle command deadline expired")?;
    let mut command = tokio::process::Command::new("docker");
    command.args(args);
    let output = veoveo_testing_support::process::output_async(command, remaining).await?;
    docker_stdout(output)
}

fn docker_stdout(output: std::process::Output) -> Result<String> {
    ensure!(
        output.status.success(),
        "owned View Docker command failed: {}",
        docker_diagnostics(&output)
    );
    String::from_utf8(output.stdout).context("View Docker stdout is not UTF-8")
}

fn docker_diagnostics(output: &std::process::Output) -> String {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .replace(SURREAL_RUNTIME_PASSWORD, "[REDACTED]");
    text.chars().take(4096).collect()
}

async fn lifecycle_logs(running: &RunningView, deadline: tokio::time::Instant) -> Result<String> {
    lifecycle_docker_logs(view_container_id(running)?.into(), deadline).await
}

async fn lifecycle_docker_logs(cid: OsString, deadline: tokio::time::Instant) -> Result<String> {
    let remaining = deadline
        .checked_duration_since(tokio::time::Instant::now())
        .context("View lifecycle logs deadline expired")?;
    let mut command = tokio::process::Command::new("docker");
    command.args(["logs".into(), cid]);
    let output = veoveo_testing_support::process::output_async(command, remaining).await?;
    ensure!(
        output.status.success(),
        "owned View Docker logs failed: {}",
        docker_diagnostics(&output)
    );
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

#[cfg(test)]
mod docker_output_tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[tokio::test]
    async fn successful_docker_stdout_survives_stderr_warnings() {
        for stdout in [
            "a".repeat(64),
            r#"{"Running":false,"ExitCode":70}"#.to_owned(),
        ] {
            let mut command = tokio::process::Command::new("/bin/sh");
            command.args([
                "-c",
                r#"printf '%s' "$1"; printf 'Docker warning\n' >&2"#,
                "docker-output",
                &stdout,
            ]);
            let output =
                veoveo_testing_support::process::output_async(command, Duration::from_secs(2))
                    .await
                    .unwrap();
            assert!(!output.stderr.is_empty());
            let clean = docker_stdout(output).unwrap();
            assert_eq!(clean, stdout);
            if clean.starts_with('{') {
                let state: Value = serde_json::from_str(&clean).unwrap();
                assert_eq!(state["ExitCode"], 70);
            } else {
                assert_eq!(clean.len(), 64);
                assert!(clean.bytes().all(|byte| byte.is_ascii_hexdigit()));
            }
        }
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(256),
            stdout: vec![],
            stderr: format!("{}{}", SURREAL_RUNTIME_PASSWORD, "x".repeat(5000)).into_bytes(),
        };
        let failure = docker_stdout(output).unwrap_err().to_string();
        assert!(failure.contains("[REDACTED]"));
        assert!(!failure.contains(SURREAL_RUNTIME_PASSWORD));
        assert!(failure.len() < 4200);
    }
}

fn require_isolation_rejection<T>(
    result: std::result::Result<T, rmcp::ServiceError>,
    message: &'static str,
) -> Result<()> {
    match result {
        Err(rmcp::ServiceError::McpError(error))
            if error.code == rmcp::model::ErrorCode::INVALID_PARAMS && error.message == message =>
        {
            Ok(())
        }
        _ => bail!("caller isolation did not return its declared peer policy rejection"),
    }
}

#[cfg(test)]
mod lifecycle_controls {
    use super::*;
    #[test]
    fn caller_isolation_requires_domain_peer_rejection() {
        for message in ["unknown task id", "unknown View resource"] {
            require_isolation_rejection::<()>(
                Err(rmcp::ServiceError::McpError(
                    rmcp::ErrorData::invalid_params(message, None),
                )),
                message,
            )
            .unwrap();
            for error in [
                rmcp::ServiceError::TransportClosed,
                rmcp::ServiceError::McpError(rmcp::ErrorData::internal_error(message, None)),
                rmcp::ServiceError::McpError(rmcp::ErrorData::parse_error(message, None)),
                rmcp::ServiceError::McpError(rmcp::ErrorData::invalid_params(
                    "unrelated admission error",
                    None,
                )),
            ] {
                assert!(require_isolation_rejection::<()>(Err(error), message).is_err());
            }
            assert!(require_isolation_rejection(Ok(()), message).is_err());
        }
    }
}

async fn lifecycle_task_row(
    store: &veoveo_platform_store::PlatformStore,
    task: veoveo_types::TaskId,
) -> Result<veoveo_platform_store::TaskRecord> {
    // Only this scenario's admitted Task identity enters the database read. No writes
    // to leases, status or capture snapshots are permitted in this qualification.
    let row: Option<veoveo_platform_store::TaskRecord> = store
        .client()
        .select(veoveo_platform_store::task_record_id(task))
        .await?;
    row.context("owned capture Task disappeared")
}

fn unfinished_claim(row: &veoveo_platform_store::TaskRecord) -> bool {
    use veoveo_platform_store::TaskStatus;
    matches!(
        row.status,
        TaskStatus::Queued | TaskStatus::Running | TaskStatus::Waiting
    ) && row.lease_owner.is_some()
        && row
            .lease_expires_at
            .is_some_and(|expiry| expiry > Utc::now())
        && row.result.is_none()
        && row.completed_at.is_none()
}

fn retain_task_observation(path: &Path, row: &veoveo_platform_store::TaskRecord) -> Result<()> {
    // Capture input contains the deterministic local scene, never a caller token.
    // Keep only the lifecycle fields required for diagnosing this owned Task.
    fs::write(
        path,
        serde_json::to_vec_pretty(&json!({
            "task": row.id, "status": row.status,
            "leaseOwner": row.lease_owner, "leaseExpiresAt": row.lease_expires_at,
            "request": row.request.input, "requestSha256": hex::encode(Sha256::digest(
                serde_json::to_vec(&row.request.input)?)),
            "completedAt": row.completed_at,
        }))?,
    )?;
    Ok(())
}

async fn qualify_view_lifecycle(
    image: &str,
    catalog: &Path,
    fixtures: &Path,
    platform: &PlatformStoreSmoke,
    mut original: RunningView,
    view: &Value,
    composition: &Value,
    evidence: &Path,
) -> Result<()> {
    let store = veoveo_platform_store::PlatformStore::connect(
        veoveo_platform_store::StoreConfig::builder(
            &platform.endpoint,
            &platform.namespace,
            &platform.database,
            veoveo_platform_store::StoreCredentials::database(
                SURREAL_RUNTIME_USER,
                SURREAL_RUNTIME_PASSWORD,
            ),
        )
        .build()?,
    )
    .await?;
    let token_a = issue_view_token("view-smoke-a")?;
    let token_b = issue_view_token("view-smoke-b")?;
    let endpoint = format!("{}/view/mcp", original.base);
    let owner = FinalTaskSmokeClient::new(&endpoint, token_a.clone())
        .connect()
        .await?;
    let caller = FinalTaskSmokeClient::new(&endpoint, token_b.clone())
        .connect()
        .await?;
    let mut request = capture_request(
        json_string(view, "/viewId")?,
        view["revision"].as_u64().context("view omitted revision")?,
        false,
    );
    request["policy"]["encoding"] = json!("jpeg");
    let mut replacement = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(360);
    let result: Result<()> = tokio::time::timeout_at(deadline, async {
        let created = futures::future::join_all((0..8).map(|_| {
            call_tool_as_task(&owner, "capture_frame", request.clone())
        })).await.into_iter().collect::<Result<Vec<_>>>()?;
        let tasks = created.iter().map(|task| veoveo_types::TaskId::parse(&task.task_id))
            .collect::<Result<Vec<_>, _>>()?;
        let mut candidate = None;
        for &task in &tasks {
            let mut selected = store.client().query(
                "SELECT * FROM $task WHERE status IN ['queued', 'running', 'waiting'] AND lease_owner != NONE AND lease_expires_at > time::now()"
            ).bind(("task", veoveo_platform_store::task_record_id(task))).await?.check()?;
            let rows: Vec<veoveo_platform_store::TaskRecord> = selected.take(0)?;
            if let Some(row) = rows.into_iter().next() {
                retain_task_observation(&evidence.join(format!("task-{task}-before.json")), &row)?;
                if unfinished_claim(&row) { candidate = Some((task, row)); break; }
            }
        }
        let (task, observed) = candidate.context("eight production captures left no claimed unfinished Task; restart qualification did not run")?;
        let cid = view_container_id(&original)?;
        lifecycle_docker(["kill".into(), "--signal=STOP".into(), cid.clone().into()], deadline).await?;
        let retained = lifecycle_task_row(&store, task).await?;
        retain_task_observation(&evidence.join("restart-retained-task.json"), &retained)?;
        ensure!(unfinished_claim(&retained), "observed capture finished before the process was frozen; restart qualification did not run");
        ensure!(retained.request.input == observed.request.input
            && retained.lease_owner == observed.lease_owner
            && retained.lease_expires_at == observed.lease_expires_at,
            "frozen capture did not preserve its observed immutable input and lease");
        let expiry = retained.lease_expires_at.context("claimed capture omitted expiry")?;
        ensure!((expiry - Utc::now()).num_seconds() > 30,
            "production lease has insufficient time for replacement startup");
        fs::write(evidence.join("view-interrupted.log"), lifecycle_logs(&original, deadline).await?
            .replace(&token_a, "[REDACTED]").replace(&token_b, "[REDACTED]")
            .replace(SURREAL_RUNTIME_PASSWORD, "[REDACTED]"))?;
        lifecycle_docker(["kill".into(), "--signal=KILL".into(), cid.clone().into()], deadline).await?;
        let interruption_end = deadline.min(tokio::time::Instant::now() + Duration::from_secs(5));
        let interrupted: Value = tokio::time::timeout_at(interruption_end, async {
            loop {
                let state: Value = serde_json::from_str(&lifecycle_docker(
                    ["inspect".into(), "--format={{json .State}}".into(), cid.clone().into()], interruption_end).await?)?;
                if state["Running"] == false { return Ok::<Value, anyhow::Error>(state); }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }).await.context("owned interrupted View did not exit within five seconds")??;
        ensure!(interrupted["Running"] == false && interrupted["ExitCode"] == 137,
            "intentional owned-process interruption did not settle: {interrupted}");
        let next = start_view_container(image, catalog, Some(fixtures), platform, false, Some(original.port), Some(deadline)).await?;
        ensure!(next.adapter["cudaDeviceUuid"] == original.adapter["cudaDeviceUuid"]
            && next.adapter["jpegEncoder"] == original.adapter["jpegEncoder"],
            "replacement changed the admitted GPU UUID or JPEG encoder");
        replacement = Some(next);
        ensure!(Utc::now() < expiry, "replacement became ready after retained lease expired");
        // Reconnect both callers through the maintained client. The endpoint stays the
        // same so existing remote Task cleanup registrations can reconcile this process.
        let recovered_owner = FinalTaskSmokeClient::new(&endpoint, token_a.clone()).connect().await?;
        let recovered_caller = FinalTaskSmokeClient::new(&endpoint, token_b.clone()).connect().await?;
        let mut early_observations = 0;
        while Utc::now() < expiry {
            let row = lifecycle_task_row(&store, task).await?;
            retain_task_observation(&evidence.join("restart-current-task.json"), &row)?;
            if Utc::now() < expiry {
                ensure!(row.request.input == retained.request.input
                    && row.lease_owner == retained.lease_owner && row.lease_expires_at == Some(expiry)
                    && row.result.is_none() && row.completed_at.is_none() && row.status == retained.status,
                    "replacement changed or resumed capture before its retained lease expired");
                let status = recovered_owner.get_task(rmcp::model::GetTaskParams::new(task.to_string())).await?;
                if Utc::now() < expiry {
                    ensure!(status.task.status() == rmcp::model::TaskStatus::Working,
                        "capture completed before retained lease expiry");
                    early_observations += 1;
                }
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        ensure!(early_observations > 0, "replacement supplied no observation under the live lease");
        require_isolation_rejection(
            recovered_caller.get_task(rmcp::model::GetTaskParams::new(task.to_string())).await,
            "unknown task id",
        )?;
        await_task_terminal_with_timeout(&recovered_owner, &task.to_string(), Duration::from_secs(60)).await?;
        let payload = task_payload(&recovered_owner, &task.to_string()).await?;
        let record = payload.structured_content.as_ref().context("recovered capture omitted metadata")?;
        let bytes = image_bytes(&payload, "image/jpeg")?;
        assert_local_frame(record, &bytes, "image/jpeg")?;
        ensure!(record["compositionId"] == composition["compositionId"]
            && record["compositionDigestSha256"] == composition["compositionDigestSha256"]
            && record["viewRevision"] == view["revision"]
            && record["renderedOverlayCount"] == 4
            && record["outputDigestSha256"] == hex::encode(Sha256::digest(&bytes)),
            "recovered capture lost immutable provenance or output bytes");
        let uri = json_string(record, "/frameUri")?;
        ensure!(read_blob_resource(&recovered_owner, uri, "image/jpeg").await? == bytes,
            "recovered frame resource differs from completed Task image");
        require_isolation_rejection(
            recovered_caller.read_resource(ReadResourceRequestParams::new(uri)).await,
            "unknown View resource",
        )?;
        retain_task_observation(&evidence.join("restart-completed-task.json"), &lifecycle_task_row(&store, task).await?)?;
        // Settle every admitted Task through its original registration before stopping
        // the replacement. These reads reconnect to the same owned endpoint.
        for created in &created {
            await_task_terminal_with_timeout(&owner, &created.task_id, Duration::from_secs(60)).await?;
        }
        recovered_owner.cancel().await?;
        recovered_caller.cancel().await?;
        owner.cancel().await?;
        caller.cancel().await?;
        let next = replacement.as_mut().context("missing replacement")?;
        assert_encoder_completion_logs(next, 1, &lifecycle_logs(next, deadline).await?)?;
        let shutdown_start = std::time::Instant::now();
        lifecycle_docker(["stop".into(), "--time=30".into(), view_container_id(next)?.into()], deadline).await?;
        let elapsed = shutdown_start.elapsed();
        let state: Value = serde_json::from_str(&lifecycle_docker(
            ["inspect".into(), "--format={{json .State}}".into(), view_container_id(next)?.into()], deadline).await?)?;
        fs::write(evidence.join("view-lifecycle.json"), serde_json::to_vec_pretty(&json!({
            "qualification": "local-owned-hardware-fixture", "task": task,
            "retainedLeaseExpiresAt": expiry, "earlyObservations": early_observations,
            "adapter": next.adapter, "interruptedState": interrupted,
            "shutdownSeconds": elapsed.as_secs_f64(), "shutdownState": state,
            "gatewayOAuthAcceptance": "not_run",
        }))?)?;
        ensure!(elapsed < Duration::from_secs(30) && state["Running"] == false && state["ExitCode"] == 0,
            "replacement did not exit normally within 30-second grace: {elapsed:?}, {state}");
        Ok(())
    }).await.unwrap_or_else(|_| Err(anyhow!("View hardware lifecycle qualification exceeded 360 seconds")));
    // Keep redacted logs on failure; TmpDirGuard preserves this fixture directory.
    for (name, running) in [
        ("view-original-final.log", Some(&original)),
        ("view-replacement-final.log", replacement.as_ref()),
    ] {
        if let Some(running) = running {
            if let Ok(logs) = lifecycle_logs(
                running,
                tokio::time::Instant::now() + Duration::from_secs(5),
            )
            .await
            {
                fs::write(
                    evidence.join(name),
                    logs.replace(&token_a, "[REDACTED]")
                        .replace(&token_b, "[REDACTED]")
                        .replace(SURREAL_RUNTIME_PASSWORD, "[REDACTED]"),
                )?;
            }
        }
    }
    // Remove both CID-owned containers even if validation failed while the first
    // process was stopped. Registered owner cleanup reconciles an unsuccessful removal.
    let next_cleanup = match replacement.as_mut() {
        Some(next) => next._container.cleanup().await,
        None => Ok(()),
    };
    let original_cleanup = original._container.cleanup().await;
    let cleanup_result = next_cleanup.and(original_cleanup);
    match (result, cleanup_result) {
        (Err(error), Err(cleanup)) => {
            Err(error.context(format!("owned container cleanup also failed: {cleanup}")))
        }
        (Err(error), Ok(())) => Err(error),
        (Ok(()), cleanup) => cleanup,
    }
}

struct RunningView {
    _container: ContainerGuard,
    base: String,
    adapter: Value,
    port: u16,
}

async fn start_view_container(
    image: &str,
    catalog: &Path,
    fixtures: Option<&Path>,
    platform: &PlatformStoreSmoke,
    google: bool,
    reuse_port: Option<u16>,
    lifecycle_deadline: Option<tokio::time::Instant>,
) -> Result<RunningView> {
    let port = match reuse_port {
        Some(port) => port,
        None => reserve_local_port()?,
    };
    let base = format!("http://127.0.0.1:{port}");
    let container_name = format!("veoveo-view-smoke-{}", uuid::Uuid::new_v4());
    let mut container = ContainerGuard::new(&container_name)?;
    let mut args: Vec<OsString> = vec![
        "run".into(),
        "--cidfile".into(),
        container.cid_file().as_os_str().to_os_string(),
        "-d".into(),
        "--name".into(),
        container_name.clone().into(),
        "--network".into(),
        "host".into(),
        "--gpus".into(),
        "all".into(),
        "--read-only".into(),
        "--tmpfs".into(),
        "/tmp".into(),
        "-e".into(),
        "NVIDIA_VISIBLE_DEVICES=all".into(),
        "-e".into(),
        "NVIDIA_DRIVER_CAPABILITIES=graphics,compute,utility".into(),
        "-e".into(),
        "WGPU_BACKEND=vulkan".into(),
        "-e".into(),
        format!("VEOVEO_SURREAL_ENDPOINT={}", platform.endpoint).into(),
        "-e".into(),
        format!("VEOVEO_SURREAL_NAMESPACE={}", platform.namespace).into(),
        "-e".into(),
        format!("VEOVEO_SURREAL_DATABASE={}", platform.database).into(),
        "-e".into(),
        "VEOVEO_SURREAL_AUTH_LEVEL=database".into(),
        "-e".into(),
        format!("VEOVEO_SURREAL_USERNAME={SURREAL_RUNTIME_USER}").into(),
        "-e".into(),
        format!("VEOVEO_SURREAL_PASSWORD={SURREAL_RUNTIME_PASSWORD}").into(),
        "-e".into(),
        format!("VEOVEO_INTERNAL_TRUST_JWKS={INTERNAL_TRUST_JWKS}").into(),
        "-v".into(),
        format!("{}:/etc/veoveo/view/layers.json:ro", catalog.display()).into(),
    ];
    if let Some(fixtures) = fixtures {
        args.extend([
            "-v".into(),
            format!("{}:/fixtures:ro", fs::canonicalize(fixtures)?.display()).into(),
        ]);
    }
    if google {
        args.extend(["-e".into(), "GOOGLE_MAPS_API_KEY".into()]);
    }
    args.extend([
        image.into(),
        "--port".into(),
        port.to_string().into(),
        "--public-base-url".into(),
        base.clone().into(),
        "--allow-loopback-hosts".into(),
        "--layer-catalog".into(),
        "/etc/veoveo/view/layers.json".into(),
        "--max-deadline-ms".into(),
        "180000".into(),
        "--max-captures-in-flight".into(),
        "1".into(),
    ]);
    let created = match lifecycle_deadline {
        Some(deadline) => lifecycle_docker(args, deadline).await?,
        None => run_checked(Path::new("docker"), args, [])?,
    };
    container.record_created(&created)?;
    if let Err(error) = wait_for_http(&format!("{base}/view/readyz")).await {
        let cid: OsString = fs::read_to_string(container.cid_file())?.trim().into();
        let logs = match lifecycle_deadline {
            Some(deadline) => {
                lifecycle_docker_logs(
                    cid,
                    deadline.min(tokio::time::Instant::now() + Duration::from_secs(5)),
                )
                .await
            }
            None => run_checked(Path::new("docker"), ["logs".into(), cid], []),
        }
        .unwrap_or_else(|_| "could not read owned View startup logs".to_owned());
        if let Some(fixtures) = fixtures {
            if let Some(directory) = fixtures.parent() {
                fs::write(
                    directory.join("view-startup-failure.log"),
                    logs.replace(SURREAL_RUNTIME_PASSWORD, "[REDACTED]"),
                )?;
            }
        }
        bail!("View container did not become ready: {error}\n{logs}");
    }
    let adapter: Value = reqwest::get(format!("{base}/view/readyz"))
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure!(
        adapter["hardwareAccelerated"] == true
            && adapter["nvidia"] == true
            && adapter["backend"] == "Vulkan"
            && adapter["jpegEncoder"] == "nvjpeg_cuda_gpu"
            && adapter["cudaDeviceUuid"]
                .as_str()
                .is_some_and(|uuid| uuid.len() == 32),
        "View container did not select NVIDIA Vulkan: {adapter}"
    );
    Ok(RunningView {
        _container: container,
        base,
        adapter,
        port,
    })
}

fn inspect_view_image(image: &str) -> Result<()> {
    run_checked(
        Path::new("docker"),
        ["image".into(), "inspect".into(), image.into()],
        [],
    )?;
    let binaries = run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "--entrypoint".into(),
            "/usr/bin/find".into(),
            image.into(),
            "/usr/local/bin".into(),
            "-maxdepth".into(),
            "1".into(),
            "-type".into(),
            "f".into(),
            "-printf".into(),
            "%f\n".into(),
        ],
        [],
    )?;
    ensure!(
        binaries.lines().any(|binary| binary == "view-mcp")
            && !binaries
                .lines()
                .any(|binary| matches!(binary, "view-gpu-smoke" | "view-google-proof")),
        "production View image had the wrong binary surface: {binaries}"
    );
    Ok(())
}

async fn call_structured(session: &SmokeMcpClient, name: &str, arguments: Value) -> Result<Value> {
    let arguments = arguments
        .as_object()
        .cloned()
        .context("tool arguments were not an object")?;
    let result = session
        .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(arguments))
        .await?;
    ensure!(
        result.is_error != Some(true),
        "View tool `{name}` failed: {:?}",
        result.content
    );
    result
        .structured_content
        .context("View tool returned no structured content")
}

async fn read_blob_resource(
    session: &SmokeMcpClient,
    uri: &str,
    expected_mime: &str,
) -> Result<Vec<u8>> {
    let result = session
        .read_resource(ReadResourceRequestParams::new(uri))
        .await?;
    let (blob, mime_type) = result
        .contents
        .iter()
        .find_map(|content| match content {
            ResourceContents::BlobResourceContents {
                blob, mime_type, ..
            } => Some((blob, mime_type)),
            _ => None,
        })
        .context("frame resource returned no blob")?;
    ensure!(mime_type.as_deref() == Some(expected_mime));
    Ok(STANDARD.decode(blob)?)
}

async fn assert_preview_app_resource(session: &SmokeMcpClient) -> Result<()> {
    let result = session
        .read_resource(ReadResourceRequestParams::new("ui://view/preview.html"))
        .await?;
    let (text, mime_type) = result
        .contents
        .iter()
        .find_map(|content| match content {
            ResourceContents::TextResourceContents {
                text, mime_type, ..
            } => Some((text, mime_type)),
            _ => None,
        })
        .context("preview app resource returned no text")?;
    ensure!(
        mime_type.as_deref() == Some("text/html;profile=mcp-app"),
        "preview app has the wrong mime type: {mime_type:?}"
    );
    ensure!(
        text.len() < 2 * 1024 * 1024,
        "preview app exceeds the console host's 2 MiB cap"
    );
    for needle in [
        "DracoDecoderModule",
        "ui/initialize",
        "tools/call",
        "app.composition = record",
        "composition ready",
    ] {
        ensure!(text.contains(needle), "preview app is missing `{needle}`");
    }
    Ok(())
}

fn image_bytes(payload: &rmcp::model::CallToolResult, expected_mime: &str) -> Result<Vec<u8>> {
    let image = payload
        .content
        .iter()
        .find_map(|content| match content {
            ContentBlock::Image(image) => Some(image),
            _ => None,
        })
        .context("capture task returned no MCP image content")?;
    ensure!(image.mime_type == expected_mime);
    Ok(STANDARD.decode(&image.data)?)
}

fn issue_view_token(subject: &str) -> Result<String> {
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        GatewayInternalSigningKey::new(
            "veoveo-internal-1",
            STANDARD.decode(INTERNAL_SIGNING_KEY_DER_B64)?,
        )?,
    );
    let principal_issuer = TokenIssuer::parse("https://smoke.veoveo.local")?;
    let principal_subject = TokenSubject::parse(subject)?;
    let principal = Principal {
        id: PrincipalId::parse(format!("{principal_issuer}#{principal_subject}"))?,
        kind: PrincipalKind::Service,
        issuer: principal_issuer,
        subject: principal_subject,
        tenant: Some(TenantId::parse("local")?),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: ["operator:use", "view:read", "view:write", "view:capture"]
            .into_iter()
            .map(ScopeName::parse)
            .collect::<Result<_, _>>()?,
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(Utc::now()),
    };
    let authority = InvocationAuthority {
        work_context: WorkContextId::parse("smoke")?,
        tenant: TenantId::parse("local")?,
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: PolicyVersion::parse("r1")?,
        output_policy: WorkContextOutputPolicy {
            owner: AccessSubject::Principal(principal.id.clone()),
            initial_grants: Vec::new(),
            classification: None,
            data_labels: BTreeSet::new(),
        },
        provenance: InvocationProvenance::Automated,
    };
    Ok(issuer
        .issue(
            GatewayProfileId::parse("operator")?,
            ServerSlug::parse("view")?,
            principal,
            authority,
            None,
            Utc::now() + TimeDelta::minutes(30),
        )?
        .bearer_token)
}

fn local_camera() -> Value {
    json!({
        "kind": "pose",
        "position": {"latitudeDegrees": 0.0, "longitudeDegrees": 0.0, "ellipsoidalHeightMeters": 0.0},
        "orientation": {"headingDegrees": 0.0, "pitchDegrees": 0.0, "rollDegrees": 0.0},
        "verticalFovDegrees": 60.0
    })
}

fn capture_request(view_id: &str, revision: u64, google: bool) -> Value {
    json!({
        "viewId": view_id,
        "expectedRevision": revision,
        "sceneTime": "2026-07-26T12:00:00Z",
        "policy": {
            "widthPx": if google { 1280 } else { 256 },
            "heightPx": if google { 720 } else { 256 },
            "maxScreenErrorPx": if google { 16.0 } else { 8.0 },
            "deadlineMs": if google { 180_000 } else { 5_000 },
            "deadlineBehavior": if google { "return_best_available" } else { "fail" },
            "encoding": if google { "jpeg" } else { "png" }
        }
    })
}

async fn create_composition(
    session: &SmokeMcpClient,
    base_layer: &str,
    with_overlays: bool,
) -> Result<Value> {
    let governed_inputs = if with_overlays {
        vec![json!({
                "inputId": "smoke-route",
                "resourceUri": veoveo_map_mcp::contract::MapRouteUri::new(veoveo_map_mcp::contract::RouteId::from_stable_key(b"view-smoke-route")),
                "digestSha256": "0".repeat(64),
                "license": "CC0-1.0",
                "attribution": "Veoveo governed overlay smoke fixture"
        })]
    } else {
        Vec::new()
    };
    let position = |latitude_degrees: f64, longitude_degrees: f64| {
        json!({
            "kind": "wgs84",
            "position": {
                "latitudeDegrees": latitude_degrees,
                "longitudeDegrees": longitude_degrees,
                "ellipsoidalHeightMeters": 1.0
            }
        })
    };
    let overlays = if with_overlays {
        vec![
            json!({
                "overlayId": "marker",
                "governedInputIds": ["smoke-route"],
                "geometry": {
                    "kind": "inline",
                    "geometry": {"kind": "marker", "position": position(0.00004, 0.0)}
                }
            }),
            json!({
                "overlayId": "line",
                "governedInputIds": ["smoke-route"],
                "geometry": {
                    "kind": "inline",
                    "geometry": {
                        "kind": "polyline",
                        "positions": [position(0.00003, -0.00001), position(0.00006, 0.00001)]
                    }
                }
            }),
            json!({
                "overlayId": "area",
                "governedInputIds": ["smoke-route"],
                "geometry": {
                    "kind": "inline",
                    "geometry": {
                        "kind": "polygon",
                        "positions": [
                            position(0.00005, -0.00001),
                            position(0.00007, 0.0),
                            position(0.00005, 0.00001)
                        ],
                        "triangleIndices": [0, 1, 2]
                    }
                }
            }),
            json!({
                "overlayId": "label",
                "governedInputIds": ["smoke-route"],
                "geometry": {
                    "kind": "inline",
                    "geometry": {
                        "kind": "label",
                        "position": position(0.00008, 0.0),
                        "text": "PLAN 1"
                    }
                }
            }),
        ]
    } else {
        Vec::new()
    };
    call_structured(
        session,
        "create_scene_composition",
        json!({
            "schemaVersion": 2,
            "baseLayer": base_layer,
            "mapReleases": if with_overlays {
                vec![veoveo_map_mcp::contract::MapReleaseUri::new(veoveo_map_mcp::contract::MapDatasetId::from_stable_key(b"view-smoke-dataset"), veoveo_map_mcp::contract::DatasetReleaseId::from_stable_key(b"view-smoke-release"))]
            } else {
                Vec::new()
            },
            "styleId": "smoke:1",
            "governedInputs": governed_inputs,
            "overlays": overlays
        }),
    )
    .await
}

fn assert_local_frame(record: &Value, bytes: &[u8], mime: &str) -> Result<()> {
    ensure!(record["mimeType"] == mime);
    match mime {
        "image/png" => ensure!(bytes.starts_with(b"\x89PNG\r\n\x1a\n")),
        "image/jpeg" => ensure!(bytes.starts_with(&[0xff, 0xd8, 0xff])),
        _ => bail!("unexpected local frame MIME"),
    }
    let decoded = image::load_from_memory(bytes)?;
    ensure!(decoded.width() == 256 && decoded.height() == 256);
    ensure!(record["widthPx"] == 256 && record["heightPx"] == 256);
    ensure!(record["visibleTileCount"] == 1 && record["pendingTileCount"] == 0);
    ensure!(record["detailComplete"] == true);
    ensure!(
        record["attribution"]["lines"]
            .as_array()
            .is_some_and(|lines| lines.iter().any(|line| line == "Veoveo GPU smoke fixture"))
    );
    ensure!(materially_different_pixels(bytes)? > 256);
    Ok(())
}

fn materially_different_pixels(bytes: &[u8]) -> Result<usize> {
    let pixels = image::load_from_memory(bytes)?.to_rgb8();
    let reference = pixels.get_pixel(0, 0).0;
    Ok(pixels
        .pixels()
        .filter(|pixel| {
            pixel
                .0
                .iter()
                .enumerate()
                .any(|(index, channel)| channel.abs_diff(reference[index]) > 24)
        })
        .count())
}

fn write_retained_frame(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes).with_context(|| format!("write retained frame {}", path.display()))
}

fn json_string<'a>(value: &'a Value, pointer: &str) -> Result<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .with_context(|| format!("JSON pointer `{pointer}` was not a string: {value}"))
}

fn write_local_fixture(directory: &Path) -> Result<PathBuf> {
    fs::create_dir_all(directory)?;
    fs::write(directory.join("triangle.glb"), triangle_glb()?)?;
    let local_from_ecef = DMat4::from_cols(
        DVec4::new(0.0, 1.0, 0.0, 0.0),
        DVec4::new(1.0, 0.0, 0.0, 0.0),
        DVec4::new(0.0, 0.0, -1.0, 0.0),
        DVec4::new(0.0, -6_378_137.0, 0.0, 1.0),
    );
    let y_up_to_z_up = DMat4::from_cols(
        DVec4::new(1.0, 0.0, 0.0, 0.0),
        DVec4::new(0.0, 0.0, 1.0, 0.0),
        DVec4::new(0.0, -1.0, 0.0, 0.0),
        DVec4::new(0.0, 0.0, 0.0, 1.0),
    );
    let transform = local_from_ecef.inverse()
        * DMat4::from_translation(DVec3::new(0.0, 0.0, -5.0))
        * y_up_to_z_up.inverse();
    fs::write(
        directory.join("tileset.json"),
        serde_json::to_vec(&json!({
            "asset": {"version": "1.1"}, "geometricError": 0.0,
            "root": {
                "boundingVolume": {"sphere": [0.0, 0.0, 0.0, 2.0]}, "geometricError": 0.0,
                "refine": "REPLACE", "transform": transform.to_cols_array(),
                "content": {"uri": "triangle.glb"}
            }
        }))?,
    )?;
    let catalog = directory.join("layers.json");
    fs::write(
        &catalog,
        serde_json::to_vec(&json!({
            "layers": [{
                "layerId": LOCAL_LAYER, "label": "deterministic local GPU smoke",
                "source": {"kind": "local_tileset", "rootPath": "/fixtures/tileset.json"}
            }]
        }))?,
    )?;
    Ok(fs::canonicalize(catalog)?)
}

fn triangle_glb() -> Result<Vec<u8>> {
    let mut binary = Vec::new();
    for value in [-1.0_f32, -1.0, 0.0, 1.0, -1.0, 0.0, 0.0, 1.0, 0.0] {
        binary.extend_from_slice(&value.to_le_bytes());
    }
    for index in [0_u16, 1, 2] {
        binary.extend_from_slice(&index.to_le_bytes());
    }
    while binary.len() % 4 != 0 {
        binary.push(0);
    }
    let document = json!({
        "asset": {"version": "2.0", "generator": "veoveo-smoke", "copyright": "Veoveo GPU smoke fixture"},
        "extensionsUsed": ["KHR_materials_unlit"], "buffers": [{"byteLength": binary.len()}],
        "bufferViews": [
            {"buffer": 0, "byteOffset": 0, "byteLength": 36, "target": 34962},
            {"buffer": 0, "byteOffset": 36, "byteLength": 6, "target": 34963}
        ],
        "accessors": [
            {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [-1.0,-1.0,0.0], "max": [1.0,1.0,0.0]},
            {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR", "min": [0], "max": [2]}
        ],
        "materials": [{
            "pbrMetallicRoughness": {"baseColorFactor": [0.8,0.1,0.05,1.0], "metallicFactor": 0.0, "roughnessFactor": 1.0},
            "doubleSided": true, "extensions": {"KHR_materials_unlit": {}}
        }],
        "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1, "material": 0, "mode": 4}]}],
        "nodes": [{"mesh": 0}], "scenes": [{"nodes": [0]}], "scene": 0
    });
    let mut json_bytes = serde_json::to_vec(&document)?;
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    let total_length = 12 + 8 + json_bytes.len() + 8 + binary.len();
    let mut glb = Vec::with_capacity(total_length);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2_u32.to_le_bytes());
    glb.extend_from_slice(&(u32::try_from(total_length)?).to_le_bytes());
    glb.extend_from_slice(&(u32::try_from(json_bytes.len())?).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json_bytes);
    glb.extend_from_slice(&(u32::try_from(binary.len())?).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&binary);
    Ok(glb)
}
