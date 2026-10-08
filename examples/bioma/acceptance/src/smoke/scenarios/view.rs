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
#[path = "view/installed.rs"]
mod installed;
#[path = "view/lifecycle.rs"]
mod lifecycle;
#[path = "view/readiness.rs"]
mod readiness;
#[path = "view/schema.rs"]
mod schema;
use readiness::AdmittedAdapter;
use veoveo_view_mcp::contract::{
    CameraDefinition, CaptureFrameRequest, CapturePolicy, CapturedFrame,
    CreateSceneCompositionRequest, CreateViewRequest, DeadlineBehavior, FrameEncoding, FrameRecord,
    GeodeticCameraPose, GovernedResourceUri, GovernedSceneInput, HeadingPitchRoll, LayerId,
    OrbitTargetCamera, PreviewScenePolicy, PreviewSceneRecord, SceneComposition,
    SceneCompositionId, SceneInputId, SceneOverlay, SceneOverlayGeometry,
    SceneOverlayGeometrySource, SceneOverlayId, ScenePosition, SceneStyleId, SetCameraRequest,
    ViewId, ViewRecord, ViewSceneUri, ViewUri, Wgs84Position3d,
};

const LOCAL_LAYER: &str = "gpu-smoke";

const GOOGLE_LAYER: &str = "google-photorealistic";

const STATUE_LATITUDE: f64 = 40.689_249_4;

const STATUE_LONGITUDE: f64 = -74.044_500_4;

const STATUE_HEIGHT_METERS: f64 = 20.0;

pub(crate) async fn view_installed(
    installation: &Path,
    fixture: &Path,
    evidence: &Path,
) -> Result<()> {
    installed::run(installation, fixture, evidence).await
}
pub(crate) fn export_view_fixture(directory: &Path) -> Result<()> {
    write_local_fixture(directory)?;
    Ok(())
}

