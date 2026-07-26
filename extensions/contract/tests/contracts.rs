use std::collections::BTreeSet;

use jsonschema::Validator;
use serde_json::json;
use veoveo_extension_contract::{
    ArtifactCoordinate, ArtifactDescriptor, ArtifactDigest, ArtifactKind, ArtifactName,
    CompatibilityManifest, CompatibilityManifestSchema, CompatibilityReleaseId,
    ContractCompatibility, ContractKind, ExtensionContractError, ExtensionId,
    ExtensionReleaseManifest, ExtensionReleaseSchema, ExtensionSource, GpuRuntimeRequirement,
    NvidiaDriverCapability, PythonDistributionInput, ReleaseVersion, RuntimeComponent,
    RuntimeComponentVersion, SdkCompatibility, SdkLanguage, SimulationAttestationEvidence,
    SimulationConformanceResult, SimulationConformanceResultSchema, SimulationGpuRequirement,
    SimulationHardwareEvidence, SimulationOverlayKind, SimulationProbeKind, SimulationProbeResult,
    SimulationRuntimeBuildLock, SimulationRuntimeBuildLockSchema, SimulationRuntimeReleaseEvidence,
    SimulationRuntimeReleaseEvidenceSchema, SimulationSourceInput, SourceRevision,
    VersionRequirement, compatibility_manifest_schema, extension_release_schema,
    simulation_conformance_result_schema, simulation_runtime_build_lock_schema,
    simulation_runtime_release_evidence_schema,
};

fn digest(byte: char) -> ArtifactDigest {
    ArtifactDigest::new(format!("sha256:{}", byte.to_string().repeat(64))).expect("valid digest")
}

fn artifact(name: &str, kind: ArtifactKind, byte: char) -> ArtifactDescriptor {
    ArtifactDescriptor {
        name: ArtifactName::new(name).expect("artifact name"),
        kind,
        version: ReleaseVersion::new("1.0.0").expect("version"),
        coordinate: ArtifactCoordinate::new(format!("oci://registry.example/{name}:1.0.0"))
            .expect("coordinate"),
        digest: digest(byte),
        platform: None,
        media_type: None,
    }
}

fn manifest() -> CompatibilityManifest {
    CompatibilityManifest {
        schema_version: CompatibilityManifestSchema::V1,
        release: CompatibilityReleaseId::new("1.0.0").expect("release"),
        platform_version: ReleaseVersion::new("1.0.0").expect("platform version"),
        contracts: vec![ContractCompatibility {
            kind: ContractKind::McpServer,
            version: "2".to_owned(),
        }],
        sdks: vec![SdkCompatibility {
            language: SdkLanguage::Python,
            runtime: VersionRequirement::new(">=3.13,<3.14").expect("runtime"),
            artifact: artifact("veoveo-mcp", ArtifactKind::PythonWheel, 'a'),
        }],
        conformance: artifact(
            "veoveo-mcp-conformance",
            ArtifactKind::ConformanceImage,
            'b',
        ),
        gateway_composer: artifact("veoveo-gateway-composer", ArtifactKind::OciImage, 'd'),
        helm_library: artifact("veoveo-extension", ArtifactKind::HelmChart, 'c'),
        simulation_runtimes: vec![],
    }
}

#[test]
fn compatibility_manifest_round_trips_through_generated_schema() {
    let manifest = manifest();
    manifest.validate().expect("manifest validation");
    let value = serde_json::to_value(&manifest).expect("serialize manifest");
    let schema = serde_json::to_value(compatibility_manifest_schema()).expect("serialize schema");
    let validator = Validator::new(&schema).expect("compile schema");
    assert!(validator.is_valid(&value));
}

#[test]
fn compatibility_manifest_rejects_duplicate_contract_kinds() {
    let mut manifest = manifest();
    manifest.contracts.push(manifest.contracts[0].clone());
    assert!(matches!(
        manifest.validate(),
        Err(ExtensionContractError::Duplicate {
            kind: "contract kind",
            ..
        })
    ));
}

#[test]
fn controlled_identifiers_reject_mutable_or_ambiguous_values() {
    assert!(ArtifactDigest::new("sha256:abc").is_err());
    assert!(ArtifactCoordinate::new("oci://registry.example/image:latest").is_err());
    assert!(ReleaseVersion::new("release-one").is_err());
}

#[test]
fn generated_schema_rejects_unknown_fields() {
    let schema = serde_json::to_value(compatibility_manifest_schema()).expect("serialize schema");
    let validator = Validator::new(&schema).expect("compile schema");
    let mut value = serde_json::to_value(manifest()).expect("serialize manifest");
    value
        .as_object_mut()
        .expect("manifest object")
        .insert("unknown".to_owned(), json!(true));
    assert!(!validator.is_valid(&value));
}

