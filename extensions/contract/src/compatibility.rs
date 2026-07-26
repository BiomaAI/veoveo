use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    ArtifactDescriptor, ArtifactKind, ArtifactName, CompatibilityReleaseId, ExtensionContractError,
    ReleaseVersion, VersionRequirement,
};

/// Compatibility-manifest schema identifier.
pub const COMPATIBILITY_MANIFEST_SCHEMA: &str = "veoveo.io/compatibility-manifest/v1";

/// Supported compatibility-manifest schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CompatibilityManifestSchema {
    /// Compatibility manifest version 1.
    #[serde(rename = "veoveo.io/compatibility-manifest/v1")]
    V1,
}

/// Contract surfaces included in one compatibility release.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ContractKind {
    /// Hosted MCP server contract revision.
    McpServer,
    /// Gateway server fragment schema.
    GatewayServerFragment,
    /// Installation binding schema.
    GatewayBinding,
    /// Multi-source deployment profile schema.
    Deployment,
    /// Extension release manifest schema.
    ExtensionRelease,
    /// Extension Helm library API.
    ExtensionHelmLibrary,
    /// Standalone conformance profile.
    Conformance,
}

/// One supported contract revision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContractCompatibility {
    /// Contract kind.
    pub kind: ContractKind,
    /// Exact contract revision or schema identifier.
    pub version: String,
}

/// Supported language SDKs.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SdkLanguage {
    /// Python SDK.
    Python,
    /// Rust SDK.
    Rust,
}

/// One supported SDK artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SdkCompatibility {
    /// SDK language.
    pub language: SdkLanguage,
    /// Supported language runtime range.
    pub runtime: VersionRequirement,
    /// Immutable SDK package.
    pub artifact: ArtifactDescriptor,
}

/// Canonical simulation runtime component names.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeComponent {
    /// NVIDIA Isaac Sim.
    IsaacSim,
    /// NVIDIA Isaac Lab.
    IsaacLab,
    /// NVIDIA Warp.
    Warp,
    /// Newton.
    Newton,
    /// MuJoCo.
    Mujoco,
    /// MuJoCo Warp.
    MujocoWarp,
    /// CPython ABI.
    Python,
    /// CUDA runtime.
    Cuda,
}

/// Exact component version in a simulation profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeComponentVersion {
    /// Runtime component.
    pub component: RuntimeComponent,
    /// Exact version, tag, or release identity.
    pub version: String,
    /// Optional immutable source revision for source-delivered components.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

/// Required NVIDIA runtime boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GpuRuntimeRequirement {
    /// Kubernetes extended-resource name.
    pub resource_name: String,
    /// Required device count.
    pub count: u32,
    /// Optional Kubernetes RuntimeClass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_class_name: Option<String>,
    /// Private memory-backed shared-memory size in bytes.
    pub shared_memory_bytes: u64,
}

/// One supported canonical simulation base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationRuntimeCompatibility {
    /// Stable profile name.
    pub profile: ArtifactName,
    /// Immutable canonical base image.
    pub base_image: ArtifactDescriptor,
    /// Exact runtime component tuple.
    pub components: Vec<RuntimeComponentVersion>,
    /// Required hardware runtime.
    pub gpu: GpuRuntimeRequirement,
    /// Conformance evidence tied to the base digest.
    pub conformance_result: ArtifactDescriptor,
}

/// One exact supported Veoveo release surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompatibilityManifest {
    /// Schema identifier.
    pub schema_version: CompatibilityManifestSchema,
    /// Compatibility release identity.
    pub release: CompatibilityReleaseId,
    /// Veoveo platform semantic version.
    pub platform_version: ReleaseVersion,
    /// Supported contract revisions.
    pub contracts: Vec<ContractCompatibility>,
    /// Supported language SDKs.
    pub sdks: Vec<SdkCompatibility>,
    /// Standalone conformance distribution.
    pub conformance: ArtifactDescriptor,
    /// Offline deterministic gateway composition distribution.
    pub gateway_composer: ArtifactDescriptor,
    /// Extension Helm library package.
    pub helm_library: ArtifactDescriptor,
    /// Optional canonical simulation profiles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub simulation_runtimes: Vec<SimulationRuntimeCompatibility>,
}