pub(crate) async fn view_mcp(view_image: &str, retained_frame: Option<&Path>) -> Result<()> {
    inspect_view_image(view_image)?;
    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());
    let fixture_dir = tmpdir.join("fixtures");
    let catalog = write_local_fixture(&fixture_dir)?;
    let platform = spawn_platform_store_smoke().await?;
    let token_a = issue_view_token("view-smoke-a")?;
    let token_b = issue_view_token("view-smoke-b")?;
    let mut running = start_view_container(
        view_image,
        &catalog,
        Some(&fixture_dir),
        &platform,
        false,
        None,
        None,
        &tmpdir,
    )
    .await?;

    let result: Result<()> = async {
    assert_view_http_status(
        &format!("{}/view/mcp", running.base),
        StatusCode::UNAUTHORIZED,
    )
    .await?;
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
        schema::assert_tool_schema(tool)?;
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
        .err().context("an unknown layer identifier unexpectedly succeeded")?;
    let invalid_layer = invalid_layer.to_string();
    ensure!(
        invalid_layer.contains(LOCAL_LAYER) && invalid_layer.contains("view://layers"),
        "unknown-layer error omitted exact recovery guidance: {invalid_layer}"
    );

    let first_composition = create_composition(&session_a, LOCAL_LAYER, true).await?;
    let second_composition = create_composition(&session_b, LOCAL_LAYER, false).await?;
    require_isolation_rejection(session_b.read_resource(ReadResourceRequestParams::new(
        json_string(&first_composition, "/compositionUri")?
    )).await, "unknown View resource")?;
    let first = call_structured(
        &session_a,
        "create_view",
        CreateViewRequest {
            composition_id: SceneCompositionId::parse(json_string(&first_composition, "/compositionId")?)?,
            camera: local_camera(),
        },
    )
    .await?;
    let second_camera = local_camera();
    let second = call_structured(
        &session_b,
        "create_view",
        CreateViewRequest {
            composition_id: SceneCompositionId::parse(json_string(&second_composition, "/compositionId")?)?,
            camera: second_camera,
        },
    )
    .await?;
    let first_id = json_string(&first, "/viewId")?;
    let second_id = json_string(&second, "/viewId")?;
    ensure!(
        first_id != second_id,
        "two owners received the same view id"
    );
    require_isolation_rejection(session_b.read_resource(ReadResourceRequestParams::new(
        ViewUri::new(ViewId::parse(first_id)?).to_string()
    )).await, "unknown View resource")?;
    let first = call_structured(
        &session_a,
        "set_camera",
        SetCameraRequest { view_id: ViewId::parse(first_id)?, expected_revision: 1, camera: local_camera() },
    )
    .await?;
    ensure!(
        first["revision"] == 2,
        "camera revision did not advance: {first}"
    );
    let second_resource =
        read_view_resource(&session_b, &ViewUri::new(ViewId::parse(second_id)?).to_string()).await?;
    ensure!(second_resource["revision"] == 1);

    let scene_uri = ViewSceneUri::new(ViewId::parse(first_id)?, PreviewScenePolicy { width_px: 256, height_px: 256, max_screen_error_px: 8.0 })?.to_string();
    let scene = read_view_resource(&session_a, &scene_uri).await?;
    let _: PreviewSceneRecord = serde_json::from_value(scene.clone()).context("scene failed View owner admission")?;
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
    require_isolation_rejection(session_b.read_resource(ReadResourceRequestParams::new(&scene_uri)).await,
        "unknown View resource")?;

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
            let mut request = capture_request(view_id, revision, false)?;
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
                        == expected_composition["compositionDigestSha256"]
                    && record["viewId"] == view["viewId"]
                    && record["viewRevision"] == view["revision"],
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
    assert_encoder_completions(&running, 2).await?;
    session_a.cancel().await?;
    session_b.cancel().await?;
    qualify_view_lifecycle(
        view_image,
        &catalog,
        &fixture_dir,
        &platform,
        &mut running,
        &first,
        &first_composition,
        &tmpdir,
    )
    .await?;
    Ok(())
    }.await;
    if let Err(error) = result {
        return Err(retain_view_failure(
            &running,
            &tmpdir,
            "view-failure",
            error,
            &[&token_a, &token_b],
        )
        .await);
    }
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
    let token = issue_view_token("view-google-live")?;
    let running = start_view_container(
        view_image, &catalog, None, &platform, true, None, None, &tmpdir,
    )
    .await?;
    let result: Result<()> = async {
        let session = connect_mcp_client(&format!("{}/view/mcp", running.base), &token).await?;
        let composition = create_composition(&session, GOOGLE_LAYER, false).await?;
        let view = call_structured(
            &session,
            "create_view",
            CreateViewRequest {
                composition_id: SceneCompositionId::parse(json_string(
                    &composition,
                    "/compositionId",
                )?)?,
                camera: CameraDefinition::OrbitTarget(OrbitTargetCamera {
                    target: Wgs84Position3d {
                        latitude_degrees: STATUE_LATITUDE,
                        longitude_degrees: STATUE_LONGITUDE,
                        ellipsoidal_height_meters: STATUE_HEIGHT_METERS,
                    },
                    distance_meters: 650.0,
                    azimuth_degrees: 210.0,
                    elevation_degrees: 40.0,
                    vertical_fov_degrees: 45.0,
                }),
            },
        )
        .await?;
        let payload =
            FinalTaskSmokeClient::new(&format!("{}/view/mcp", running.base), token.clone())
                .run_tool(
                    "capture_frame",
                    capture_request(
                        json_string(&view, "/viewId")?,
                        view["revision"].as_u64().context("view omitted revision")?,
                        true,
                    )?,
                    Duration::from_secs(300),
                )
                .await?;
        let record = payload
            .structured_content
            .as_ref()
            .context("Google capture omitted frame metadata")?;
        let bytes = image_bytes(&payload, "image/jpeg")?;
        admit_captured_frame(record, &bytes, "image/jpeg")?;
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
                "adapter": running.adapter.name,
                "backend": running.adapter.backend,
                "deviceType": running.adapter.device_type,
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
        Ok(())
    }
    .await;
    if let Err(error) = result {
        return Err(retain_view_failure(
            &running,
            &tmpdir,
            "view-google-failure",
            error,
            &[&token],
        )
        .await);
    }
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

