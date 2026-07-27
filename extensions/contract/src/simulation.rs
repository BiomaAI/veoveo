use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::compatibility::canonical_runtime_components;
use crate::{
    ArtifactCoordinate, ArtifactDescriptor, ArtifactDigest, ArtifactKind, ArtifactName,
    ExtensionContractError, GpuRuntimeRequirement, RuntimeComponent, RuntimeComponentVersion,
    SourceRevision,
};

/// Canonical simulation-runtime build-lock schema identifier.
pub const SIMULATION_RUNTIME_BUILD_LOCK_SCHEMA: &str = "veoveo.io/simulation-runtime-build-lock/v1";

/// Hardware simulation-conformance result schema identifier.
pub const SIMULATION_CONFORMANCE_RESULT_SCHEMA: &str = "veoveo.io/simulation-conformance-result/v1";

/// Simulation-runtime release-evidence schema identifier.
pub const SIMULATION_RUNTIME_RELEASE_EVIDENCE_SCHEMA: &str =
    "veoveo.io/simulation-runtime-release-evidence/v1";

/// Supported simulation-runtime build-lock schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SimulationRuntimeBuildLockSchema {
    /// Simulation-runtime build lock version 1.
    #[serde(rename = "veoveo.io/simulation-runtime-build-lock/v1")]
    V1,
}

/// Supported hardware simulation-conformance result schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SimulationConformanceResultSchema {
    /// Simulation hardware result version 1.
    #[serde(rename = "veoveo.io/simulation-conformance-result/v1")]
    V1,
}

/// Supported simulation-runtime release evidence schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum SimulationRuntimeReleaseEvidenceSchema {
    /// Simulation-runtime release evidence version 1.
    #[serde(rename = "veoveo.io/simulation-runtime-release-evidence/v1")]
    V1,
}

/// One exact source-delivered runtime component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationSourceInput {
    /// Runtime component supplied by this source.
    pub component: RuntimeComponent,
    /// HTTPS source repository.
    pub repository: String,
    /// Upstream release tag.
    pub tag: String,
    /// Immutable source revision.
    pub revision: SourceRevision,
    /// SHA-256 identity of the selected source archive.
    pub archive_digest: ArtifactDigest,
    /// Product reason for selecting a pre-release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prerelease_reason: Option<String>,
}

/// One hash-verified Python distribution used to replace a bundled runtime payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PythonDistributionInput {
    /// Python distribution name.
    pub package: ArtifactName,
    /// Exact distribution version.
    pub version: String,
    /// Selected wheel filename.
    pub filename: String,
    /// SHA-256 wheel identity.
    pub digest: ArtifactDigest,
}

/// NVIDIA driver and container runtime requirements for the simulation base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationGpuRequirement {
    /// Kubernetes GPU scheduling contract.
    pub runtime: GpuRuntimeRequirement,
    /// Lowest supported NVIDIA driver branch and patch identity.
    pub minimum_driver_version: String,
    /// Required NVIDIA container driver capabilities.
    pub driver_capabilities: BTreeSet<NvidiaDriverCapability>,
}

/// Required NVIDIA container driver capabilities.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum NvidiaDriverCapability {
    /// CUDA compute.
    Compute,
    /// Vulkan and RTX graphics.
    Graphics,
    /// NVENC video encoding.
    Video,
    /// NVIDIA management and device utilities.
    Utility,
}

/// Exact build inputs for the canonical simulation-base lineage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationRuntimeBuildLock {
    /// Schema identifier.
    pub schema_version: SimulationRuntimeBuildLockSchema,
    /// Stable compatibility profile.
    pub profile: ArtifactName,
    /// Immutable upstream Isaac Sim image coordinate.
    pub upstream_image: ArtifactCoordinate,
    /// Immutable upstream Isaac Sim platform manifest.
    pub upstream_digest: ArtifactDigest,
    /// Exact runtime tuple produced by the build.
    pub components: Vec<RuntimeComponentVersion>,
    /// Exact source inputs.
    pub sources: Vec<SimulationSourceInput>,
    /// Hash-verified replacement distributions.
    pub python_distributions: Vec<PythonDistributionInput>,
    /// Authoritative loaded package roots.
    pub authoritative_package_roots: Vec<String>,
    /// Components that a derived overlay may not replace.
    pub overlay_immutable_components: BTreeSet<RuntimeComponent>,
    /// Required NVIDIA runtime boundary.
    pub gpu: SimulationGpuRequirement,
}

