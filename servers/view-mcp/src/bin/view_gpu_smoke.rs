use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result, ensure};
use glam::{DMat4, DVec3};
use serde_json::json;
use tokio_util::sync::CancellationToken;
use veoveo_view_mcp::{
    contract::{
        CameraDefinition, CaptureFrameRequest, CaptureLimits, CapturePolicy, CreateViewRequest,
        DeadlineBehavior, FrameEncoding, GeodeticCameraPose, HeadingPitchRoll, LayerId,
        SetCameraRequest, Wgs84Position3d,
    },
    geodesy::world_from_ecef,
    renderer::{RendererConfig, RendererHandle},
    source::{LayerCatalog, LayerDefinition, LayerSourceDefinition, SourceConfig},
    state::{ViewService, ViewServiceConfig},
    tiles::traversal::Y_UP_TO_Z_UP,
};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let proof_output = std::env::var_os("VIEW_GPU_SMOKE_OUTPUT").map(PathBuf::from);
    let fixture = LocalFixture::create()?;
    let layer_id = LayerId::new("gpu-smoke")?;
    let catalog = LayerCatalog::from_definitions(
        vec![LayerDefinition {
            layer_id: layer_id.clone(),
            label: "deterministic local GPU smoke".to_owned(),
            source: LayerSourceDefinition::LocalTileset {
                root_path: fixture.tileset.clone(),
            },
        }],
        SourceConfig {
            raw_cache_bytes: 4 * 1024 * 1024,
            max_response_bytes: 4 * 1024 * 1024,
            request_timeout: Duration::from_secs(5),
        },
    )?;
    let renderer = RendererHandle::start(RendererConfig {
        require_nvidia: true,
        gpu_cache_bytes: 64 * 1024 * 1024,
        jpeg_quality: 85,
    })
    .context("initialize NVIDIA Vulkan renderer")?;
    let service = ViewService::new(
        ViewServiceConfig {
            capture_limits: CaptureLimits {
                max_width_px: 512,
                max_height_px: 512,
                max_pixels: 512 * 512,
                max_deadline_ms: 5_000,
            },
            max_views: 4,
            max_views_per_owner: 2,
            max_frames: 4,
            max_frame_bytes: 4 * 1024 * 1024,
            max_single_frame_bytes: 2 * 1024 * 1024,
            decoded_cache_bytes: 4 * 1024 * 1024,
            max_concurrent_loads: 2,
            max_tree_nodes: 64,
            detail_falloff_meters: 2_000.0,
        },
        catalog,
        renderer.clone(),
    );
    let origin = Wgs84Position3d {
        latitude_degrees: 0.0,
        longitude_degrees: 0.0,
        ellipsoidal_height_meters: 0.0,
    };
    let camera = CameraDefinition::Pose(GeodeticCameraPose {
        position: origin,
        orientation: HeadingPitchRoll {
            heading_degrees: 0.0,
            pitch_degrees: 0.0,
            roll_degrees: 0.0,
        },
        vertical_fov_degrees: 60.0,
    });
    let first_view = service
        .create_view(
            "gpu-smoke-owner-a",
            CreateViewRequest {
                scene_layer: layer_id.clone(),
                camera: camera.clone(),
            },
        )
        .await?;
    let second_view = service
        .create_view(
            "gpu-smoke-owner-b",
            CreateViewRequest {
                scene_layer: layer_id,
                camera: camera.clone(),
            },
        )
        .await?;
    ensure!(
        matches!(
            service
                .get_view("gpu-smoke-owner-a", &second_view.view_id)
                .await,
            Err(veoveo_view_mcp::state::ServiceError::ViewNotFound)
        ),
        "one owner could read another owner's view"
    );
    let first_view = service
        .set_camera(
            "gpu-smoke-owner-a",
            SetCameraRequest {
                view_id: first_view.view_id,
                expected_revision: first_view.revision,
                camera,
            },
        )
        .await?;
    ensure!(first_view.revision == 2, "camera revision did not advance");
    ensure!(
        service
            .get_view("gpu-smoke-owner-b", &second_view.view_id)
            .await?
            .revision
            == 1,
        "one view's camera update changed another view"
    );
    let policy = CapturePolicy {
        width_px: 256,
        height_px: 256,
        max_screen_error_px: 8.0,
        deadline_ms: 5_000,
        deadline_behavior: DeadlineBehavior::Fail,
        encoding: FrameEncoding::Png,
    };
    let requests = [
        (
            "gpu-smoke-owner-a",
            CaptureFrameRequest {
                view_id: first_view.view_id,
                expected_revision: first_view.revision,
                policy: policy.clone(),
            },
        ),
        (
            "gpu-smoke-owner-b",
            CaptureFrameRequest {
                view_id: second_view.view_id,
                expected_revision: second_view.revision,
                policy,
            },
        ),
    ];
    let mut proof_foreground_pixels = 0;
    for (capture_index, (owner, request)) in requests.into_iter().enumerate() {
        let expected_revision = request.expected_revision;
        let frame = service
            .capture_frame(owner, request, CancellationToken::new())
            .await
            .context("capture local 3D Tiles frame")?;
        ensure!(
            frame.record.view_revision == expected_revision,
            "capture did not preserve its accepted view revision"
        );
        ensure!(
            frame.bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            "capture was not PNG"
        );
        ensure!(frame.bytes.len() > 256, "captured image was too small");
        let pixels = image::load_from_memory(&frame.bytes)
            .context("decode captured PNG for visible-pixel assertion")?
            .to_rgb8();
        let background = pixels.get_pixel(0, 0).0;
        let foreground_pixels = pixels
            .pixels()
            .filter(|pixel| {
                pixel
                    .0
                    .iter()
                    .enumerate()
                    .any(|(index, channel)| channel.abs_diff(background[index]) > 24)
            })
            .count();
        ensure!(
            foreground_pixels > 256,
            "capture contained only the clear background"
        );
        if capture_index == 0 {
            proof_foreground_pixels = foreground_pixels;
        }
        ensure!(frame.record.visible_tile_count == 1, "tile was not visible");
        ensure!(
            frame.record.detail_complete,
            "detail selection was incomplete"
        );
        ensure!(
            frame
                .record
                .attribution
                .lines
                .iter()
                .any(|line| line == "Veoveo GPU smoke fixture"),
            "GLB attribution was not propagated"
        );
        if capture_index == 0
            && let Some(path) = proof_output.as_ref()
        {
            fs::write(path, &frame.bytes)
                .with_context(|| format!("write proof image {}", path.display()))?;
        }
    }
    let stats = renderer.stats().await?;
    ensure!(stats.resident_tiles == 1, "expected one resident GPU tile");
    ensure!(
        stats.tile_uploads == 1,
        "second view re-uploaded the shared cached tile"
    );
    println!(
        "adapter={} backend={} device_type={} foreground_pixels={} resident_tiles={} tile_uploads={}{}",
        renderer.adapter().name,
        renderer.adapter().backend,
        renderer.adapter().device_type,
        proof_foreground_pixels,
        stats.resident_tiles,
        stats.tile_uploads,
        proof_output
            .as_ref()
            .map(|path| format!(" proof_image={}", path.display()))
            .unwrap_or_default(),
    );
    Ok(())
}

