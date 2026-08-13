use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Write,
    os::unix::fs::PermissionsExt,
    process::{ExitStatus, Output, Stdio},
};

use anyhow::{bail, ensure};
use chrono::Utc;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncBufReadExt, BufReader};
use veoveo_deploy_contract::{DeploymentLock, RegistryTransport};
use veoveo_extension_contract::{
    ArtifactCoordinate, ArtifactDigest, SimulationAttestationEvidence, SimulationConformanceResult,
    SimulationConformanceResultSchema, SimulationHardwareEvidence, SimulationOverlayKind,
    SimulationProbeKind, SimulationProbeResult, SimulationRuntimeBuildLock, SourceRevision,
};

use super::*;

const RESULT_MARKER: &str = "VEOVEO_SIMULATION_PROBE_RESULT=";
const EMBEDDED_BUILD_LOCK: &str = "/opt/veoveo/simulation-base/simulation-runtime.lock.json";

#[derive(Debug)]
struct RegistryAccess {
    address: Option<String>,
    transport: RegistryTransport,
}

struct MaterializedImage {
    tag: String,
}

struct Transcript {
    path: PathBuf,
    file: File,
}

impl Transcript {
    fn create(output: &Path) -> Result<Self> {
        let path = output.with_extension("transcript.log");
        let parent = path
            .parent()
            .context("simulation transcript has no parent directory")?;
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "creating simulation transcript directory {}",
                parent.display()
            )
        })?;
        let mut file = File::create(&path)
            .with_context(|| format!("creating simulation transcript {}", path.display()))?;
        writeln!(
            file,
            "Veoveo simulation certification transcript\nstartedAt={}",
            Utc::now().to_rfc3339()
        )?;
        file.sync_data()?;
        Ok(Self { path, file })
    }

    fn stage(&mut self, name: &str) -> Result<()> {
        writeln!(self.file, "\n== {name} ==")?;
        self.file.flush()?;
        Ok(())
    }

    fn line(&mut self, stream: &str, line: &str) -> Result<()> {
        writeln!(self.file, "[{stream}] {line}")?;
        self.file.flush()?;
        Ok(())
    }

    fn output(&mut self, output: &Output) -> Result<()> {
        writeln!(self.file, "status={}", output.status)?;
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            self.line("stdout", line)?;
        }
        for line in String::from_utf8_lossy(&output.stderr).lines() {
            self.line("stderr", line)?;
        }
        Ok(())
    }

    fn failure(&mut self, error: &anyhow::Error) {
        let _ = writeln!(self.file, "\n== certification failed ==\n{error:#}");
        let _ = self.file.sync_all();
    }
}

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

#[allow(clippy::too_many_arguments)]
pub(crate) async fn simulation_certify(
    deployment_lock: Option<&Path>,
    base_image: &str,
    overlay_image: &str,
    overlay_kind: SimulationOverlayKind,
    source_revision: &str,
    output: &Path,
    cache_directory: &Path,
    timeout: Duration,
) -> Result<()> {
    let mut transcript = Transcript::create(output)?;
    let result = simulation_certify_inner(
        deployment_lock,
        base_image,
        overlay_image,
        overlay_kind,
        source_revision,
        output,
        cache_directory,
        timeout,
        &mut transcript,
    )
    .await;
    if let Err(error) = &result {
        transcript.failure(error);
    }
    result.with_context(|| {
        format!(
            "simulation certification transcript retained at {}",
            transcript.path.display()
        )
    })
}