impl SimulationRuntimeBuildLock {
    /// Validates the complete canonical build-input contract.
    pub fn validate(&self) -> Result<(), ExtensionContractError> {
        let required_components = canonical_runtime_components();
        let components = unique_components(&self.components)?;
        if components != required_components {
            return Err(ExtensionContractError::RuntimeComponents {
                expected: required_components,
                actual: components,
            });
        }
        if self.overlay_immutable_components != required_components {
            return Err(ExtensionContractError::RuntimeComponents {
                expected: required_components,
                actual: self.overlay_immutable_components.clone(),
            });
        }
        if self.sources.is_empty() {
            return Err(ExtensionContractError::Empty {
                field: "simulation sources",
            });
        }
        let mut source_components = BTreeSet::new();
        for source in &self.sources {
            if !source_components.insert(source.component) {
                return Err(ExtensionContractError::Duplicate {
                    kind: "simulation source component",
                    identity: format!("{:?}", source.component),
                });
            }
            if !source.repository.starts_with("https://")
                || source.tag.trim().is_empty()
                || source
                    .prerelease_reason
                    .as_ref()
                    .is_some_and(|reason| reason.trim().is_empty())
            {
                return Err(ExtensionContractError::Empty {
                    field: "simulation source identity",
                });
            }
        }
        if !source_components.contains(&RuntimeComponent::IsaacLab) {
            return Err(ExtensionContractError::Empty {
                field: "Isaac Lab source",
            });
        }
        if self.python_distributions.is_empty() {
            return Err(ExtensionContractError::Empty {
                field: "simulation Python distributions",
            });
        }
        let mut distributions = BTreeSet::new();
        for distribution in &self.python_distributions {
            if !distributions.insert(distribution.package.clone()) {
                return Err(ExtensionContractError::Duplicate {
                    kind: "simulation Python distribution",
                    identity: distribution.package.to_string(),
                });
            }
            if distribution.version.trim().is_empty()
                || distribution.filename.trim().is_empty()
                || !distribution.filename.ends_with(".whl")
            {
                return Err(ExtensionContractError::Empty {
                    field: "simulation Python distribution identity",
                });
            }
        }
        let roots = self
            .authoritative_package_roots
            .iter()
            .map(|root| root.as_str())
            .collect::<BTreeSet<_>>();
        if roots != BTreeSet::from(["newton", "torch", "warp"]) {
            return Err(ExtensionContractError::Empty {
                field: "authoritative Torch, Warp, and Newton package roots",
            });
        }
        if self.gpu.runtime.count == 0
            || self.gpu.runtime.shared_memory_bytes < 2 * 1024 * 1024 * 1024
            || self.gpu.minimum_driver_version.trim().is_empty()
            || self.gpu.driver_capabilities
                != BTreeSet::from([
                    NvidiaDriverCapability::Compute,
                    NvidiaDriverCapability::Graphics,
                    NvidiaDriverCapability::Utility,
                    NvidiaDriverCapability::Video,
                ])
        {
            return Err(ExtensionContractError::Empty {
                field: "simulation NVIDIA runtime requirement",
            });
        }
        Ok(())
    }
}

/// Kind of simulator overlay certified against the canonical base.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SimulationOverlayKind {
    /// Veoveo's UAV simulator overlay.
    FirstPartyUav,
    /// Repository-neutral external-overlay acceptance fixture.
    AnonymousExternal,
}

/// Hardware-backed probe required for simulation certification.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SimulationProbeKind {
    /// Exact component tuple and Isaac Lab import.
    ComponentTuple,
    /// One authoritative Warp and Newton module graph.
    ModuleGraph,
    /// Newton tiled cameras executed on CUDA.
    NewtonTiledCamera,
    /// Independent RaytracedLighting camera products.
    IndependentRtxCameras,
    /// Overlay identity and immutable-base lock remained intact.
    OverlayBoundary,
}