async fn assert_encoder_completions(running: &RunningView, minimum: usize) -> Result<()> {
    assert_encoder_completion_logs(
        running,
        minimum,
        &lifecycle_logs(
            running,
            tokio::time::Instant::now() + Duration::from_secs(5),
        )
        .await?,
    )
}

fn assert_encoder_completion_logs(running: &RunningView, minimum: usize, logs: &str) -> Result<()> {
    readiness::assert_capture_completions(&running.adapter, minimum, logs)
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct HttpObservation {
    request: String,
    status: u16,
    body_excerpt: String,
}

async fn observe_http(url: &str) -> Result<HttpObservation> {
    use futures::StreamExt as _;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("GET {url}: no HTTP response"))?;
    let status = response.status().as_u16();
    let mut body = Vec::new();
    let mut chunks = response.bytes_stream();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk
            .with_context(|| format!("GET {url}: status {status}, incomplete response body"))?;
        let remaining = 4096 - body.len();
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if body.len() == 4096 {
            break;
        }
    }
    Ok(HttpObservation {
        request: format!("GET {url}"),
        status,
        body_excerpt: bounded_redacted(&String::from_utf8_lossy(&body), &[]),
    })
}

async fn assert_view_http_status(url: &str, expected: StatusCode) -> Result<()> {
    let response = observe_http(url).await?;
    ensure!(
        response.status == expected.as_u16(),
        "{}: expected {expected}, got {}; body excerpt: {}",
        response.request,
        response.status,
        response.body_excerpt
    );
    Ok(())
}

fn bounded_redacted(text: &str, secrets: &[&str]) -> String {
    // Redact before truncation, including the optional provider credential that
    // can appear only in the explicitly selected Google scenario.
    let mut redacted = text
        .replace(SURREAL_RUNTIME_PASSWORD, "[REDACTED]")
        .replace(INTERNAL_SIGNING_KEY_DER_B64, "[REDACTED]");
    for secret in secrets.iter().copied().filter(|secret| !secret.is_empty()) {
        redacted = redacted.replace(secret, "[REDACTED]");
    }
    if let Ok(secret) = std::env::var("GOOGLE_MAPS_API_KEY") {
        if !secret.is_empty() {
            redacted = redacted.replace(&secret, "[REDACTED]");
        }
    }
    redacted.chars().take(8192).collect()
}

async fn retain_view_failure(
    running: &RunningView,
    evidence: &Path,
    stem: &str,
    error: anyhow::Error,
    secrets: &[&str],
) -> anyhow::Error {
    let cid = match view_container_id(running) {
        Ok(cid) => cid,
        Err(cid_error) => {
            return error.context(format!(
                "failed to identify owned View diagnostics: {cid_error}"
            ));
        }
    };
    retain_container_failure(cid.into(), &running.base, evidence, stem, error, secrets).await
}

async fn retain_container_failure(
    cid: OsString,
    base: &str,
    evidence: &Path,
    stem: &str,
    error: anyhow::Error,
    secrets: &[&str],
) -> anyhow::Error {
    // This runs while the ContainerGuard is still held. Diagnostic failure cannot
    // replace the original failure or defer owned-container cleanup.
    let logs =
        lifecycle_docker_logs(cid, tokio::time::Instant::now() + Duration::from_secs(5)).await;
    let logs = bounded_redacted(
        &logs.unwrap_or_else(|error| {
            format!("owned startup/log diagnostics unavailable: {error:#}")
        }),
        secrets,
    );
    let ready = observe_http(&format!("{base}/view/readyz")).await;
    let failure = bounded_redacted(&format!("{error:#}"), secrets);
    let diagnostics = json!({
        "failure": failure,
        "readiness": match ready {
            Ok(response) => serde_json::to_value(response).unwrap_or(Value::Null),
            Err(error) => json!({"request": format!("GET {base}/view/readyz"), "status": null,
                "bodyExcerpt": bounded_redacted(&format!("{error:#}"), secrets)}),
        },
        "startupLogExcerpt": logs,
    });
    // A single bounded record goes to both retained evidence and the command error.
    let text = diagnostics.to_string();
    let write = fs::write(evidence.join(format!("{stem}.json")), &text);
    match write {
        Ok(()) => anyhow!("{text}"),
        Err(write_error) => anyhow!("{text}; retaining View diagnostics failed: {write_error}"),
    }
}

