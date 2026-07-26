use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_deploy_contract::{PROFILE_SCHEMA, deployment_lock_schema, deployment_profile_schema};
use veoveo_extension_contract::{
    ArtifactCoordinate, ArtifactDescriptor, ArtifactDigest, ArtifactKind, ArtifactName,
    CompatibilityManifest, CompatibilityManifestSchema, CompatibilityReleaseId,
    ContractCompatibility, ContractKind, EXTENSION_HELM_LIBRARY_API, EXTENSION_RELEASE_SCHEMA,
    ReleaseVersion, SdkCompatibility, SdkLanguage, SimulationRuntimeCompatibility,
    SimulationRuntimeReleaseEvidence, VersionRequirement, compatibility_manifest_schema,
    extension_release_schema, simulation_conformance_result_schema,
    simulation_runtime_build_lock_schema, simulation_runtime_release_evidence_schema,
};
use veoveo_mcp_conformance::{
    HOSTED_SERVER_PROFILE_SCHEMA, conformance_report_schema,
    hosted_server_conformance_profile_schema,
};
use veoveo_mcp_contract::{
    GATEWAY_BINDING_SCHEMA, GATEWAY_SERVER_FRAGMENT_SCHEMA, HOSTED_MCP_CONTRACT_REVISION,
    gateway_binding_schema, gateway_composition_provenance_schema, gateway_server_fragment_schema,
};

use crate::ReleaseCompatibilityArgs;