/// One successful hardware probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationProbeResult {
    /// Probe identity.
    pub probe: SimulationProbeKind,
    /// Wall-clock probe duration in milliseconds.
    pub duration_milliseconds: u64,
}

/// Hardware observed during simulation certification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationHardwareEvidence {
    /// NVIDIA GPU product name.
    pub gpu_name: String,
    /// Exact NVIDIA host driver version.
    pub driver_version: String,
    /// Warp CUDA device identity.
    pub cuda_device: String,
    /// Hardware graphics API.
    pub graphics_api: String,
    /// RTX renderer profile.
    pub renderer: String,
}

/// OCI attestations inspected for the certified base image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationAttestationEvidence {
    /// SHA-256 identity of BuildKit's SPDX SBOM projection.
    pub sbom_digest: ArtifactDigest,
    /// SHA-256 identity of BuildKit's SLSA provenance projection.
    pub provenance_digest: ArtifactDigest,
}

/// Hardware-backed result for one overlay and one immutable simulation base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationConformanceResult {
    /// Schema identifier.
    pub schema_version: SimulationConformanceResultSchema,
    /// Canonical compatibility profile.
    pub profile: ArtifactName,
    /// Certified canonical base image.
    pub base_image: ArtifactCoordinate,
    /// Canonical base manifest digest.
    pub base_digest: ArtifactDigest,
    /// Overlay class.
    pub overlay_kind: SimulationOverlayKind,
    /// Certified overlay image.
    pub overlay_image: ArtifactCoordinate,
    /// Overlay manifest digest.
    pub overlay_digest: ArtifactDigest,
    /// Exact source revision that produced the overlay.
    pub source_revision: SourceRevision,
    /// SHA-256 identity of the embedded build-input lock.
    pub build_lock_digest: ArtifactDigest,
    /// Runtime tuple observed after Kit startup.
    pub components: Vec<RuntimeComponentVersion>,
    /// Observed hardware.
    pub hardware: SimulationHardwareEvidence,
    /// OCI attestations attached to the canonical base manifest.
    pub attestations: SimulationAttestationEvidence,
    /// Independent camera count.
    pub camera_count: u32,
    /// RFC 3339 completion timestamp.
    pub completed_at: String,
    /// Successful required probes.
    pub probes: Vec<SimulationProbeResult>,
}

impl SimulationConformanceResult {
    /// Rejects incomplete or software-backed acceptance evidence.
    pub fn validate(&self) -> Result<(), ExtensionContractError> {
        let components = unique_components(&self.components)?;
        let required_components = canonical_runtime_components();
        if components != required_components {
            return Err(ExtensionContractError::RuntimeComponents {
                expected: required_components,
                actual: components,
            });
        }
        let required_probes = BTreeSet::from([
            SimulationProbeKind::ComponentTuple,
            SimulationProbeKind::ModuleGraph,
            SimulationProbeKind::NewtonTiledCamera,
            SimulationProbeKind::IndependentRtxCameras,
            SimulationProbeKind::OverlayBoundary,
        ]);
        let mut probes = BTreeSet::new();
        for probe in &self.probes {
            if !probes.insert(probe.probe) {
                return Err(ExtensionContractError::Duplicate {
                    kind: "simulation probe",
                    identity: format!("{:?}", probe.probe),
                });
            }
        }
        if probes != required_probes || self.camera_count < 20 {
            return Err(ExtensionContractError::Empty {
                field: "simulation hardware probes",
            });
        }
        let forbidden = ["swiftshader", "llvmpipe", "software rasterizer"];
        let hardware = format!(
            "{} {} {} {}",
            self.hardware.gpu_name,
            self.hardware.cuda_device,
            self.hardware.graphics_api,
            self.hardware.renderer
        )
        .to_ascii_lowercase();
        if !self
            .hardware
            .gpu_name
            .to_ascii_lowercase()
            .contains("nvidia")
            || self.hardware.driver_version.trim().is_empty()
            || !self.hardware.cuda_device.starts_with("cuda:")
            || self.hardware.graphics_api != "Vulkan"
            || self.hardware.renderer != "RaytracedLighting"
            || forbidden.iter().any(|name| hardware.contains(name))
            || self.completed_at.trim().is_empty()
        {
            return Err(ExtensionContractError::Empty {
                field: "hardware-backed simulation evidence",
            });
        }
        Ok(())
    }
}