#[test]
fn generated_schema_carries_identifier_constraints() {
    let schema = serde_json::to_value(compatibility_manifest_schema()).expect("serialize schema");
    let validator = Validator::new(&schema).expect("compile schema");
    let mut value = serde_json::to_value(manifest()).expect("serialize manifest");
    value["platformVersion"] = json!("release-one");
    value["conformance"]["digest"] = json!("sha256:abc");
    value["helmLibrary"]["coordinate"] = json!("oci://registry.example/chart:latest");
    assert!(!validator.is_valid(&value));
}

#[test]
fn extension_release_round_trips_through_generated_schema() {
    let manifest = ExtensionReleaseManifest {
        schema_version: ExtensionReleaseSchema::V1,
        extension: ExtensionId::new("example-extension").expect("extension id"),
        version: ReleaseVersion::new("1.2.3").expect("version"),
        source: ExtensionSource {
            name: ArtifactName::new("example-source").expect("source name"),
            revision: SourceRevision::new("d".repeat(40)).expect("source revision"),
        },
        compatibility_release: CompatibilityReleaseId::new("1.0.0").expect("compatibility release"),
        artifacts: vec![artifact("example-image", ArtifactKind::OciImage, 'd')],
        helm_chart: artifact("example-chart", ArtifactKind::HelmChart, 'e'),
        gateway_fragment: artifact("example-fragment", ArtifactKind::GatewayServerFragment, 'f'),
        conformance_results: vec![artifact(
            "example-conformance",
            ArtifactKind::ConformanceResult,
            '1',
        )],
        simulation_overlay: None,
    };
    manifest.validate().expect("release validation");
    let value = serde_json::to_value(&manifest).expect("serialize release");
    let schema = serde_json::to_value(extension_release_schema()).expect("serialize schema");
    let validator = Validator::new(&schema).expect("compile schema");
    assert!(validator.is_valid(&value));
}

fn simulation_components() -> Vec<RuntimeComponentVersion> {
    [
        (RuntimeComponent::IsaacSim, "6.0.1", None),
        (
            RuntimeComponent::IsaacLab,
            "3.0.0-beta2.patch1",
            Some("f".repeat(40)),
        ),
        (RuntimeComponent::Warp, "1.15.0", None),
        (RuntimeComponent::Newton, "1.4.0", None),
        (RuntimeComponent::Mujoco, "3.10.0", None),
        (RuntimeComponent::MujocoWarp, "3.10.0.3", None),
        (RuntimeComponent::Python, "3.12.13", None),
        (RuntimeComponent::Cuda, "12.9", None),
        (RuntimeComponent::Kit, "110.1.2", None),
    ]
    .into_iter()
    .map(|(component, version, revision)| RuntimeComponentVersion {
        component,
        version: version.to_owned(),
        revision,
    })
    .collect()
}

#[test]
fn simulation_build_lock_requires_complete_immutable_tuple() {
    let components = simulation_components();
    let immutable = components
        .iter()
        .map(|component| component.component)
        .collect();
    let lock = SimulationRuntimeBuildLock {
        schema_version: SimulationRuntimeBuildLockSchema::V1,
        profile: ArtifactName::new("isaac-sim-6").expect("profile"),
        upstream_image: ArtifactCoordinate::new(format!(
            "oci://nvcr.io/nvidia/isaac-sim@{}",
            digest('a')
        ))
        .expect("image"),
        upstream_digest: digest('a'),
        components,
        sources: vec![SimulationSourceInput {
            component: RuntimeComponent::IsaacLab,
            repository: "https://github.com/isaac-sim/IsaacLab.git".to_owned(),
            tag: "v3.0.0-beta2.patch1".to_owned(),
            revision: SourceRevision::new("f".repeat(40)).expect("revision"),
            archive_digest: digest('1'),
            prerelease_reason: Some(
                "Isaac Lab has no stable Isaac Sim 6.0-compatible release".to_owned(),
            ),
        }],
        python_distributions: vec![PythonDistributionInput {
            package: ArtifactName::new("warp-lang").expect("package"),
            version: "1.15.0".to_owned(),
            filename: "warp_lang-1.15.0-py3-none-manylinux_2_28_x86_64.whl".to_owned(),
            digest: digest('b'),
        }],
        authoritative_package_roots: vec!["newton".to_owned(), "warp".to_owned()],
        overlay_immutable_components: immutable,
        gpu: SimulationGpuRequirement {
            runtime: GpuRuntimeRequirement {
                resource_name: "nvidia.com/gpu".to_owned(),
                count: 1,
                runtime_class_name: Some("nvidia".to_owned()),
                shared_memory_bytes: 2 * 1024 * 1024 * 1024,
            },
            minimum_driver_version: "580.173.02".to_owned(),
            driver_capabilities: BTreeSet::from([
                NvidiaDriverCapability::Compute,
                NvidiaDriverCapability::Graphics,
                NvidiaDriverCapability::Utility,
                NvidiaDriverCapability::Video,
            ]),
        },
    };
    lock.validate().expect("simulation build lock");
    let value = serde_json::to_value(&lock).expect("serialize lock");
    let schema =
        serde_json::to_value(simulation_runtime_build_lock_schema()).expect("serialize schema");
    assert!(Validator::new(&schema).expect("schema").is_valid(&value));
}