async fn lifecycle_logs(running: &RunningView, deadline: tokio::time::Instant) -> Result<String> {
    lifecycle_docker_logs(view_container_id(running)?.into(), deadline).await
}

async fn lifecycle_docker_logs(cid: OsString, deadline: tokio::time::Instant) -> Result<String> {
    let remaining = deadline
        .checked_duration_since(tokio::time::Instant::now())
        .context("View lifecycle logs deadline expired")?;
    let mut command = tokio::process::Command::new("docker");
    command.args(["logs".into(), "--tail=160".into(), cid]);
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
    fn packaged_preview_document_satisfies_public_content_checks() {
        let template = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../servers/view-mcp/assets/preview-app.template.html"
        ));
        let vendor = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../servers/view-mcp/assets/vendor/three-bundle.min.js"
        ));
        let packaged = template.replacen("/*__VEOVEO_THREE_BUNDLE__*/", vendor, 1);
        assert_preview_document(&packaged, Some("text/html;profile=mcp-app")).unwrap();
        assert!(assert_preview_document(&packaged, Some("text/html")).is_err());
        assert!(assert_preview_document("", Some("text/html;profile=mcp-app")).is_err());
    }

    #[test]
    fn local_fixture_requests_admit_through_current_view_contracts() {
        local_camera().validate().unwrap();
        for overlays in [false, true] {
            let composition = composition_request(LOCAL_LAYER, overlays).unwrap();
            composition.validate().unwrap();
            assert_eq!(composition.overlays.len(), if overlays { 4 } else { 0 });
            let create = CreateViewRequest {
                composition_id: SceneCompositionId::parse(
                    "composition-281b253a-55b2-5b69-8f2b-cc214e0be326",
                )
                .unwrap(),
                camera: local_camera(),
            };
            let wire = serde_json::to_value(create).unwrap();
            assert!(wire["camera"].is_object());
            let _: CreateViewRequest = serde_json::from_value(wire).unwrap();
        }
        let request = capture_request("view-1", 2, false).unwrap();
        let request: CaptureFrameRequest = serde_json::from_value(request).unwrap();
        assert_eq!(request.policy.width_px, 256);
        assert_eq!(request.policy.height_px, 256);
        let address = ViewSceneUri::new(
            request.view_id,
            PreviewScenePolicy {
                width_px: request.policy.width_px,
                height_px: request.policy.height_px,
                max_screen_error_px: request.policy.max_screen_error_px,
            },
        )
        .unwrap();
        assert_eq!(ViewSceneUri::parse(address.to_string()).unwrap(), address);
    }

    #[test]
    fn failure_excerpts_redact_before_bounding_and_preserve_context() {
        let token = "fixture-bearer-must-not-survive";
        let input = format!(
            "GET /view/readyz HTTP 503 body not ready\n{} {token} {}",
            SURREAL_RUNTIME_PASSWORD,
            "x".repeat(10_000)
        );
        let excerpt = bounded_redacted(&input, &[token]);
        assert!(excerpt.contains("GET /view/readyz HTTP 503 body not ready"));
        assert!(excerpt.contains("[REDACTED]"));
        assert!(!excerpt.contains(token));
        assert!(!excerpt.contains(SURREAL_RUNTIME_PASSWORD));
        assert_eq!(excerpt.chars().count(), 8192);
    }

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
    original: &mut RunningView,
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
    )?;
    request["policy"]["encoding"] = json!("jpeg");
    let expectation = lifecycle::CaptureExpectation::new(&request, view, composition)?;
    let mut replacement = None;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(360);
    let result: Result<()> = tokio::time::timeout_at(deadline, async {
        let cid = view_container_id(original)?;
        let freeze = lifecycle::OwnedEngineFreeze::prepare(&cid, deadline).await?;
        let created = futures::future::join_all((0..8).map(|_| {
            call_tool_as_task(&owner, "capture_frame", request.clone())
        })).await.into_iter().collect::<Result<Vec<_>>>()?;
        // Freeze before any Store observation or evidence write can consume the capture window.
        freeze.freeze(deadline).await?;
        let tasks = created.iter().map(|task| veoveo_types::TaskId::parse(&task.task_id))
            .collect::<Result<Vec<_>, _>>()?;
        let mut candidate = None;
        for &task in &tasks {
            let mut selected = store.client().query(
                "SELECT * FROM $task WHERE status IN ['queued', 'running', 'waiting'] AND lease_owner != NONE AND lease_expires_at > time::now()"
            ).bind(("task", veoveo_platform_store::task_record_id(task))).await?.check()?;
            let rows: Vec<veoveo_platform_store::TaskRecord> = selected.take(0)?;
            if let Some(row) = rows.into_iter().next() {
                expectation.admit(&row, task)?;
                retain_task_observation(&evidence.join(format!("task-{task}-before.json")), &row)?;
                candidate = Some((task, row)); break;
            }
        }
        let (task, observed) = candidate.context("frozen production captures left no claimed unfinished Task; restart qualification did not run")?;
        let retained = lifecycle_task_row(&store, task).await?;
        retain_task_observation(&evidence.join("restart-retained-task.json"), &retained)?;
        expectation.admit(&retained, task)?;
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
        let next = start_view_container(image, catalog, Some(fixtures), platform, false, Some(original.port), Some(deadline), evidence).await?;
        replacement = Some(next);
        let next = replacement.as_ref().context("replacement was not retained")?;
        ensure!(next.adapter.cuda_device_uuid == original.adapter.cuda_device_uuid
            && next.adapter.jpeg_encoder == original.adapter.jpeg_encoder,
            "replacement changed the admitted GPU UUID or JPEG encoder");
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
    let result = match result {
        Err(error) => {
            let error = retain_view_failure(
                original,
                evidence,
                "view-original-lifecycle-failure",
                error,
                &[&token_a, &token_b],
            )
            .await;
            let error = match replacement.as_ref() {
                Some(next) => {
                    retain_view_failure(
                        next,
                        evidence,
                        "view-replacement-lifecycle-failure",
                        error,
                        &[&token_a, &token_b],
                    )
                    .await
                }
                None => error,
            };
            Err(error)
        }
        Ok(()) => Ok(()),
    };
    // Capture each process before cleanup even on success, since removal itself
    // can fail. TmpDirGuard preserves these observations on any later failure.
    for (name, running) in [
        ("view-original-final.log", Some(&*original)),
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
                    bounded_redacted(&logs, &[&token_a, &token_b]),
                )?;
            }
            let ready = observe_http(&format!("{}/view/readyz", running.base)).await;
            let observation = match ready {
                Ok(ready) => serde_json::to_value(ready)?,
                Err(error) => json!({"request": format!("GET {}/view/readyz", running.base),
                    "status": null, "bodyExcerpt": bounded_redacted(&format!("{error:#}"), &[&token_a, &token_b])}),
            };
            fs::write(
                evidence.join(format!("{name}.http.json")),
                serde_json::to_vec(&observation)?,
            )?;
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
    adapter: AdmittedAdapter,
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
    evidence: &Path,
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
    let startup_deadline =
        lifecycle_deadline.unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(40));
    let startup: Result<AdmittedAdapter> = async {
        let created = lifecycle_docker(args, startup_deadline).await?;
        container.record_created(&created)?;
        tokio::time::timeout_at(
            startup_deadline,
            wait_for_http(&format!("{base}/view/readyz")),
        )
        .await
        .context("View readiness deadline expired")??;
        let observed = observe_http(&format!("{base}/view/readyz")).await?;
        readiness::admit_readiness(observed.status, &observed.body_excerpt)?;
        let logs = lifecycle_docker_logs(
            container_name.clone().into(),
            startup_deadline.min(tokio::time::Instant::now() + Duration::from_secs(5)),
        )
        .await?;
        readiness::admit_startup_logs(&logs)
    }
    .await;
    let adapter = match startup {
        Ok(adapter) => adapter,
        Err(error) => {
            return Err(retain_container_failure(
                container_name.into(),
                &base,
                evidence,
                "view-startup-failure",
                error,
                &[],
            )
            .await);
        }
    };
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