#[allow(clippy::too_many_arguments)]
async fn simulation_certify_inner(
    deployment_lock: Option<&Path>,
    base_image: &str,
    overlay_image: &str,
    overlay_kind: SimulationOverlayKind,
    source_revision: &str,
    output: &Path,
    cache_directory: &Path,
    timeout: Duration,
    transcript: &mut Transcript,
) -> Result<()> {
    let (base_coordinate, base_digest) = exact_oci_image("base image", base_image)?;
    let (overlay_coordinate, overlay_digest) = exact_oci_image("overlay image", overlay_image)?;
    let source_revision = SourceRevision::new(source_revision)?;
    ensure!(
        timeout >= Duration::from_secs(120),
        "simulation certification timeout must allow at least 120 seconds"
    );

    let registry = registry_access(deployment_lock, base_image, overlay_image)?;
    let repository = repository_root()?;
    transcript.stage("managed BuildKit registry resolver")?;
    let _builder = match &registry.address {
        Some(address) => veoveo_image_build_control::ensure_for_registry(
            &repository,
            address,
            registry.transport,
        )?,
        None => veoveo_image_build_control::ensure(&repository)?,
    };

    transcript.stage("image environment invariants")?;
    let base_environment = inspect_image_environment(&repository, base_image, transcript)?;
    let overlay_environment = inspect_image_environment(&repository, overlay_image, transcript)?;
    validate_inherited_python_path(&base_environment, &overlay_environment)?;

    transcript.stage("digest-addressed overlay materialization")?;
    let materialized = materialize_image(&repository, overlay_image, transcript)
        .context("materializing overlay")?;
    let build_lock_bytes = image_file(&materialized.tag, EMBEDDED_BUILD_LOCK, transcript)?;
    let build_lock: SimulationRuntimeBuildLock =
        serde_json::from_slice(&build_lock_bytes).context("decoding embedded simulation lock")?;
    build_lock.validate()?;
    let build_lock_digest = ArtifactDigest::new(format!(
        "sha256:{}",
        hex::encode(Sha256::digest(&build_lock_bytes))
    ))?;

    transcript.stage("canonical base attestations")?;
    let sbom = inspect_attestation(&repository, base_image, "SBOM", transcript)?;
    let provenance = inspect_attestation(&repository, base_image, "Provenance", transcript)?;
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
    transcript.stage("hardware GPU conformance probe")?;
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
            "--pull",
            "never",
            &materialized.tag,
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
    let (status, probe_transcript) = run_logged(&mut command, timeout, transcript).await?;
    ensure!(
        status.success(),
        "simulation certification failed with {status}"
    );
    for forbidden in ["SwiftShader", "llvmpipe", "Software Rasterizer"] {
        ensure!(
            !probe_transcript.contains(forbidden),
            "simulation certification selected forbidden renderer {forbidden}"
        );
    }
    ensure!(
        probe_transcript.contains("Graphics API: Vulkan"),
        "simulation certification did not prove Vulkan hardware"
    );
    let payload = probe_transcript
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
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(output)
        .map_err(|error| error.error)
        .with_context(|| format!("publishing simulation result {}", output.display()))?;
    println!(
        "Simulation certification passed: {:?} overlay {} on base {}; result {}; transcript {}",
        overlay_kind,
        overlay_image,
        base_image,
        output.display(),
        transcript.path.display()
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

fn registry_access(
    deployment_lock: Option<&Path>,
    base_image: &str,
    overlay_image: &str,
) -> Result<RegistryAccess> {
    let Some(path) = deployment_lock else {
        return Ok(RegistryAccess {
            address: None,
            transport: RegistryTransport::Tls,
        });
    };
    let bytes =
        fs::read(path).with_context(|| format!("reading deployment lock {}", path.display()))?;
    let lock: DeploymentLock = serde_json::from_slice(&bytes)
        .with_context(|| format!("decoding deployment lock {}", path.display()))?;
    lock.validate()?;
    for (field, image) in [("base image", base_image), ("overlay image", overlay_image)] {
        validate_locked_authority(field, image, &lock.registry.pull_address)?;
    }
    Ok(RegistryAccess {
        address: Some(lock.registry.push_address),
        transport: lock.registry.transport,
    })
}

fn validate_locked_authority(field: &str, image: &str, registry: &str) -> Result<()> {
    let repository = image
        .rsplit_once('@')
        .map(|(repository, _)| repository)
        .with_context(|| format!("{field} must use repository@sha256 identity"))?;
    let authority = repository
        .split_once('/')
        .map(|(authority, _)| authority)
        .with_context(|| format!("{field} must include an explicit registry authority"))?;
    ensure!(
        authority == registry,
        "{field} uses registry {authority}; deployment lock authorizes {registry}"
    );
    Ok(())
}

fn repository_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("locating the Veoveo repository")?;
    ensure!(
        output.status.success(),
        "locating the Veoveo repository failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let root = String::from_utf8(output.stdout).context("repository path is not UTF-8")?;
    Ok(PathBuf::from(root.trim()))
}

fn inspect_image_environment(
    repository: &Path,
    image: &str,
    transcript: &mut Transcript,
) -> Result<BTreeMap<String, String>> {
    let mut command = veoveo_image_build_control::buildx_command(repository)?;
    command.args([
        "imagetools",
        "inspect",
        "--builder",
        veoveo_image_build_control::BUILDER_NAME,
        "--format",
        "{{json .Image}}",
        image,
    ]);
    let output = command_output(&mut command, transcript, &format!("inspect config {image}"))?;
    ensure!(
        output.status.success(),
        "inspecting image configuration for {image} failed"
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("decoding image configuration for {image}"))?;
    unique_environment(&value).with_context(|| format!("reading image environment from {image}"))
}

fn unique_environment(value: &serde_json::Value) -> Result<BTreeMap<String, String>> {
    fn visit(value: &serde_json::Value, candidates: &mut Vec<BTreeMap<String, String>>) {
        match value {
            serde_json::Value::Array(values) => {
                for value in values {
                    visit(value, candidates);
                }
            }
            serde_json::Value::Object(object) => {
                if let Some(config) = object.get("config").or_else(|| object.get("Config"))
                    && let Some(environment) = config.get("Env").or_else(|| config.get("env"))
                    && let Some(environment) = environment.as_array()
                {
                    let values = environment
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .filter_map(|item| item.split_once('='))
                        .map(|(name, value)| (name.to_owned(), value.to_owned()))
                        .collect::<BTreeMap<_, _>>();
                    candidates.push(values);
                    return;
                }
                for value in object.values() {
                    visit(value, candidates);
                }
            }
            _ => {}
        }
    }

    let mut candidates = Vec::new();
    visit(value, &mut candidates);
    ensure!(
        !candidates.is_empty(),
        "Buildx image inspection returned no image configuration"
    );
    let unique = candidates.into_iter().collect::<BTreeSet<_>>();
    ensure!(
        unique.len() == 1,
        "image index contains different environments across runtime manifests"
    );
    Ok(unique.into_iter().next().expect("one environment"))
}

fn validate_inherited_python_path(
    base: &BTreeMap<String, String>,
    overlay: &BTreeMap<String, String>,
) -> Result<()> {
    let base = base
        .get("PYTHONPATH")
        .context("canonical simulation runtime declares no PYTHONPATH")?;
    let overlay = overlay
        .get("PYTHONPATH")
        .context("simulation overlay declares no PYTHONPATH")?;
    let base_roots = base
        .split(':')
        .filter(|root| !root.is_empty())
        .collect::<Vec<_>>();
    let overlay_roots = overlay
        .split(':')
        .filter(|root| !root.is_empty())
        .collect::<Vec<_>>();
    let mut position = 0usize;
    for root in &base_roots {
        let Some(offset) = overlay_roots[position..]
            .iter()
            .position(|candidate| candidate == root)
        else {
            bail!("simulation overlay removed or reordered platform Python root {root}");
        };
        position += offset + 1;
    }
    ensure!(
        base_roots.contains(&"/opt/veoveo/python")
            && base_roots
                .iter()
                .any(|root| root.starts_with("/opt/veoveo/isaaclab/source/")),
        "canonical simulation runtime PYTHONPATH omits platform or Isaac Lab roots"
    );
    Ok(())
}

fn materialize_image(
    repository: &Path,
    image: &str,
    transcript: &mut Transcript,
) -> Result<MaterializedImage> {
    let tag = format!(
        "{}:{}",
        veoveo_image_build_control::CERTIFICATION_CACHE_REPOSITORY,
        hex::encode(Sha256::digest(image.as_bytes()))
    );
    if cached_materialization(&tag, image, transcript)? {
        transcript.line("cache", &format!("reusing {tag}"))?;
        return Ok(MaterializedImage { tag });
    }
    let context = tempfile::tempdir().context("creating image materialization context")?;
    fs::write(
        context.path().join("Dockerfile"),
        "# syntax=docker/dockerfile:1.25.0\nARG CERT_IMAGE\nFROM ${CERT_IMAGE}\nARG CERT_IMAGE\nLABEL io.veoveo.certification.source=\"${CERT_IMAGE}\"\n",
    )?;
    let mut command = veoveo_image_build_control::buildx_command(repository)?;
    command
        .args([
            "build",
            "--builder",
            veoveo_image_build_control::BUILDER_NAME,
            "--platform",
            "linux/amd64",
            "--pull",
            "--provenance=false",
            "--build-arg",
            &format!("CERT_IMAGE={image}"),
            "--output",
            "type=docker",
            "--tag",
            &tag,
        ])
        .arg(context.path());
    let output = command_output(&mut command, transcript, &format!("materialize {image}"))?;
    ensure!(
        output.status.success(),
        "BuildKit failed to materialize exact image {image}"
    );
    ensure!(
        cached_materialization(&tag, image, transcript)?,
        "materialized image does not retain its exact source identity"
    );
    Ok(MaterializedImage { tag })
}

fn cached_materialization(tag: &str, image: &str, transcript: &mut Transcript) -> Result<bool> {
    let mut command = Command::new("docker");
    command.args([
        "image",
        "inspect",
        tag,
        "--format",
        "{{json (index .Config.Labels \"io.veoveo.certification.source\")}}",
    ]);
    let output = command_output(
        &mut command,
        transcript,
        &format!("inspect certification cache {tag}"),
    )?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No such image") {
            return Ok(false);
        }
        bail!("inspecting certification cache {tag} failed");
    }
    let source: String = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("decoding certification cache identity for {tag}"))?;
    ensure!(
        source == image,
        "certification cache {tag} identifies {source}; expected {image}"
    );
    Ok(true)
}