/// Immutable publication evidence for one canonical simulation runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationRuntimeReleaseEvidence {
    /// Schema identifier.
    pub schema_version: SimulationRuntimeReleaseEvidenceSchema,
    /// Exact source revision that produced the runtime and evidence.
    pub source_revision: SourceRevision,
    /// Stable compatibility profile.
    pub profile: ArtifactName,
    /// Published canonical base image.
    pub base_image: ArtifactDescriptor,
    /// Exact runtime component tuple.
    pub components: Vec<RuntimeComponentVersion>,
    /// Required Kubernetes GPU boundary.
    pub gpu: GpuRuntimeRequirement,
    /// Published OCI bundle containing the lock, schemas, and paired results.
    pub conformance_result: ArtifactDescriptor,
    /// Hardware results for the first-party and anonymous overlay classes.
    pub results: Vec<SimulationConformanceResult>,
}

impl SimulationRuntimeReleaseEvidence {
    /// Validates the paired-overlay publication boundary.
    pub fn validate(&self) -> Result<(), ExtensionContractError> {
        if self.base_image.kind != ArtifactKind::OciImage {
            return Err(ExtensionContractError::ArtifactKind {
                field: "simulation base image",
                expected: ArtifactKind::OciImage,
                actual: self.base_image.kind,
            });
        }
        if self.conformance_result.kind != ArtifactKind::ConformanceResult {
            return Err(ExtensionContractError::ArtifactKind {
                field: "simulation conformance result",
                expected: ArtifactKind::ConformanceResult,
                actual: self.conformance_result.kind,
            });
        }
        if self.gpu.count == 0 || self.gpu.shared_memory_bytes == 0 {
            return Err(ExtensionContractError::Empty {
                field: "simulation GPU requirement",
            });
        }
        let components = unique_components(&self.components)?;
        let result_kinds = self
            .results
            .iter()
            .map(|result| result.overlay_kind)
            .collect::<BTreeSet<_>>();
        if self.results.len() != 2
            || result_kinds
                != BTreeSet::from([
                    SimulationOverlayKind::FirstPartyUav,
                    SimulationOverlayKind::AnonymousExternal,
                ])
        {
            return Err(ExtensionContractError::Empty {
                field: "paired simulation overlay results",
            });
        }
        let mut lock_digest = None;
        for result in &self.results {
            result.validate()?;
            if result.source_revision != self.source_revision
                || result.profile != self.profile
                || result.base_image != self.base_image.coordinate
                || result.base_digest != self.base_image.digest
                || unique_components(&result.components)? != components
                || result.components != self.components
            {
                return Err(ExtensionContractError::Empty {
                    field: "simulation release result identity",
                });
            }
            if lock_digest
                .replace(result.build_lock_digest.clone())
                .is_some_and(|digest| digest != result.build_lock_digest)
            {
                return Err(ExtensionContractError::Empty {
                    field: "shared simulation build lock",
                });
            }
        }
        Ok(())
    }
}

fn unique_components(
    versions: &[RuntimeComponentVersion],
) -> Result<BTreeSet<RuntimeComponent>, ExtensionContractError> {
    let mut components = BTreeSet::new();
    for component in versions {
        if !components.insert(component.component) {
            return Err(ExtensionContractError::Duplicate {
                kind: "simulation component",
                identity: format!("{:?}", component.component),
            });
        }
        if component.version.trim().is_empty() {
            return Err(ExtensionContractError::Empty {
                field: "simulation component version",
            });
        }
    }
    Ok(components)
}
