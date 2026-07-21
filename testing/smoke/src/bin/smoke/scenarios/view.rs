use std::collections::BTreeSet;

use anyhow::ensure;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{TimeDelta, Utc};
use glam::{DMat4, DVec3, DVec4};
use rmcp::model::{CallToolRequestParams, ContentBlock};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    AccessSubject, GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalSigningKey,
    GatewayInternalTokenIssuer, GatewayProfileId, InvocationAuthority, InvocationProvenance,
    PolicyVersion, Principal, PrincipalId, PrincipalKind, ScopeName, ServerSlug, TenantId,
    TokenIssuer, TokenSubject, WorkContextId, WorkContextMembershipLevel, WorkContextOutputPolicy,
};

use super::*;

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
    let running =
        start_view_container(view_image, &catalog, Some(&fixture_dir), &platform, false).await?;

    assert_http_status(
        &format!("{}/view/mcp", running.base),
        StatusCode::UNAUTHORIZED,
    )
    .await?;
    let token_a = issue_view_token("view-smoke-a")?;
    let token_b = issue_view_token("view-smoke-b")?;
    let session_a = connect_mcp_session(&format!("{}/view/mcp", running.base), &token_a).await?;
    let session_b = connect_mcp_session(&format!("{}/view/mcp", running.base), &token_b).await?;
    let tools = session_a.list_tools(Default::default()).await?;
    for name in ["create_view", "set_camera", "capture_frame", "close_view"] {
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
    }
    assert_preview_app_resource(&session_a).await?;

    let first = call_structured(
        &session_a,
        "create_view",
        json!({"scene_layer": LOCAL_LAYER, "camera": local_camera()}),
    )
    .await?;
    let second = call_structured(
        &session_b,
        "create_view",
        json!({"scene_layer": LOCAL_LAYER, "camera": local_camera()}),
    )
    .await?;
    let first_id = json_string(&first, "/view_id")?;
    let second_id = json_string(&second, "/view_id")?;
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
        json!({"view_id": first_id, "expected_revision": 1, "camera": local_camera()}),
    )
    .await?;
    ensure!(
        first["revision"] == 2,
        "camera revision did not advance: {first}"
    );
    let second_resource =
        read_mcp_resource_json(&session_b, &format!("view://view/{second_id}")).await?;
    ensure!(second_resource["revision"] == 1);

    let scene =
        read_mcp_resource_json(&session_a, &format!("view://view/{first_id}/scene")).await?;
    ensure!(
        scene["view_revision"] == 2,
        "scene revision mismatch: {scene}"
    );
    let scene_tiles = scene["tiles"].as_array().context("scene omitted tiles")?;
    ensure!(!scene_tiles.is_empty(), "scene manifest listed no tiles");
    for tile in scene_tiles {
        let matrix = tile["ecef_from_content"]
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
    let tile_uri = json_string(&scene_tiles[0], "/tile_uri")?;
    let tile_bytes = read_blob_resource(&session_a, tile_uri, "model/gltf-binary").await?;
    ensure!(
        tile_bytes.starts_with(b"glTF"),
        "preview tile blob is not a GLB container"
    );
    ensure!(
        read_mcp_resource_json(&session_b, &format!("view://view/{first_id}/scene"))
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
        let view_id = json_string(view, "/view_id")?;
        let revision = view["revision"].as_u64().context("view omitted revision")?;
        let payload =
            FinalTaskSmokeClient::new(&format!("{}/view/mcp", running.base), token.clone())
                .run_tool(
                    "capture_frame",
                    capture_request(view_id, revision, false),
                    Duration::from_secs(30),
                )
                .await?;
        let record = payload
            .structured_content
            .as_ref()
            .context("capture task omitted frame metadata")?;
        let bytes = image_bytes(&payload, "image/png")?;
        assert_local_frame(record, &bytes)?;
        if index == 0 {
            let resource_bytes =
                read_blob_resource(session, json_string(record, "/frame_uri")?, "image/png")
                    .await?;
            ensure!(resource_bytes == bytes);
            first_bytes = Some(bytes);
        }
    }
    if let Some(output) = retained_frame {
        write_retained_frame(
            output,
            &first_bytes.context("first frame was not retained")?,
        )?;
        println!("retained local frame: {}", output.display());
    }
    session_a.cancel().await?;
    session_b.cancel().await?;
    drop(running);
    cleanup.remove_on_drop();
    println!(
        "View MCP smoke ok: production NVIDIA container, MCP ownership, tasks, and frame resources"
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
    let running = start_view_container(view_image, &catalog, None, &platform, true).await?;
    let token = issue_view_token("view-google-live")?;
    let session = connect_mcp_session(&format!("{}/view/mcp", running.base), &token).await?;
    let view = call_structured(
        &session,
        "create_view",
        json!({
            "scene_layer": GOOGLE_LAYER,
            "camera": {
                "kind": "orbit_target",
                "target": {
                    "latitude_degrees": STATUE_LATITUDE,
                    "longitude_degrees": STATUE_LONGITUDE,
                    "ellipsoidal_height_meters": STATUE_HEIGHT_METERS
                },
                "distance_meters": 650.0,
                "azimuth_degrees": 210.0,
                "elevation_degrees": 40.0,
                "vertical_fov_degrees": 45.0
            }
        }),
    )
    .await?;
    let payload = FinalTaskSmokeClient::new(&format!("{}/view/mcp", running.base), token.clone())
        .run_tool(
            "capture_frame",
            capture_request(
                json_string(&view, "/view_id")?,
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
    ensure!(record["width_px"] == 1280 && record["height_px"] == 720);
    ensure!(record["visible_tile_count"].as_u64().unwrap_or_default() > 0);
    ensure!(record["pending_tile_count"].as_u64().unwrap_or_default() == 0);
    ensure!(materially_different_pixels(&bytes)? > 10_000);
    let resource_bytes =
        read_blob_resource(&session, json_string(record, "/frame_uri")?, "image/jpeg").await?;
    ensure!(resource_bytes == bytes);
    write_retained_frame(output, &bytes)?;
    let digest = Sha256::digest(&bytes);
    println!(
        "{}",
        serde_json::to_string(&json!({
            "adapter": running.adapter["name"],
            "backend": running.adapter["backend"],
            "device_type": running.adapter["device_type"],
            "target": {
                "latitude_degrees": STATUE_LATITUDE,
                "longitude_degrees": STATUE_LONGITUDE,
                "ellipsoidal_height_meters": STATUE_HEIGHT_METERS
            },
            "frame": record,
            "bytes": bytes.len(),
            "sha256": hex::encode(digest),
            "proof_image": output,
        }))?
    );
    session.cancel().await?;
    drop(running);
    cleanup.remove_on_drop();
    Ok(())
}

struct RunningView {
    _container: ContainerGuard,
    base: String,
    adapter: Value,
}

async fn start_view_container(
    image: &str,
    catalog: &Path,
    fixtures: Option<&Path>,
    platform: &PlatformStoreSmoke,
    google: bool,
) -> Result<RunningView> {
    let port = reserve_local_port()?;
    let base = format!("http://127.0.0.1:{port}");
    let container_name = format!("veoveo-view-smoke-{}", uuid::Uuid::new_v4());
    let mut args: Vec<OsString> = vec![
        "run".into(),
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
    ]);
    run_checked(Path::new("docker"), args, [])?;
    let container = ContainerGuard::new(&container_name);
    if let Err(error) = wait_for_http(&format!("{base}/view/readyz")).await {
        let logs = run_checked(
            Path::new("docker"),
            ["logs".into(), container_name.clone().into()],
            [],
        )
        .unwrap_or_else(|log_error| format!("could not read View logs: {log_error}"));
        bail!("View container did not become ready: {error}\n{logs}");
    }
    let adapter: Value = reqwest::get(format!("{base}/view/readyz"))
        .await?
        .error_for_status()?
        .json()
        .await?;
    ensure!(
        adapter["hardware_accelerated"] == true
            && adapter["nvidia"] == true
            && adapter["backend"] == "Vulkan",
        "View container did not select NVIDIA Vulkan: {adapter}"
    );
    Ok(RunningView {
        _container: container,
        base,
        adapter,
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
            "--rm".into(),
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

async fn call_structured(session: &SmokeMcpSession, name: &str, arguments: Value) -> Result<Value> {
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
    session: &SmokeMcpSession,
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

async fn assert_preview_app_resource(session: &SmokeMcpSession) -> Result<()> {
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
    for needle in ["DracoDecoderModule", "ui/initialize", "tools/call"] {
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
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        GatewayInternalSigningKey::new(
            "veoveo-internal-1",
            STANDARD.decode(INTERNAL_SIGNING_KEY_DER_B64)?,
        )?,
    );
    let principal_issuer = TokenIssuer::new("https://smoke.veoveo.local")?;
    let principal_subject = TokenSubject::new(subject)?;
    let principal = Principal {
        id: PrincipalId::new(format!("{principal_issuer}#{principal_subject}"))?,
        kind: PrincipalKind::Service,
        issuer: principal_issuer,
        subject: principal_subject,
        tenant: Some(TenantId::new("local")?),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: ["operator:use", "view:read", "view:write", "view:capture"]
            .into_iter()
            .map(ScopeName::new)
            .collect::<Result<_, _>>()?,
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(Utc::now()),
    };
    let authority = InvocationAuthority {
        work_context: WorkContextId::new("smoke")?,
        tenant: TenantId::new("local")?,
        membership: WorkContextMembershipLevel::Owner,
        policy_revision: PolicyVersion::new("r1")?,
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
            GatewayProfileId::new("operator")?,
            ServerSlug::new("view")?,
            principal,
            authority,
            Utc::now() + TimeDelta::minutes(30),
        )?
        .bearer_token)
}

fn local_camera() -> Value {
    json!({
        "kind": "pose",
        "position": {"latitude_degrees": 0.0, "longitude_degrees": 0.0, "ellipsoidal_height_meters": 0.0},
        "orientation": {"heading_degrees": 0.0, "pitch_degrees": 0.0, "roll_degrees": 0.0},
        "vertical_fov_degrees": 60.0
    })
}

fn capture_request(view_id: &str, revision: u64, google: bool) -> Value {
    json!({
        "view_id": view_id,
        "expected_revision": revision,
        "policy": {
            "width_px": if google { 1280 } else { 256 },
            "height_px": if google { 720 } else { 256 },
            "max_screen_error_px": if google { 16.0 } else { 8.0 },
            "deadline_ms": if google { 180_000 } else { 5_000 },
            "deadline_behavior": if google { "return_best_available" } else { "fail" },
            "encoding": if google { "jpeg" } else { "png" }
        }
    })
}

fn assert_local_frame(record: &Value, bytes: &[u8]) -> Result<()> {
    ensure!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    ensure!(record["width_px"] == 256 && record["height_px"] == 256);
    ensure!(record["visible_tile_count"] == 1 && record["pending_tile_count"] == 0);
    ensure!(record["detail_complete"] == true);
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
                "layer_id": LOCAL_LAYER, "label": "deterministic local GPU smoke",
                "source": {"kind": "local_tileset", "root_path": "/fixtures/tileset.json"}
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