fn image_file(image: &str, path: &str, transcript: &mut Transcript) -> Result<Vec<u8>> {
    let mut command = Command::new("docker");
    command.args([
        "run",
        "--rm",
        "--pull",
        "never",
        "--entrypoint",
        "/bin/cat",
        image,
        path,
    ]);
    let output = command_output(
        &mut command,
        transcript,
        &format!("read embedded file {path}"),
    )?;
    ensure!(
        output.status.success(),
        "reading {path} from materialized image failed"
    );
    Ok(output.stdout)
}

fn inspect_attestation(
    repository: &Path,
    image: &str,
    field: &str,
    transcript: &mut Transcript,
) -> Result<Vec<u8>> {
    let template = format!("{{{{json .{field}}}}}");
    let mut command = veoveo_image_build_control::buildx_command(repository)?;
    command.args([
        "imagetools",
        "inspect",
        "--builder",
        veoveo_image_build_control::BUILDER_NAME,
        "--format",
        &template,
        image,
    ]);
    let output = command_output(
        &mut command,
        transcript,
        &format!("inspect {field} attestation for {image}"),
    )?;
    ensure!(
        output.status.success(),
        "inspecting {field} attestation for {image} failed"
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

fn command_output(
    command: &mut Command,
    transcript: &mut Transcript,
    stage: &str,
) -> Result<Output> {
    transcript.stage(stage)?;
    let output = command
        .output()
        .with_context(|| format!("running {stage}"))?;
    transcript.output(&output)?;
    Ok(output)
}

async fn run_logged(
    command: &mut tokio::process::Command,
    timeout: Duration,
    transcript: &mut Transcript,
) -> Result<(ExitStatus, String)> {
    let mut child = command.spawn().context("starting simulation GPU probe")?;
    let stdout = child
        .stdout
        .take()
        .context("simulation GPU probe stdout is not piped")?;
    let stderr = child
        .stderr
        .take()
        .context("simulation GPU probe stderr is not piped")?;
    let mut stdout = BufReader::new(stdout).lines();
    let mut stderr = BufReader::new(stderr).lines();
    let mut stdout_open = true;
    let mut stderr_open = true;
    let mut complete = String::new();
    let deadline = tokio::time::sleep(timeout);
    tokio::pin!(deadline);

    while stdout_open || stderr_open {
        tokio::select! {
            line = stdout.next_line(), if stdout_open => match line? {
                Some(line) => {
                    println!("{line}");
                    transcript.line("stdout", &line)?;
                    complete.push_str(&line);
                    complete.push('\n');
                }
                None => stdout_open = false,
            },
            line = stderr.next_line(), if stderr_open => match line? {
                Some(line) => {
                    eprintln!("{line}");
                    transcript.line("stderr", &line)?;
                    complete.push_str(&line);
                    complete.push('\n');
                }
                None => stderr_open = false,
            },
            () = &mut deadline => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                bail!("simulation certification exceeded {timeout:?}");
            }
        }
    }
    let status = child
        .wait()
        .await
        .context("waiting for simulation GPU probe")?;
    transcript.line("status", &status.to_string())?;
    Ok((status, complete))
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

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, process::Stdio, time::Duration};

    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        Transcript, run_logged, unique_environment, validate_inherited_python_path,
        validate_locked_authority,
    };

    #[test]
    fn deployment_lock_authority_accepts_exact_private_registry_with_arbitrary_port() {
        validate_locked_authority(
            "overlay image",
            "registry.acceptance.internal:5002/veoveo/overlay@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "registry.acceptance.internal:5002",
        )
        .expect("accept selected registry");
        let error = validate_locked_authority(
            "overlay image",
            "127.0.0.1:5002/veoveo/overlay@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "registry.acceptance.internal:5002",
        )
        .expect_err("reject substituted registry identity");
        assert!(error.to_string().contains("deployment lock authorizes"));
    }

    #[test]
    fn image_environment_parser_accepts_single_and_index_shapes() {
        let single = json!({"config": {"Env": ["HOME=/tmp", "PYTHONPATH=/base"]}});
        assert_eq!(
            unique_environment(&single)
                .expect("single environment")
                .get("PYTHONPATH")
                .map(String::as_str),
            Some("/base")
        );
        let index = json!({
            "linux/amd64": {"config": {"Env": ["HOME=/tmp", "PYTHONPATH=/base"]}},
            "linux/amd64/attestation": {"config": {"Env": ["HOME=/tmp", "PYTHONPATH=/base"]}}
        });
        assert_eq!(
            unique_environment(&index)
                .expect("identical index environments")
                .get("PYTHONPATH")
                .map(String::as_str),
            Some("/base")
        );
    }

    #[test]
    fn overlay_python_path_must_preserve_every_platform_root_in_order() {
        let base = BTreeMap::from([(
            "PYTHONPATH".to_owned(),
            "/isaac-sim/extsDeprecated/omni.isaac.ml_archive/pip_prebundle:/opt/veoveo/python:/opt/veoveo/isaaclab/source/isaaclab:/opt/veoveo/isaaclab/source/isaaclab_newton"
                .to_owned(),
        )]);
        let valid = BTreeMap::from([(
            "PYTHONPATH".to_owned(),
            "/opt/extension:/isaac-sim/extsDeprecated/omni.isaac.ml_archive/pip_prebundle:/opt/veoveo/python:/opt/veoveo/isaaclab/source/isaaclab:/opt/veoveo/isaaclab/source/isaaclab_newton:/opt/extension-tail"
                .to_owned(),
        )]);
        validate_inherited_python_path(&base, &valid).expect("monotonic extension");

        let missing = BTreeMap::from([(
            "PYTHONPATH".to_owned(),
            "/opt/extension:/opt/veoveo/python".to_owned(),
        )]);
        assert!(validate_inherited_python_path(&base, &missing).is_err());

        let reordered = BTreeMap::from([(
            "PYTHONPATH".to_owned(),
            "/opt/veoveo/python:/isaac-sim/extsDeprecated/omni.isaac.ml_archive/pip_prebundle:/opt/veoveo/isaaclab/source/isaaclab_newton:/opt/veoveo/isaaclab/source/isaaclab"
                .to_owned(),
        )]);
        assert!(validate_inherited_python_path(&base, &reordered).is_err());
    }

    #[tokio::test]
    async fn failing_and_timed_out_probes_retain_partial_transcripts() {
        let directory = tempdir().expect("temporary transcript directory");
        let output = directory.path().join("failure.result.json");
        let mut transcript = Transcript::create(&output).expect("create transcript");
        let mut failure = tokio::process::Command::new("sh");
        failure
            .args(["-c", "printf 'diagnostic-before-failure\\n'; exit 7"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let (status, _) = run_logged(&mut failure, Duration::from_secs(1), &mut transcript)
            .await
            .expect("collect failed command");
        assert_eq!(status.code(), Some(7));
        let contents = std::fs::read_to_string(&transcript.path).expect("read transcript");
        assert!(contents.contains("diagnostic-before-failure"));
        assert!(contents.contains("exit status: 7"));

        let output = directory.path().join("timeout.result.json");
        let mut transcript = Transcript::create(&output).expect("create timeout transcript");
        let mut timeout = tokio::process::Command::new("sh");
        timeout
            .args([
                "-c",
                "printf 'diagnostic-before-timeout\\n'; while :; do :; done",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let error = run_logged(&mut timeout, Duration::from_millis(20), &mut transcript)
            .await
            .expect_err("probe must time out");
        assert!(error.to_string().contains("exceeded"));
        let contents = std::fs::read_to_string(&transcript.path).expect("read timeout transcript");
        assert!(contents.contains("diagnostic-before-timeout"));
    }
}
