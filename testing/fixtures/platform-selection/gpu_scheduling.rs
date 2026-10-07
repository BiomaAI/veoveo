// Wiring-only fixture for the real deployment selection and chart adapters.
// No allocator is installed and this synthetic topology is not hardware evidence.
use std::collections::BTreeMap;
use veoveo_deploy_contract::{
    ComputerCapacity, ConflictingGpuDevicePluginRemoval, GpuAllocatorMaturityAcceptance,
    GpuDynamicResourceAllocator, GpuIsolation, GpuSamePhysicalDeviceGroup, GpuSchedulingProfile,
    GpuWorkloadPlacement, InstallationPreset, ManagedGpuAllocatorInstallation, ManagedOciChart,
    ManagedOciImage, NVIDIA_DRA_CHART_CONTENT_DIGEST, NVIDIA_DRA_CHART_COORDINATE,
    NVIDIA_DRA_CHART_DIGEST, NVIDIA_DRA_DRIVER_NAME, NVIDIA_DRA_IMAGE_AMD64_DIGEST,
    NVIDIA_DRA_IMAGE_ARM64_DIGEST, NVIDIA_DRA_IMAGE_DIGEST, NVIDIA_DRA_IMAGE_REPOSITORY,
    NVIDIA_DRA_VERSION, PlatformSelection,
};

pub fn selection(preset: InstallationPreset) -> PlatformSelection {
    PlatformSelection {
        computer_capacity: ComputerCapacity::Unconfigured,
        installation_preset: preset,
        components: Default::default(),
        mcp_servers: Default::default(),
        artifact_audiences: Default::default(),
        workloads: Default::default(),
        gpu_scheduling: (preset == InstallationPreset::Full).then(full_scheduling),
    }
}

fn full_scheduling() -> GpuSchedulingProfile {
    let placements = [
        ("embedding", "embedding", "embedding"),
        ("view-renderer", "view-mcp", "view-mcp"),
        ("stream", "stream", "stream"),
        ("reason", "reason", "reason"),
        ("speech", "speech", "speech"),
        ("rerun-bridge", "rerun-bridge", "rerun-bridge"),
        ("cuopt-executor", "optimization-mcp", "cuopt-executor"),
    ];
    GpuSchedulingProfile {
        runtime_class_name: "nvidia".to_owned(),
        allocatable_devices: 7,
        evidence_digest: format!("sha256:{}", "1".repeat(64)),
        allocator: GpuDynamicResourceAllocator {
            claim_name: "chart-test-gpu-placement".to_owned(),
            full_device_class_name: "gpu.nvidia.com".to_owned(),
            mig_device_class_name: "mig.nvidia.com".to_owned(),
            driver_name: NVIDIA_DRA_DRIVER_NAME.to_owned(),
            configuration_api_version: "resource.nvidia.com/v1beta1".to_owned(),
            installation: ManagedGpuAllocatorInstallation {
                release_name: "dra-driver-nvidia-gpu".to_owned(),
                namespace: "nvidia-dra-driver-gpu".to_owned(),
                chart: ManagedOciChart {
                    coordinate: NVIDIA_DRA_CHART_COORDINATE.to_owned(),
                    version: NVIDIA_DRA_VERSION.to_owned(),
                    digest: NVIDIA_DRA_CHART_DIGEST.to_owned(),
                    content_digest: NVIDIA_DRA_CHART_CONTENT_DIGEST.to_owned(),
                },
                image: ManagedOciImage {
                    repository: NVIDIA_DRA_IMAGE_REPOSITORY.to_owned(),
                    tag: format!("v{NVIDIA_DRA_VERSION}"),
                    digest: NVIDIA_DRA_IMAGE_DIGEST.to_owned(),
                    platform_digests: BTreeMap::from([
                        (
                            "linux/amd64".to_owned(),
                            NVIDIA_DRA_IMAGE_AMD64_DIGEST.to_owned(),
                        ),
                        (
                            "linux/arm64".to_owned(),
                            NVIDIA_DRA_IMAGE_ARM64_DIGEST.to_owned(),
                        ),
                    ]),
                },
                nvidia_driver_root: "/".to_owned(),
                eligible_node_selector: BTreeMap::from([(
                    "kubernetes.io/hostname".to_owned(),
                    "chart-test-gpu-node".to_owned(),
                )]),
                conflicting_device_plugin_removal: ConflictingGpuDevicePluginRemoval::RequireAbsent,
                maturity_acceptance: GpuAllocatorMaturityAcceptance::TechnologyPreview,
                timeout_seconds: 300,
            },
        },
        same_physical_device_groups: placements
            .into_iter()
            .map(
                |(workload, deployment, container)| GpuSamePhysicalDeviceGroup {
                    name: workload.to_owned(),
                    workloads: vec![GpuWorkloadPlacement {
                        workload: workload.to_owned(),
                        deployment: deployment.to_owned(),
                        container: container.to_owned(),
                        replicas: 1,
                    }],
                    isolation: GpuIsolation::Exclusive,
                    maximum_consumers: 1,
                    evidence_digest: None,
                    mig_profile: None,
                    time_slice_interval: None,
                },
            )
            .collect(),
        different_physical_device_groups: Vec::new(),
    }
}
