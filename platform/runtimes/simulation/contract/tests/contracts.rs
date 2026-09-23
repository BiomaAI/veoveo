use jsonschema::Validator;
use std::collections::BTreeSet;
use veoveo_deploy_contract::*;
use veoveo_simulation_contract::*;

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

fn simulation_components() -> Vec<RuntimeComponentVersion> {
    [
        (RuntimeComponent::IsaacSim, "6.0.1", None),
        (
            RuntimeComponent::IsaacLab,
            "3.0.0-beta2.patch1",
            Some("f".repeat(40)),
        ),
        (RuntimeComponent::Warp, "1.16.0", None),
        (RuntimeComponent::Newton, "1.5.0", None),
        (RuntimeComponent::Mujoco, "3.11.0", None),
        (RuntimeComponent::MujocoWarp, "3.11.0", None),
        (RuntimeComponent::Python, "3.12.13", None),
        (RuntimeComponent::Torch, "2.12.0+cu130", None),
        (RuntimeComponent::Cuda, "13.0", None),
        (RuntimeComponent::IsaacRtxNvrtc, "12.8.61", None),
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
            version: "1.16.0".to_owned(),
            filename: "warp_lang-1.16.0-py3-none-manylinux_2_28_x86_64.whl".to_owned(),
            digest: digest('b'),
        }],
        authoritative_package_roots: vec![
            "newton".to_owned(),
            "torch".to_owned(),
            "warp".to_owned(),
        ],
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

    let mut static_body = simulation_result(SimulationOverlayKind::AnonymousExternal);
    static_body.newton_dynamics.final_height = static_body.newton_dynamics.initial_height;
    assert!(static_body.validate().is_err());
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
        schema_version: SimulationConformanceResultSchema::V2,
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
        newton_dynamics: SimulationNewtonDynamicsEvidence {
            device: "cuda:0".to_owned(),
            initial_height: 2.0,
            final_height: 2.08,
            final_vertical_velocity: 1.5,
            force_newtons: 25.0,
            steps: 12,
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
            SimulationProbeKind::NewtonDynamics,
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