impl CompatibilityManifest {
    /// Validates cross-field compatibility invariants.
    pub fn validate(&self) -> Result<(), ExtensionContractError> {
        if self.contracts.is_empty() {
            return Err(ExtensionContractError::Empty { field: "contracts" });
        }
        if self.sdks.is_empty() {
            return Err(ExtensionContractError::Empty { field: "sdks" });
        }
        require_kind(
            "conformance",
            &self.conformance,
            ArtifactKind::ConformanceImage,
        )?;
        require_kind(
            "gatewayComposer",
            &self.gateway_composer,
            ArtifactKind::OciImage,
        )?;
        require_kind("helmLibrary", &self.helm_library, ArtifactKind::HelmChart)?;

        let mut contracts = BTreeSet::new();
        for contract in &self.contracts {
            if !contracts.insert(contract.kind) {
                return Err(ExtensionContractError::Duplicate {
                    kind: "contract kind",
                    identity: format!("{:?}", contract.kind),
                });
            }
            if contract.version.trim().is_empty() {
                return Err(ExtensionContractError::Empty {
                    field: "contract version",
                });
            }
        }

        let mut sdks = BTreeSet::new();
        for sdk in &self.sdks {
            let key = (sdk.language, sdk.artifact.name.clone(), sdk.artifact.kind);
            if !sdks.insert(key) {
                return Err(ExtensionContractError::Duplicate {
                    kind: "SDK",
                    identity: format!("{:?}/{}", sdk.language, sdk.artifact.name),
                });
            }
            match sdk.language {
                SdkLanguage::Python
                    if !matches!(
                        sdk.artifact.kind,
                        ArtifactKind::PythonWheel | ArtifactKind::PythonSdist
                    ) =>
                {
                    return Err(ExtensionContractError::ArtifactKind {
                        field: "python SDK",
                        expected: ArtifactKind::PythonWheel,
                        actual: sdk.artifact.kind,
                    });
                }
                SdkLanguage::Rust if sdk.artifact.kind != ArtifactKind::RustCrate => {
                    return Err(ExtensionContractError::ArtifactKind {
                        field: "Rust SDK",
                        expected: ArtifactKind::RustCrate,
                        actual: sdk.artifact.kind,
                    });
                }
                _ => {}
            }
        }

        let mut runtime_profiles = BTreeSet::new();
        for runtime in &self.simulation_runtimes {
            if !runtime_profiles.insert(runtime.profile.clone()) {
                return Err(ExtensionContractError::Duplicate {
                    kind: "simulation runtime",
                    identity: runtime.profile.to_string(),
                });
            }
            require_kind(
                "simulation base image",
                &runtime.base_image,
                ArtifactKind::OciImage,
            )?;
            require_kind(
                "simulation conformance result",
                &runtime.conformance_result,
                ArtifactKind::ConformanceResult,
            )?;
            if runtime.gpu.count == 0 || runtime.gpu.shared_memory_bytes == 0 {
                return Err(ExtensionContractError::Empty {
                    field: "simulation GPU requirement",
                });
            }
            let mut components = BTreeSet::new();
            for component in &runtime.components {
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
        }
        Ok(())
    }
}

fn require_kind(
    field: &'static str,
    artifact: &ArtifactDescriptor,
    expected: ArtifactKind,
) -> Result<(), ExtensionContractError> {
    if artifact.kind != expected {
        return Err(ExtensionContractError::ArtifactKind {
            field,
            expected,
            actual: artifact.kind,
        });
    }
    Ok(())
}
