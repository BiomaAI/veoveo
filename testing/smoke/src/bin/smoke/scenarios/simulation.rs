use std::{collections::BTreeMap, os::unix::fs::PermissionsExt, process::Stdio};

use anyhow::ensure;
use chrono::Utc;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use veoveo_extension_contract::{
    ArtifactCoordinate, ArtifactDigest, SimulationAttestationEvidence, SimulationConformanceResult,
    SimulationConformanceResultSchema, SimulationHardwareEvidence, SimulationOverlayKind,
    SimulationProbeKind, SimulationProbeResult, SimulationRuntimeBuildLock, SourceRevision,
};

use super::*;

const RESULT_MARKER: &str = "VEOVEO_SIMULATION_PROBE_RESULT=";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProbePayload {
    components: ObservedComponents,
    hardware: ProbeHardware,
    camera_count: u32,
    module_roots: ModuleRoots,
    newton_camera: NewtonCameraResult,
    rtx: RtxResult,
    overlay: OverlayResult,
    probe_durations_milliseconds: ProbeDurations,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
struct ObservedComponents {
    isaac_sim: String,
    isaac_lab: String,
    warp: String,
    newton: String,
    mujoco: String,
    mujoco_warp: String,
    python: String,
    torch: String,
    cuda: String,
    isaac_rtx_nvrtc: String,
    kit: String,
}

impl ObservedComponents {
    fn as_map(&self) -> BTreeMap<&'static str, &str> {
        BTreeMap::from([
            ("cuda", self.cuda.as_str()),
            ("isaac_lab", self.isaac_lab.as_str()),
            ("isaac_sim", self.isaac_sim.as_str()),
            ("kit", self.kit.as_str()),
            ("mujoco", self.mujoco.as_str()),
            ("mujoco_warp", self.mujoco_warp.as_str()),
            ("newton", self.newton.as_str()),
            ("python", self.python.as_str()),
            ("torch", self.torch.as_str()),
            ("warp", self.warp.as_str()),
            ("isaac_rtx_nvrtc", self.isaac_rtx_nvrtc.as_str()),
        ])
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProbeHardware {
    gpu_name: String,
    driver_version: String,
    cuda_device: String,
    graphics_api: String,
    renderer: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModuleRoots {
    torch: String,
    warp: String,
    newton: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NewtonCameraResult {
    shape: Vec<u64>,
    unique_pixel_values: u64,
    unique_frame_hashes: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RtxResult {
    shape: Vec<u64>,
    minimum_standard_deviation: f64,
    unique_frame_hashes: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OverlayResult {
    kind: String,
    module: String,
    marker: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProbeDurations {
    component_tuple: u64,
    module_graph: u64,
    newton_tiled_camera: u64,
    independent_rtx_cameras: u64,
    overlay_boundary: u64,
}

pub(crate) async fn simulation_certify(
    base_image: &str,
    overlay_image: &str,
    overlay_kind: SimulationOverlayKind,
    source_revision: &str,
    output: &Path,
    cache_directory: &Path,
    timeout: Duration,
) -> Result<()> {
    let (base_coordinate, base_digest) = exact_oci_image("base image", base_image)?;
    let (overlay_coordinate, overlay_digest) = exact_oci_image("overlay image", overlay_image)?;
    let source_revision = SourceRevision::new(source_revision)?;
    ensure!(
        timeout >= Duration::from_secs(120),
        "simulation certification timeout must allow at least 120 seconds"
    );

    let build_lock_bytes = image_file(
        overlay_image,
        "/opt/veoveo/simulation-base/simulation-runtime.lock.json",
    )?;
    let build_lock: SimulationRuntimeBuildLock =
        serde_json::from_slice(&build_lock_bytes).context("decoding embedded simulation lock")?;
    build_lock.validate()?;
    let build_lock_digest = ArtifactDigest::new(format!(
        "sha256:{}",
        hex::encode(Sha256::digest(&build_lock_bytes))
    ))?;

    let sbom = inspect_attestation(base_image, "SBOM")?;
    let provenance = inspect_attestation(base_image, "Provenance")?;
    let attestations = SimulationAttestationEvidence {
        sbom_digest: ArtifactDigest::new(format!("sha256:{}", hex::encode(Sha256::digest(&sbom))))?,
        provenance_digest: ArtifactDigest::new(format!(
            "sha256:{}",
            hex::encode(Sha256::digest(&provenance))
        ))?,
    };

    fs::create_dir_all(cache_directory).with_context(|| {
        format!(
            "creating simulation certification cache {}",
            cache_directory.display()
        )
    })?;
    fs::set_permissions(cache_directory, fs::Permissions::from_mode(0o777)).with_context(|| {
        format!(
            "making simulation certification cache writable: {}",
            cache_directory.display()
        )
    })?;
    let cache_directory = cache_directory.canonicalize()?;
    let container_name = format!("veoveo-simulation-certify-{}", uuid::Uuid::new_v4());
    let _container = ContainerGuard::new(container_name.clone());
    let overlay_argument = match overlay_kind {
        SimulationOverlayKind::FirstPartyUav => "first_party_uav",
        SimulationOverlayKind::AnonymousExternal => "anonymous_external",
    };
    let mut command = tokio::process::Command::new("docker");
    command
        .args([
            "run",
            "--rm",
            "--name",
            &container_name,
            "--gpus",
            "all",
            "--runtime",
            "nvidia",
            "--network",
            "none",
            "--shm-size",
            "2g",
            "-e",
            "NVIDIA_VISIBLE_DEVICES=all",
            "-e",
            "NVIDIA_DRIVER_CAPABILITIES=compute,graphics,utility,video",
            "-e",
            "XDG_CACHE_HOME=/var/lib/veoveo/.cache",
            "-v",
            &format!("{}:/var/lib/veoveo/.cache", cache_directory.display()),
            "--entrypoint",
            "/isaac-sim/python.sh",
            overlay_image,
            "-m",
            "veoveo_simulation_base.probe",
            "--overlay-kind",
            overlay_argument,
            "--cameras",
            "20",
        ])
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let process = tokio::time::timeout(timeout, command.output())
        .await
        .with_context(|| format!("simulation certification exceeded {timeout:?}"))??;
    let stdout = String::from_utf8_lossy(&process.stdout);
    let stderr = String::from_utf8_lossy(&process.stderr);
    let transcript = format!("{stdout}\n{stderr}");
    ensure!(
        process.status.success(),
        "simulation certification failed with {}\n{}",
        process.status,
        transcript
    );
    for forbidden in ["SwiftShader", "llvmpipe", "Software Rasterizer"] {
        ensure!(
            !transcript.contains(forbidden),
            "simulation certification selected forbidden renderer {forbidden}"
        );
    }
    ensure!(
        transcript.contains("Graphics API: Vulkan"),
        "simulation certification did not prove Vulkan hardware"
    );
    let payload = transcript
        .lines()
        .find_map(|line| line.strip_prefix(RESULT_MARKER))
        .context("simulation certification emitted no typed result marker")?;
    let payload: ProbePayload =
        serde_json::from_str(payload).context("decoding simulation probe result")?;
    validate_probe(&payload, &build_lock, overlay_argument)?;
    ensure!(
        driver_at_least(
            &payload.hardware.driver_version,
            &build_lock.gpu.minimum_driver_version
        )?,
        "NVIDIA driver {} is older than required {}",
        payload.hardware.driver_version,
        build_lock.gpu.minimum_driver_version
    );

    let result = SimulationConformanceResult {
        schema_version: SimulationConformanceResultSchema::V1,
        profile: build_lock.profile,
        base_image: base_coordinate,
        base_digest,
        overlay_kind,
        overlay_image: overlay_coordinate,
        overlay_digest,
        source_revision,
        build_lock_digest,
        components: build_lock.components,
        hardware: SimulationHardwareEvidence {
            gpu_name: payload.hardware.gpu_name,
            driver_version: payload.hardware.driver_version,
            cuda_device: payload.hardware.cuda_device,
            graphics_api: payload.hardware.graphics_api,
            renderer: payload.hardware.renderer,
        },
        attestations,
        camera_count: payload.camera_count,
        completed_at: Utc::now().to_rfc3339(),
        probes: vec![
            probe(
                SimulationProbeKind::ComponentTuple,
                payload.probe_durations_milliseconds.component_tuple,
            ),
            probe(
                SimulationProbeKind::ModuleGraph,
                payload.probe_durations_milliseconds.module_graph,
            ),
            probe(
                SimulationProbeKind::NewtonTiledCamera,
                payload.probe_durations_milliseconds.newton_tiled_camera,
            ),
            probe(
                SimulationProbeKind::IndependentRtxCameras,
                payload.probe_durations_milliseconds.independent_rtx_cameras,
            ),
            probe(
                SimulationProbeKind::OverlayBoundary,
                payload.probe_durations_milliseconds.overlay_boundary,
            ),
        ],
    };
    result.validate()?;
    let parent = output
        .parent()
        .context("simulation result output has no parent directory")?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, &result)?;
    use std::io::Write as _;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(output)
        .map_err(|error| error.error)
        .with_context(|| format!("publishing simulation result {}", output.display()))?;
    println!(
        "Simulation certification passed: {:?} overlay {} on base {}; result {}",
        overlay_kind,
        overlay_image,
        base_image,
        output.display()
    );
    Ok(())
}

fn validate_probe(
    payload: &ProbePayload,
    build_lock: &SimulationRuntimeBuildLock,
    expected_overlay: &str,
) -> Result<()> {
    let expected = build_lock
        .components
        .iter()
        .map(|component| {
            (
                serde_json::to_value(component.component)
                    .expect("runtime component serializes")
                    .as_str()
                    .expect("runtime component is a string")
                    .to_owned(),
                component.version.as_str(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for (name, observed) in payload.components.as_map() {
        ensure!(
            expected.get(name).copied() == Some(observed),
            "probe observed unexpected {name} version {observed}"
        );
    }
    ensure!(
        payload.camera_count >= 20
            && payload.newton_camera.shape.first().copied() == Some(1)
            && payload.newton_camera.shape.get(1).copied() == Some(payload.camera_count.into())
            && payload.newton_camera.unique_pixel_values >= 2
            && payload.newton_camera.unique_frame_hashes == payload.camera_count,
        "Newton tiled-camera evidence is incomplete"
    );
    ensure!(
        payload.rtx.shape.first().copied() == Some(payload.camera_count.into())
            && payload.rtx.unique_frame_hashes == payload.camera_count
            && payload.rtx.minimum_standard_deviation >= 1.0,
        "independent RTX camera evidence is incomplete"
    );
    ensure!(
        payload
            .module_roots
            .torch
            .contains("omni.isaac.ml_archive/pip_prebundle/torch")
            && payload.module_roots.warp.contains("omni.warp.core")
            && payload.module_roots.newton.contains("isaacsim.pip.newton"),
        "Torch, Warp, and Newton did not load from their authoritative roots"
    );
    ensure!(
        payload.overlay.kind == expected_overlay
            && !payload.overlay.module.trim().is_empty()
            && !payload.overlay.marker.trim().is_empty(),
        "overlay boundary evidence differs from the requested overlay"
    );
    Ok(())
}

fn probe(probe: SimulationProbeKind, duration_milliseconds: u64) -> SimulationProbeResult {
    SimulationProbeResult {
        probe,
        duration_milliseconds,
    }
}

fn exact_oci_image(field: &str, reference: &str) -> Result<(ArtifactCoordinate, ArtifactDigest)> {
    let (repository, digest) = reference
        .rsplit_once('@')
        .with_context(|| format!("{field} must use repository@sha256 identity"))?;
    ensure!(
        !repository.contains("://") && !repository.is_empty(),
        "{field} must be an OCI repository without a URL scheme"
    );
    let digest = ArtifactDigest::new(digest)?;
    let coordinate = ArtifactCoordinate::new(format!("oci://{repository}@{digest}"))?;
    Ok((coordinate, digest))
}

fn image_file(image: &str, path: &str) -> Result<Vec<u8>> {
    let output = Command::new("docker")
        .args(["run", "--rm", "--entrypoint", "/bin/cat", image, path])
        .output()
        .with_context(|| format!("reading {path} from {image}"))?;
    ensure!(
        output.status.success(),
        "reading {path} from {image} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

fn inspect_attestation(image: &str, field: &str) -> Result<Vec<u8>> {
    let template = format!("{{{{json .{field}}}}}");
    let output = Command::new("docker")
        .args([
            "buildx",
            "imagetools",
            "inspect",
            "--format",
            &template,
            image,
        ])
        .output()
        .with_context(|| format!("inspecting {field} attestation for {image}"))?;
    ensure!(
        output.status.success(),
        "inspecting {field} attestation for {image} failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let trimmed = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    ensure!(
        !matches!(trimmed.as_str(), "" | "null" | "{}" | "[]"),
        "{image} has no {field} attestation"
    );
    serde_json::from_str::<serde_json::Value>(&trimmed)
        .with_context(|| format!("decoding {field} attestation for {image}"))?;
    Ok(trimmed.into_bytes())
}

fn driver_at_least(actual: &str, minimum: &str) -> Result<bool> {
    fn parse(value: &str) -> Result<Vec<u64>> {
        value
            .split('.')
            .map(|part| {
                part.parse::<u64>()
                    .with_context(|| format!("invalid NVIDIA driver version {value}"))
            })
            .collect()
    }
    let mut actual = parse(actual)?;
    let mut minimum = parse(minimum)?;
    let width = actual.len().max(minimum.len());
    actual.resize(width, 0);
    minimum.resize(width, 0);
    Ok(actual >= minimum)
}
