use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Stdio,
};

use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use veoveo_extension_contract::{
    ArtifactCoordinate, ArtifactDescriptor, ArtifactDigest, ArtifactKind, ArtifactName,
    ArtifactPlatform, CpuArchitecture, OperatingSystem, ReleaseVersion,
    SimulationConformanceResult, SimulationOverlayKind, SimulationRuntimeBuildLock,
    SimulationRuntimeReleaseEvidence, SimulationRuntimeReleaseEvidenceSchema, SourceRevision,
    simulation_conformance_result_schema, simulation_runtime_build_lock_schema,
};

use crate::{ReleaseSimulationRuntimeArgs, commands::builder, context::RepositoryContext};

const CONFORMANCE_REPOSITORY: &str = "veoveo-simulation-conformance";
const OCI_INDEX_MEDIA_TYPE: &str = "application/vnd.oci.image.index.v1+json";
const DOCKERFILE: &str = r#"# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM scratch
ARG SOURCE_REVISION
LABEL org.opencontainers.image.title="Veoveo simulation conformance evidence" \
      org.opencontainers.image.revision="${SOURCE_REVISION}" \
      io.veoveo.artifact.kind="simulation-conformance-result"
COPY simulation-runtime.lock.json /veoveo/simulation/simulation-runtime.lock.json
COPY simulation-runtime-build-lock.schema.json /veoveo/simulation/schemas/simulation-runtime-build-lock.schema.json
COPY simulation-conformance-result.schema.json /veoveo/simulation/schemas/simulation-conformance-result.schema.json
COPY first-party-uav.result.json /veoveo/simulation/results/first-party-uav.result.json
COPY anonymous-external.result.json /veoveo/simulation/results/anonymous-external.result.json
"#;