async fn call_structured(
    session: &SmokeMcpClient,
    name: &str,
    arguments: impl serde::Serialize,
) -> Result<Value> {
    let arguments = serde_json::to_value(arguments)?;
    let request = bounded_redacted(&format!("MCP tools/call {name}: {arguments}"), &[]);
    let arguments = arguments
        .as_object()
        .cloned()
        .context("tool arguments were not an object")?;
    let result = session
        .call_tool(CallToolRequestParams::new(name.to_owned()).with_arguments(arguments))
        .await
        .with_context(|| request.clone())?;
    ensure!(
        result.is_error != Some(true),
        "View tool `{name}` failed: {:?}",
        result.content
    );
    let value = result
        .structured_content
        .with_context(|| format!("{request}; tool returned no structured response"))?;
    let response_context = || {
        format!(
            "{request}; MCP response failed owner admission; body excerpt: {}",
            bounded_redacted(&value.to_string(), &[])
        )
    };
    match name {
        "create_view" | "set_camera" => {
            let _: ViewRecord =
                serde_json::from_value(value.clone()).with_context(response_context)?;
        }
        "create_scene_composition" => {
            let _: SceneComposition =
                serde_json::from_value(value.clone()).with_context(response_context)?;
        }
        _ => {}
    }
    Ok(value)
}