struct LocalFixture {
    directory: PathBuf,
    tileset: PathBuf,
}

impl LocalFixture {
    fn create() -> Result<Self> {
        let directory =
            std::env::temp_dir().join(format!("veoveo-view-gpu-smoke-{}", std::process::id()));
        if directory.exists() {
            fs::remove_dir_all(&directory)?;
        }
        fs::create_dir_all(&directory)?;
        fs::write(directory.join("triangle.glb"), triangle_glb()?)?;

        let origin = Wgs84Position3d {
            latitude_degrees: 0.0,
            longitude_degrees: 0.0,
            ellipsoidal_height_meters: 0.0,
        };
        let desired_world = DMat4::from_translation(DVec3::new(0.0, 0.0, -5.0));
        let ecef_from_tile =
            world_from_ecef(origin).inverse() * desired_world * Y_UP_TO_Z_UP.inverse();
        let tileset = json!({
            "asset": { "version": "1.1" },
            "geometricError": 0.0,
            "root": {
                "boundingVolume": { "sphere": [0.0, 0.0, 0.0, 2.0] },
                "geometricError": 0.0,
                "refine": "REPLACE",
                "transform": ecef_from_tile.to_cols_array(),
                "content": { "uri": "triangle.glb" }
            }
        });
        let tileset_path = directory.join("tileset.json");
        fs::write(&tileset_path, serde_json::to_vec(&tileset)?)?;
        Ok(Self {
            directory,
            tileset: tileset_path,
        })
    }
}

impl Drop for LocalFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
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
        "asset": {
            "version": "2.0",
            "generator": "veoveo-view-gpu-smoke",
            "copyright": "Veoveo GPU smoke fixture"
        },
        "extensionsUsed": ["KHR_materials_unlit"],
        "buffers": [{ "byteLength": binary.len() }],
        "bufferViews": [
            { "buffer": 0, "byteOffset": 0, "byteLength": 36, "target": 34962 },
            { "buffer": 0, "byteOffset": 36, "byteLength": 6, "target": 34963 }
        ],
        "accessors": [
            {
                "bufferView": 0,
                "componentType": 5126,
                "count": 3,
                "type": "VEC3",
                "min": [-1.0, -1.0, 0.0],
                "max": [1.0, 1.0, 0.0]
            },
            {
                "bufferView": 1,
                "componentType": 5123,
                "count": 3,
                "type": "SCALAR",
                "min": [0],
                "max": [2]
            }
        ],
        "materials": [{
            "pbrMetallicRoughness": {
                "baseColorFactor": [0.8, 0.1, 0.05, 1.0],
                "metallicFactor": 0.0,
                "roughnessFactor": 1.0
            },
            "doubleSided": true,
            "extensions": { "KHR_materials_unlit": {} }
        }],
        "meshes": [{
            "primitives": [{
                "attributes": { "POSITION": 0 },
                "indices": 1,
                "material": 0,
                "mode": 4
            }]
        }],
        "nodes": [{ "mesh": 0 }],
        "scenes": [{ "nodes": [0] }],
        "scene": 0
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