#[test]
fn simulation_result_rejects_incomplete_or_software_evidence() {
    let result = simulation_result(SimulationOverlayKind::AnonymousExternal);
    result.validate().expect("hardware simulation result");
    let value = serde_json::to_value(&result).expect("serialize result");
    let schema =
        serde_json::to_value(simulation_conformance_result_schema()).expect("serialize schema");
    assert!(Validator::new(&schema).expect("schema").is_valid(&value));

    let mut software = result;
    software.hardware.gpu_name = "llvmpipe".to_owned();
    assert!(software.validate().is_err());
}

#[test]
fn simulation_release_evidence_requires_paired_overlays() {
    let anonymous = simulation_result(SimulationOverlayKind::AnonymousExternal);
    let mut first_party = simulation_result(SimulationOverlayKind::FirstPartyUav);
    first_party.overlay_image = ArtifactCoordinate::new(format!(
        "oci://registry.example/veoveo/uav-sim-runtime@{}",
        digest('c')
    ))
    .expect("overlay image");
    first_party.overlay_digest = digest('c');
    let evidence = SimulationRuntimeReleaseEvidence {
        schema_version: SimulationRuntimeReleaseEvidenceSchema::V1,
        source_revision: anonymous.source_revision.clone(),
        profile: anonymous.profile.clone(),
        base_image: ArtifactDescriptor {
            name: ArtifactName::new("uav-sim-base").expect("name"),
            kind: ArtifactKind::OciImage,
            version: ReleaseVersion::new("1.0.0").expect("version"),
            coordinate: anonymous.base_image.clone(),
            digest: anonymous.base_digest.clone(),
            platform: None,
            media_type: None,
        },
        components: anonymous.components.clone(),
        gpu: GpuRuntimeRequirement {
            resource_name: "nvidia.com/gpu".to_owned(),
            count: 1,
            runtime_class_name: Some("nvidia".to_owned()),
            shared_memory_bytes: 2 * 1024 * 1024 * 1024,
        },
        conformance_result: artifact(
            "veoveo-simulation-conformance",
            ArtifactKind::ConformanceResult,
            'e',
        ),
        results: vec![first_party, anonymous],
    };
    evidence.validate().expect("simulation release evidence");
    let value = serde_json::to_value(&evidence).expect("serialize release evidence");
    let schema = serde_json::to_value(simulation_runtime_release_evidence_schema())
        .expect("serialize schema");
    assert!(Validator::new(&schema).expect("schema").is_valid(&value));

    let mut incomplete = evidence;
    incomplete.results.pop();
    assert!(incomplete.validate().is_err());
}

fn simulation_result(overlay_kind: SimulationOverlayKind) -> SimulationConformanceResult {
    SimulationConformanceResult {
        schema_version: SimulationConformanceResultSchema::V1,
        profile: ArtifactName::new("isaac-sim-6").expect("profile"),
        base_image: ArtifactCoordinate::new(format!(
            "oci://registry.example/veoveo/uav-sim-base@{}",
            digest('a')
        ))
        .expect("base image"),
        base_digest: digest('a'),
        overlay_kind,
        overlay_image: ArtifactCoordinate::new(format!(
            "oci://registry.example/example/simulation@{}",
            digest('b')
        ))
        .expect("overlay image"),
        overlay_digest: digest('b'),
        source_revision: SourceRevision::new("c".repeat(40)).expect("revision"),
        build_lock_digest: digest('d'),
        components: simulation_components(),
        hardware: SimulationHardwareEvidence {
            gpu_name: "NVIDIA GPU".to_owned(),
            driver_version: "580.173.02".to_owned(),
            cuda_device: "cuda:0".to_owned(),
            graphics_api: "Vulkan".to_owned(),
            renderer: "RaytracedLighting".to_owned(),
        },
        attestations: SimulationAttestationEvidence {
            sbom_digest: digest('e'),
            provenance_digest: digest('f'),
        },
        camera_count: 20,
        completed_at: "2026-07-26T20:00:00Z".to_owned(),
        probes: [
            SimulationProbeKind::ComponentTuple,
            SimulationProbeKind::ModuleGraph,
            SimulationProbeKind::NewtonTiledCamera,
            SimulationProbeKind::IndependentRtxCameras,
            SimulationProbeKind::OverlayBoundary,
        ]
        .into_iter()
        .map(|probe| SimulationProbeResult {
            probe,
            duration_milliseconds: 1,
        })
        .collect(),
    }
}