async fn read_view_resource(session: &SmokeMcpClient, uri: &str) -> Result<Value> {
    read_mcp_resource_json(session, uri)
        .await
        .with_context(|| format!("MCP resources/read {uri}"))
}

fn admit_captured_frame(record: &Value, bytes: &[u8], mime: &str) -> Result<()> {
    // Owner admission checks C02 resultUri, frameUri and their typed parents;
    // the byte wrapper checks the declared length and SHA-256 together.
    let record: FrameRecord = serde_json::from_value(record.clone()).with_context(|| {
        format!(
            "capture metadata failed View owner admission; body excerpt: {}",
            bounded_redacted(&record.to_string(), &[])
        )
    })?;
    ensure!(
        record.mime_type() == mime,
        "capture MIME does not match the requested format"
    );
    CapturedFrame::from_record(record, bytes.to_vec())
        .context("capture bytes failed View owner admission")?;
    Ok(())
}

async fn read_blob_resource(
    session: &SmokeMcpClient,
    uri: &str,
    expected_mime: &str,
) -> Result<Vec<u8>> {
    let result = session
        .read_resource(ReadResourceRequestParams::new(uri))
        .await
        .with_context(|| format!("MCP resources/read {uri}, expected {expected_mime}"))?;
    let (blob, mime_type, actual_uri) = result
        .contents
        .iter()
        .find_map(|content| match content {
            ResourceContents::BlobResourceContents {
                blob,
                mime_type,
                uri: actual_uri,
                ..
            } => Some((blob, mime_type, actual_uri)),
            _ => None,
        })
        .context("frame resource returned no blob")?;
    ensure!(
        actual_uri == uri && mime_type.as_deref() == Some(expected_mime),
        "MCP resources/read {uri}: response URI or MIME disagrees: {actual_uri}, {mime_type:?}"
    );
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
    assert_preview_document(text, mime_type.as_deref())
}