const PYTHON_EVIDENCE_SCHEMA: &str = "veoveo.io/python-sdk-release-evidence/v1";
const HELM_EVIDENCE_SCHEMA: &str = "veoveo.io/helm-chart-release-evidence/v1";
const IMAGE_EVIDENCE_SCHEMA: &str = "veoveo.io/image-release-evidence/v1";
const RELEASE_EVIDENCE_SCHEMA: &str = "veoveo.io/compatibility-release-evidence/v1";
const PYTHON_RUNTIME: &str = ">=3.13,<3.14";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PythonEvidence {
    schema_version: String,
    package: String,
    version: String,
    source_revision: String,
    artifacts: Vec<PythonArtifact>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PythonArtifact {
    filename: String,
    sha256: String,
    media_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HelmEvidence {
    schema_version: String,
    version: String,
    source_revision: String,
    helm_version: String,
    artifacts: Vec<HelmArtifact>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HelmArtifact {
    name: String,
    filename: String,
    sha256: String,
    media_type: String,
    oci: Option<OciPublication>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OciPublication {
    coordinate: String,
    digest: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImageEvidence {
    schema_version: String,
    source_revision: String,
    registry: String,
    images: Vec<ImageArtifact>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImageArtifact {
    name: String,
    repository: String,
    digest: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompatibilityReleaseEvidence {
    schema_version: &'static str,
    source_revision: String,
    manifest_sha256: String,
    schema_sha256: BTreeMap<String, String>,
    input_sha256: BTreeMap<String, String>,
}

pub(crate) fn generate(
    invocation_root: &Path,
    source_root: &Path,
    revision: &str,
    args: &ReleaseCompatibilityArgs,
    output: &Path,
) -> Result<()> {
    let python_path = resolve_input(invocation_root, &args.python_evidence);
    let helm_path = resolve_input(invocation_root, &args.helm_evidence);
    let image_path = resolve_input(invocation_root, &args.image_evidence);
    let simulation_path = args
        .simulation_evidence
        .as_ref()
        .map(|path| resolve_input(invocation_root, path));
    let python = read_json::<PythonEvidence>(&python_path)?;
    let helm = read_json::<HelmEvidence>(&helm_path)?;
    let images = read_json::<ImageEvidence>(&image_path)?;
    let simulation = simulation_path
        .as_ref()
        .map(|path| read_json::<SimulationRuntimeReleaseEvidence>(path))
        .transpose()?;
    validate_evidence(revision, &python, &helm, &images, simulation.as_ref())?;

    let release = CompatibilityReleaseId::new(&args.release)?;
    let platform_version = ReleaseVersion::new(&args.platform_version)?;
    let sdk_version = ReleaseVersion::new(&python.version)?;
    let helm_version = ReleaseVersion::new(&helm.version)?;
    let python_base = args.python_artifact_base.trim_end_matches('/');
    let _ = ArtifactCoordinate::new(format!("{python_base}/probe"))?;

    let sdks = python
        .artifacts
        .iter()
        .map(|artifact| {
            let kind = if artifact.filename.ends_with(".whl") {
                ArtifactKind::PythonWheel
            } else if artifact.filename.ends_with(".tar.gz") {
                ArtifactKind::PythonSdist
            } else {
                anyhow::bail!(
                    "Python SDK evidence contains unsupported artifact {}",
                    artifact.filename
                );
            };
            Ok(SdkCompatibility {
                language: SdkLanguage::Python,
                runtime: VersionRequirement::new(PYTHON_RUNTIME)?,
                artifact: ArtifactDescriptor {
                    name: ArtifactName::new(&python.package)?,
                    kind,
                    version: sdk_version.clone(),
                    coordinate: ArtifactCoordinate::new(format!(
                        "{python_base}/{}",
                        artifact.filename
                    ))?,
                    digest: ArtifactDigest::new(&artifact.sha256)?,
                    platform: None,
                    media_type: Some(artifact.media_type.clone()),
                },
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let helm_library = helm
        .artifacts
        .iter()
        .find(|artifact| artifact.name == "veoveo-extension")
        .context("Helm evidence does not contain veoveo-extension")?;
    let helm_oci = helm_library
        .oci
        .as_ref()
        .context("veoveo-extension Helm evidence has no OCI publication")?;
    let conformance = image_descriptor(
        &images,
        "mcp-conformance",
        "veoveo-mcp-conformance",
        ArtifactKind::ConformanceImage,
        &platform_version,
    )?;
    let gateway_composer = image_descriptor(
        &images,
        "gateway-composer",
        "veoveo-gateway-composer",
        ArtifactKind::OciImage,
        &platform_version,
    )?;
    let manifest = CompatibilityManifest {
        schema_version: CompatibilityManifestSchema::V1,
        release,
        platform_version,
        contracts: vec![
            contract(ContractKind::McpServer, HOSTED_MCP_CONTRACT_REVISION),
            contract(
                ContractKind::GatewayServerFragment,
                GATEWAY_SERVER_FRAGMENT_SCHEMA,
            ),
            contract(ContractKind::GatewayBinding, GATEWAY_BINDING_SCHEMA),
            contract(ContractKind::Deployment, PROFILE_SCHEMA),
            contract(ContractKind::ExtensionRelease, EXTENSION_RELEASE_SCHEMA),
            contract(
                ContractKind::ExtensionHelmLibrary,
                EXTENSION_HELM_LIBRARY_API,
            ),
            contract(ContractKind::Conformance, HOSTED_SERVER_PROFILE_SCHEMA),
        ],
        sdks,
        conformance,
        gateway_composer,
        helm_library: ArtifactDescriptor {
            name: ArtifactName::new("veoveo-extension")?,
            kind: ArtifactKind::HelmChart,
            version: helm_version,
            coordinate: ArtifactCoordinate::new(&helm_oci.coordinate)?,
            digest: ArtifactDigest::new(&helm_oci.digest)?,
            platform: None,
            media_type: Some(helm_library.media_type.clone()),
        },
        simulation_runtimes: simulation
            .as_ref()
            .map(simulation_compatibility)
            .transpose()?
            .into_iter()
            .collect(),
    };
    manifest.validate()?;

    fs::create_dir_all(output)
        .with_context(|| format!("creating compatibility output {}", output.display()))?;
    let manifest_path = output.join("compatibility-manifest.json");
    write_json_immutable(&manifest_path, &manifest)?;

    let schema_directory = output.join("schemas");
    fs::create_dir_all(&schema_directory)?;
    let schemas = BTreeMap::from([
        (
            "compatibility-manifest.schema.json",
            serde_json::to_value(compatibility_manifest_schema())?,
        ),
        (
            "extension-release.schema.json",
            serde_json::to_value(extension_release_schema())?,
        ),
        (
            "deployment-profile.schema.json",
            serde_json::to_value(deployment_profile_schema())?,
        ),
        (
            "deployment-lock.schema.json",
            serde_json::to_value(deployment_lock_schema())?,
        ),
        (
            "gateway-server-fragment.schema.json",
            serde_json::to_value(gateway_server_fragment_schema())?,
        ),
        (
            "gateway-binding.schema.json",
            serde_json::to_value(gateway_binding_schema())?,
        ),
        (
            "gateway-composition-provenance.schema.json",
            serde_json::to_value(gateway_composition_provenance_schema())?,
        ),
        (
            "mcp-conformance-profile.schema.json",
            serde_json::to_value(hosted_server_conformance_profile_schema())?,
        ),
        (
            "mcp-conformance-report.schema.json",
            serde_json::to_value(conformance_report_schema())?,
        ),
        (
            "simulation-runtime-build-lock.schema.json",
            serde_json::to_value(simulation_runtime_build_lock_schema())?,
        ),
        (
            "simulation-conformance-result.schema.json",
            serde_json::to_value(simulation_conformance_result_schema())?,
        ),
        (
            "simulation-runtime-release-evidence.schema.json",
            serde_json::to_value(simulation_runtime_release_evidence_schema())?,
        ),
    ]);
    let mut schema_sha256 = BTreeMap::new();
    for (name, schema) in schemas {
        let path = schema_directory.join(name);
        write_json_immutable(&path, &schema)?;
        schema_sha256.insert(name.to_owned(), sha256_file(&path)?);
    }
    let evidence = CompatibilityReleaseEvidence {
        schema_version: RELEASE_EVIDENCE_SCHEMA,
        source_revision: revision.to_owned(),
        manifest_sha256: sha256_file(&manifest_path)?,
        schema_sha256,
        input_sha256: BTreeMap::from_iter(
            [
                ("helm".to_owned(), sha256_file(&helm_path)?),
                ("images".to_owned(), sha256_file(&image_path)?),
                ("python".to_owned(), sha256_file(&python_path)?),
            ]
            .into_iter()
            .chain(
                simulation_path
                    .as_ref()
                    .map(|path| Ok(("simulation".to_owned(), sha256_file(path)?)))
                    .into_iter()
                    .collect::<Result<Vec<_>>>()?,
            ),
        ),
    };
    write_json_immutable(&output.join("release-evidence.json"), &evidence)?;

    let source_manifest = source_root.join("Cargo.toml");
    ensure!(
        source_manifest.is_file(),
        "compatibility source revision has no workspace Cargo.toml"
    );
    Ok(())
}

fn validate_evidence(
    revision: &str,
    python: &PythonEvidence,
    helm: &HelmEvidence,
    images: &ImageEvidence,
    simulation: Option<&SimulationRuntimeReleaseEvidence>,
) -> Result<()> {
    ensure!(
        python.schema_version == PYTHON_EVIDENCE_SCHEMA,
        "Python evidence schemaVersion must be {PYTHON_EVIDENCE_SCHEMA}"
    );
    ensure!(
        helm.schema_version == HELM_EVIDENCE_SCHEMA,
        "Helm evidence schemaVersion must be {HELM_EVIDENCE_SCHEMA}"
    );
    ensure!(
        images.schema_version == IMAGE_EVIDENCE_SCHEMA,
        "image evidence schemaVersion must be {IMAGE_EVIDENCE_SCHEMA}"
    );
    for (kind, evidence_revision) in [
        ("Python", python.source_revision.as_str()),
        ("Helm", helm.source_revision.as_str()),
        ("image", images.source_revision.as_str()),
    ] {
        ensure!(
            evidence_revision == revision,
            "{kind} evidence revision {evidence_revision} differs from compatibility revision {revision}"
        );
    }
    ensure!(python.package == "veoveo-mcp", "unexpected Python package");
    ensure!(
        python.artifacts.len() == 2,
        "Python evidence must contain a wheel and sdist"
    );
    ensure!(
        !helm.helm_version.trim().is_empty(),
        "Helm evidence has no Helm version"
    );
    ensure!(
        !images.registry.trim().is_empty(),
        "image evidence has no registry"
    );
    let names = images
        .images
        .iter()
        .map(|image| image.name.as_str())
        .collect::<BTreeSet<_>>();
    ensure!(
        names.contains("mcp-conformance") && names.contains("gateway-composer"),
        "image evidence must contain mcp-conformance and gateway-composer"
    );
    for artifact in &helm.artifacts {
        let _ = ArtifactDigest::new(&artifact.sha256)?;
        ensure!(!artifact.filename.trim().is_empty(), "empty Helm filename");
    }
    if let Some(simulation) = simulation {
        simulation.validate()?;
        ensure!(
            simulation.source_revision.as_str() == revision,
            "simulation evidence revision differs from compatibility revision {revision}"
        );
    }
    Ok(())
}

fn simulation_compatibility(
    evidence: &SimulationRuntimeReleaseEvidence,
) -> Result<SimulationRuntimeCompatibility> {
    Ok(SimulationRuntimeCompatibility {
        profile: evidence.profile.clone(),
        base_image: evidence.base_image.clone(),
        components: evidence.components.clone(),
        gpu: evidence.gpu.clone(),
        conformance_result: evidence.conformance_result.clone(),
    })
}

fn image_descriptor(
    evidence: &ImageEvidence,
    target: &str,
    name: &str,
    kind: ArtifactKind,
    version: &ReleaseVersion,
) -> Result<ArtifactDescriptor> {
    let image = evidence
        .images
        .iter()
        .find(|image| image.name == target)
        .with_context(|| format!("image evidence is missing {target}"))?;
    let digest = ArtifactDigest::new(&image.digest)?;
    Ok(ArtifactDescriptor {
        name: ArtifactName::new(name)?,
        kind,
        version: version.clone(),
        coordinate: ArtifactCoordinate::new(format!("oci://{}@{}", image.repository, digest))?,
        digest,
        platform: None,
        media_type: Some("application/vnd.oci.image.manifest.v1+json".to_owned()),
    })
}

fn contract(kind: ContractKind, version: &str) -> ContractCompatibility {
    ContractCompatibility {
        kind,
        version: version.to_owned(),
    }
}

fn resolve_input(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    serde_json::from_slice(&fs::read(path).with_context(|| format!("reading {}", path.display()))?)
        .with_context(|| format!("decoding {}", path.display()))
}

fn write_json_immutable(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    if path.exists() {
        ensure!(
            fs::read(path)? == bytes,
            "immutable compatibility artifact {} already exists with different content",
            path.display()
        );
        return Ok(());
    }
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use serde_json::json;
    use tempfile::TempDir;

    use super::generate;
    use crate::ReleaseCompatibilityArgs;

    #[test]
    fn generates_manifest_and_schema_bundle_from_release_evidence() {
        let workspace = TempDir::new().expect("temporary workspace");
        let revision = "a".repeat(40);
        let digest = |byte: char| format!("sha256:{}", byte.to_string().repeat(64));
        let python = workspace.path().join("python.json");
        let helm = workspace.path().join("helm.json");
        let images = workspace.path().join("images.json");
        fs::write(
            &python,
            serde_json::to_vec(&json!({
                "schemaVersion": "veoveo.io/python-sdk-release-evidence/v1",
                "package": "veoveo-mcp",
                "version": "0.1.0",
                "sourceRevision": revision,
                "artifacts": [
                    {
                        "filename": "veoveo_mcp-0.1.0-py3-none-any.whl",
                        "sha256": digest('1'),
                        "mediaType": "application/vnd.pypi.wheel"
                    },
                    {
                        "filename": "veoveo_mcp-0.1.0.tar.gz",
                        "sha256": digest('2'),
                        "mediaType": "application/vnd.pypi.sdist"
                    }
                ]
            }))
            .expect("Python evidence"),
        )
        .expect("write Python evidence");
        fs::write(
            &helm,
            serde_json::to_vec(&json!({
                "schemaVersion": "veoveo.io/helm-chart-release-evidence/v1",
                "version": "0.1.0",
                "sourceRevision": revision,
                "helmVersion": "v4.0.4",
                "artifacts": [{
                    "name": "veoveo-extension",
                    "filename": "veoveo-extension-0.1.0.tgz",
                    "sha256": digest('3'),
                    "mediaType": "application/vnd.cncf.helm.chart.content.v1.tar+gzip",
                    "oci": {
                        "coordinate": "oci://registry.internal/charts/veoveo-extension:0.1.0",
                        "digest": digest('4')
                    }
                }]
            }))
            .expect("Helm evidence"),
        )
        .expect("write Helm evidence");
        fs::write(
            &images,
            serde_json::to_vec(&json!({
                "schemaVersion": "veoveo.io/image-release-evidence/v1",
                "sourceRevision": revision,
                "registry": "registry.internal",
                "images": [
                    {
                        "name": "mcp-conformance",
                        "repository": "registry.internal/veoveo/mcp-conformance",
                        "digest": digest('5')
                    },
                    {
                        "name": "gateway-composer",
                        "repository": "registry.internal/veoveo/gateway-composer",
                        "digest": digest('6')
                    }
                ]
            }))
            .expect("image evidence"),
        )
        .expect("write image evidence");
        let output = workspace.path().join("output");
        let args = ReleaseCompatibilityArgs {
            revision: revision.clone(),
            release: "0.1.0".to_owned(),
            platform_version: "0.1.0".to_owned(),
            python_evidence: PathBuf::from("python.json"),
            python_artifact_base: "python://packages.internal/veoveo".to_owned(),
            helm_evidence: PathBuf::from("helm.json"),
            image_evidence: PathBuf::from("images.json"),
            simulation_evidence: None,
            output_dir: PathBuf::from("unused"),
        };
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        generate(workspace.path(), &source, &revision, &args, &output)
            .expect("generate compatibility release");
        let manifest: serde_json::Value = serde_json::from_slice(
            &fs::read(output.join("compatibility-manifest.json")).expect("read manifest"),
        )
        .expect("decode manifest");
        assert_eq!(manifest["gatewayComposer"]["digest"], json!(digest('6')));
        assert!(output.join("schemas/deployment-lock.schema.json").is_file());
        assert!(output.join("release-evidence.json").is_file());
    }
}
