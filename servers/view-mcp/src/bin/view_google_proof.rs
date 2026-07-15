use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result, ensure};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use veoveo_view_mcp::{
    contract::{
        CameraDefinition, CaptureFrameRequest, CaptureLimits, CapturePolicy, CreateViewRequest,
        DeadlineBehavior, FrameEncoding, LayerId, OrbitTargetCamera, Wgs84Position3d,
    },
    renderer::{RendererConfig, RendererHandle},
    source::{
        GOOGLE_P3DT_ROOT_URL, LayerCatalog, LayerDefinition, LayerSourceDefinition, SourceConfig,
    },
    state::{ViewService, ViewServiceConfig},
};

const TARGET_LATITUDE: f64 = 40.689_249_4;
const TARGET_LONGITUDE: f64 = -74.044_500_4;
const TARGET_ELLIPSOIDAL_HEIGHT_METERS: f64 = 20.0;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    ensure!(
        std::env::var_os("GOOGLE_MAPS_API_KEY").is_some(),
        "GOOGLE_MAPS_API_KEY must be set"
    );
    let output = std::env::var_os("VIEW_GOOGLE_PROOF_OUTPUT")
        .map(PathBuf::from)
        .context("VIEW_GOOGLE_PROOF_OUTPUT must name the retained JPEG")?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create proof output directory {}", parent.display()))?;
    }

    let layer_id = LayerId::new("google-photorealistic")?;
    let catalog = LayerCatalog::from_definitions(
        vec![LayerDefinition {
            layer_id: layer_id.clone(),
            label: "Google Photorealistic 3D Tiles".to_owned(),
            source: LayerSourceDefinition::GooglePhotorealistic {
                root_url: GOOGLE_P3DT_ROOT_URL.to_owned(),
                api_key_env: "GOOGLE_MAPS_API_KEY".to_owned(),
                daily_request_cap: 2_000,
            },
        }],
        SourceConfig {
            raw_cache_bytes: 1024 * 1024 * 1024,
            max_response_bytes: 256 * 1024 * 1024,
            request_timeout: Duration::from_secs(30),
        },
    )?;
    let renderer = RendererHandle::start(RendererConfig {
        require_nvidia: true,
        gpu_cache_bytes: 6 * 1024 * 1024 * 1024,
        jpeg_quality: 92,
    })
    .context("initialize NVIDIA Vulkan renderer")?;
    let service = ViewService::new(
        ViewServiceConfig {
            capture_limits: CaptureLimits {
                max_width_px: 2_048,
                max_height_px: 2_048,
                max_pixels: 4_194_304,
                max_deadline_ms: 180_000,
            },
            max_views: 2,
            max_views_per_owner: 2,
            max_frames: 2,
            max_frame_bytes: 64 * 1024 * 1024,
            max_single_frame_bytes: 32 * 1024 * 1024,
            decoded_cache_bytes: 4 * 1024 * 1024 * 1024,
            max_concurrent_loads: 16,
            max_tree_nodes: 2_000_000,
            detail_falloff_meters: 2_000.0,
        },
        catalog,
        renderer.clone(),
    );
    let target = Wgs84Position3d {
        latitude_degrees: TARGET_LATITUDE,
        longitude_degrees: TARGET_LONGITUDE,
        ellipsoidal_height_meters: TARGET_ELLIPSOIDAL_HEIGHT_METERS,
    };
    let view = service
        .create_view(
            "google-live-proof",
            CreateViewRequest {
                scene_layer: layer_id,
                camera: CameraDefinition::OrbitTarget(OrbitTargetCamera {
                    target,
                    distance_meters: 650.0,
                    azimuth_degrees: 210.0,
                    elevation_degrees: 40.0,
                    vertical_fov_degrees: 45.0,
                }),
            },
        )
        .await?;
    let frame = service
        .capture_frame(
            "google-live-proof",
            CaptureFrameRequest {
                view_id: view.view_id,
                expected_revision: view.revision,
                policy: CapturePolicy {
                    width_px: 1_280,
                    height_px: 720,
                    max_screen_error_px: 16.0,
                    deadline_ms: 180_000,
                    deadline_behavior: DeadlineBehavior::ReturnBestAvailable,
                    encoding: FrameEncoding::Jpeg,
                },
            },
            CancellationToken::new(),
        )
        .await
        .context("capture live Google Photorealistic 3D Tiles frame")?;

    ensure!(
        frame.bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "live capture was not JPEG"
    );
    ensure!(
        frame.record.visible_tile_count > 0,
        "live capture selected no visible tiles"
    );
    let foreground_pixels = materially_different_pixels(&frame.bytes)?;
    ensure!(
        foreground_pixels > 10_000,
        "live capture did not contain a materially varied world scene"
    );
    fs::write(&output, &frame.bytes)
        .with_context(|| format!("write live proof image {}", output.display()))?;

    let digest = Sha256::digest(&frame.bytes);
    println!(
        "{}",
        serde_json::to_string(&json!({
            "adapter": renderer.adapter().name,
            "backend": renderer.adapter().backend,
            "device_type": renderer.adapter().device_type,
            "target": target,
            "resolved_camera": frame.record.resolved_camera,
            "visible_tiles": frame.record.visible_tile_count,
            "pending_tiles": frame.record.pending_tile_count,
            "detail_complete": frame.record.detail_complete,
            "actual_max_screen_error_px": frame.record.actual_max_screen_error_px,
            "attribution": frame.record.attribution,
            "foreground_pixels": foreground_pixels,
            "bytes": frame.bytes.len(),
            "sha256": hex::encode(digest),
            "proof_image": output,
        }))?
    );
    Ok(())
}

fn materially_different_pixels(bytes: &[u8]) -> Result<usize> {
    let pixels = image::load_from_memory(bytes)
        .context("decode live proof image")?
        .to_rgb8();
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
