use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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
    /// PyTorch build and CUDA module graph.
    Torch,
    /// CUDA runtime.
    Cuda,
    /// Isaac RTX NVRTC builtins retained from the upstream image.
    IsaacRtxNvrtc,
    /// NVIDIA Kit runtime.
    Kit,
}

pub(crate) fn canonical_runtime_components() -> BTreeSet<RuntimeComponent> {
    BTreeSet::from([
        RuntimeComponent::IsaacSim,
        RuntimeComponent::IsaacLab,
        RuntimeComponent::Warp,
        RuntimeComponent::Newton,
        RuntimeComponent::Mujoco,
        RuntimeComponent::MujocoWarp,
        RuntimeComponent::Python,
        RuntimeComponent::Torch,
        RuntimeComponent::Cuda,
        RuntimeComponent::IsaacRtxNvrtc,
        RuntimeComponent::Kit,
    ])
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