fn assert_preview_document(text: &str, mime_type: Option<&str>) -> Result<()> {
    ensure!(
        mime_type == Some("text/html;profile=mcp-app"),
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

fn local_camera() -> CameraDefinition {
    CameraDefinition::Pose(GeodeticCameraPose {
        position: Wgs84Position3d {
            latitude_degrees: 0.0,
            longitude_degrees: 0.0,
            ellipsoidal_height_meters: 0.0,
        },
        orientation: HeadingPitchRoll {
            heading_degrees: 0.0,
            pitch_degrees: 0.0,
            roll_degrees: 0.0,
        },
        vertical_fov_degrees: 60.0,
    })
}

fn capture_request(view_id: &str, revision: u64, google: bool) -> Result<Value> {
    Ok(serde_json::to_value(CaptureFrameRequest {
        view_id: ViewId::parse(view_id)?,
        expected_revision: revision,
        scene_time: "2026-07-26T12:00:00Z".parse()?,
        policy: CapturePolicy {
            width_px: if google { 1280 } else { 256 },
            height_px: if google { 720 } else { 256 },
            max_screen_error_px: if google { 16.0 } else { 8.0 },
            deadline_ms: if google { 180_000 } else { 5_000 },
            deadline_behavior: if google {
                DeadlineBehavior::ReturnBestAvailable
            } else {
                DeadlineBehavior::Fail
            },
            encoding: if google {
                FrameEncoding::Jpeg
            } else {
                FrameEncoding::Png
            },
        },
    })?)
}

async fn create_composition(
    session: &SmokeMcpClient,
    base_layer: &str,
    with_overlays: bool,
) -> Result<Value> {
    call_structured(
        session,
        "create_scene_composition",
        composition_request(base_layer, with_overlays)?,
    )
    .await
}

fn composition_request(
    base_layer: &str,
    with_overlays: bool,
) -> Result<CreateSceneCompositionRequest> {
    let input_id = SceneInputId::parse("smoke-route")?;
    let governed_inputs = if with_overlays {
        vec![GovernedSceneInput {
            input_id: input_id.clone(),
            resource_uri: GovernedResourceUri::Route(veoveo_map_mcp::contract::MapRouteUri::new(
                veoveo_map_mcp::contract::RouteId::from_stable_key(b"view-smoke-route"),
            )),
            digest_sha256: veoveo_view_mcp::contract::Sha256Digest::parse("0".repeat(64))?,
            media_type: None,
            license: "CC0-1.0".into(),
            attribution: "Veoveo governed overlay smoke fixture".into(),
        }]
    } else {
        Vec::new()
    };
    let position = |latitude_degrees, longitude_degrees| ScenePosition::Wgs84 {
        position: Wgs84Position3d {
            latitude_degrees,
            longitude_degrees,
            ellipsoidal_height_meters: 1.0,
        },
    };
    let overlays = if with_overlays {
        [
            (
                "marker",
                SceneOverlayGeometry::Marker {
                    position: position(0.00004, 0.0),
                },
            ),
            (
                "line",
                SceneOverlayGeometry::Polyline {
                    positions: vec![position(0.00003, -0.00001), position(0.00006, 0.00001)],
                    closed: false,
                },
            ),
            (
                "area",
                SceneOverlayGeometry::Polygon {
                    positions: vec![
                        position(0.00005, -0.00001),
                        position(0.00007, 0.0),
                        position(0.00005, 0.00001),
                    ],
                    triangle_indices: vec![0, 1, 2],
                },
            ),
            (
                "label",
                SceneOverlayGeometry::Label {
                    position: position(0.00008, 0.0),
                    text: "PLAN 1".into(),
                },
            ),
        ]
        .into_iter()
        .map(|(id, geometry)| {
            Ok(SceneOverlay {
                overlay_id: SceneOverlayId::parse(id)?,
                governed_input_ids: BTreeSet::from([input_id.clone()]),
                geometry: SceneOverlayGeometrySource::Inline { geometry },
                style: Default::default(),
                visibility: Default::default(),
                validity: None,
            })
        })
        .collect::<Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let request = CreateSceneCompositionRequest {
        schema_version: 2,
        base_layer: LayerId::parse(base_layer)?,
        map_releases: if with_overlays {
            BTreeSet::from([veoveo_map_mcp::contract::MapReleaseUri::new(
                veoveo_map_mcp::contract::MapDatasetId::from_stable_key(b"view-smoke-dataset"),
                veoveo_map_mcp::contract::DatasetReleaseId::from_stable_key(b"view-smoke-release"),
            )])
        } else {
            BTreeSet::new()
        },
        local_frame: None,
        style_id: SceneStyleId::parse("smoke:1")?,
        governed_inputs,
        overlays,
    };
    request.validate()?;
    Ok(request)
}

fn assert_local_frame(record: &Value, bytes: &[u8], mime: &str) -> Result<()> {
    admit_captured_frame(record, bytes, mime)?;
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