pub(crate) fn publish(
    invocation_repository: &RepositoryContext,
    source_root: &Path,
    revision: &str,
    args: &ReleaseSimulationRuntimeArgs,
    output: &Path,
) -> Result<()> {
    validate_registry(&args.registry)?;
    let source_revision = SourceRevision::new(revision)?;
    let version = ReleaseVersion::new(&args.version)?;
    let build_lock_path =
        source_root.join("platform/runtimes/simulation/simulation-runtime.lock.json");
    let build_lock_bytes = fs::read(&build_lock_path)
        .with_context(|| format!("reading simulation lock {}", build_lock_path.display()))?;
    let build_lock: SimulationRuntimeBuildLock =
        serde_json::from_slice(&build_lock_bytes).context("decoding simulation runtime lock")?;
    build_lock.validate()?;
    let build_lock_digest = ArtifactDigest::new(format!(
        "sha256:{}",
        hex::encode(Sha256::digest(&build_lock_bytes))
    ))?;

    let first_party_path = resolve_input(invocation_repository.root(), &args.first_party_result);
    let anonymous_path = resolve_input(invocation_repository.root(), &args.anonymous_result);
    let first_party: SimulationConformanceResult = read_json(&first_party_path)?;
    let anonymous: SimulationConformanceResult = read_json(&anonymous_path)?;
    validate_result(
        &first_party,
        SimulationOverlayKind::FirstPartyUav,
        &source_revision,
        &build_lock,
        &build_lock_digest,
    )?;
    validate_result(
        &anonymous,
        SimulationOverlayKind::AnonymousExternal,
        &source_revision,
        &build_lock,
        &build_lock_digest,
    )?;
    ensure!(
        first_party.base_image == anonymous.base_image
            && first_party.base_digest == anonymous.base_digest,
        "simulation results do not certify the same canonical base image"
    );

    let context = tempfile::tempdir().context("creating simulation evidence build context")?;
    fs::write(context.path().join("Dockerfile"), DOCKERFILE)?;
    fs::write(
        context.path().join("simulation-runtime.lock.json"),
        &build_lock_bytes,
    )?;
    write_json(
        &context
            .path()
            .join("simulation-runtime-build-lock.schema.json"),
        &simulation_runtime_build_lock_schema(),
    )?;
    write_json(
        &context
            .path()
            .join("simulation-conformance-result.schema.json"),
        &simulation_conformance_result_schema(),
    )?;
    write_json(
        &context.path().join("first-party-uav.result.json"),
        &first_party,
    )?;
    write_json(
        &context.path().join("anonymous-external.result.json"),
        &anonymous,
    )?;

    let repository = format!(
        "{}/{}",
        args.registry.trim_end_matches('/'),
        CONFORMANCE_REPOSITORY
    );
    let tag = format!("{repository}:{revision}");
    let status = builder::buildx_command(invocation_repository)?
        .current_dir(source_root)
        .args([
            "build",
            "--builder",
            builder::BUILDER_NAME,
            "--platform",
            "linux/amd64",
            "--build-arg",
            &format!("SOURCE_REVISION={revision}"),
            "--attest",
            "type=provenance,mode=max",
            "--attest",
            "type=sbom",
            "--output",
            "type=registry,rewrite-timestamp=true",
            "--tag",
            &tag,
        ])
        .arg(context.path())
        .env("SOURCE_DATE_EPOCH", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("publishing simulation conformance OCI bundle")?;
    ensure!(
        status.success(),
        "simulation conformance OCI publication failed with {status}"
    );
    let conformance_digest = inspect_manifest_digest(invocation_repository, &tag)?;
    let conformance_digest = ArtifactDigest::new(conformance_digest)?;

    let evidence = SimulationRuntimeReleaseEvidence {
        schema_version: SimulationRuntimeReleaseEvidenceSchema::V1,
        source_revision,
        profile: build_lock.profile.clone(),
        base_image: ArtifactDescriptor {
            name: ArtifactName::new("simulation-runtime")?,
            kind: ArtifactKind::OciImage,
            version: version.clone(),
            coordinate: first_party.base_image.clone(),
            digest: first_party.base_digest.clone(),
            platform: Some(ArtifactPlatform {
                operating_system: OperatingSystem::Linux,
                architecture: CpuArchitecture::Amd64,
            }),
            media_type: Some(OCI_INDEX_MEDIA_TYPE.to_owned()),
        },
        components: build_lock.components.clone(),
        gpu: build_lock.gpu.runtime.clone(),
        conformance_result: ArtifactDescriptor {
            name: ArtifactName::new(CONFORMANCE_REPOSITORY)?,
            kind: ArtifactKind::ConformanceResult,
            version,
            coordinate: ArtifactCoordinate::new(format!(
                "oci://{repository}@{conformance_digest}"
            ))?,
            digest: conformance_digest,
            platform: Some(ArtifactPlatform {
                operating_system: OperatingSystem::Linux,
                architecture: CpuArchitecture::Amd64,
            }),
            media_type: Some(OCI_INDEX_MEDIA_TYPE.to_owned()),
        },
        results: vec![first_party, anonymous],
    };
    evidence.validate()?;

    fs::create_dir_all(output)
        .with_context(|| format!("creating simulation release output {}", output.display()))?;
    write_json(&output.join("release-evidence.json"), &evidence)?;
    write_json(
        &output.join("simulation-runtime-release-evidence.schema.json"),
        &veoveo_extension_contract::simulation_runtime_release_evidence_schema(),
    )?;
    fs::write(
        output.join("simulation-runtime.lock.json"),
        build_lock_bytes,
    )?;
    println!(
        "Published simulation conformance bundle: oci://{repository}@{}",
        evidence.conformance_result.digest
    );
    Ok(())
}

fn validate_result(
    result: &SimulationConformanceResult,
    kind: SimulationOverlayKind,
    revision: &SourceRevision,
    lock: &SimulationRuntimeBuildLock,
    lock_digest: &ArtifactDigest,
) -> Result<()> {
    result.validate()?;
    ensure!(
        result.overlay_kind == kind,
        "simulation result has {:?} overlay kind; expected {kind:?}",
        result.overlay_kind
    );
    ensure!(
        &result.source_revision == revision
            && result.profile == lock.profile
            && result.build_lock_digest == *lock_digest
            && result.components == lock.components,
        "simulation result does not match the selected revision and embedded runtime lock"
    );
    Ok(())
}

fn inspect_manifest_digest(repository: &RepositoryContext, reference: &str) -> Result<String> {
    let output = builder::buildx_command(repository)?
        .args([
            "imagetools",
            "inspect",
            "--format",
            "{{json .Manifest.Digest}}",
            reference,
        ])
        .output()
        .with_context(|| format!("inspecting simulation evidence manifest {reference}"))?;
    ensure!(
        output.status.success(),
        "inspecting simulation evidence manifest failed with {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice::<String>(&output.stdout)
        .with_context(|| format!("decoding simulation evidence digest for {reference}"))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("decoding {}", path.display()))
}

fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let parent = path
        .parent()
        .context("simulation release output has no parent directory")?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temporary, value)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("publishing {}", path.display()))?;
    Ok(())
}

fn resolve_input(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn validate_registry(registry: &str) -> Result<()> {
    ensure!(
        !registry.trim().is_empty()
            && !registry.contains("://")
            && !registry.ends_with('/')
            && !registry.chars().any(char::is_whitespace),
        "registry must be a non-empty OCI host/prefix without a scheme, trailing slash, or whitespace"
    );
    Ok(())
}
