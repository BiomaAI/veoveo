use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ArtifactCoordinate, ArtifactDigest, ArtifactName, ReleaseVersion};

/// Supported immutable artifact classes.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// OCI application image.
    OciImage,
    /// OCI-packaged Helm chart.
    HelmChart,
    /// Python wheel.
    PythonWheel,
    /// Python source distribution.
    PythonSdist,
    /// Cargo package.
    RustCrate,
    /// Native executable archive.
    NativeBinary,
    /// Standalone conformance OCI image.
    ConformanceImage,
    /// Machine-readable conformance result.
    ConformanceResult,
    /// Extension-owned gateway server fragment.
    GatewayServerFragment,
    /// JSON Schema bundle.
    SchemaBundle,
    /// Software bill of materials.
    Sbom,
    /// Build provenance or attestation bundle.
    Provenance,
}

/// Operating system selected by an artifact.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OperatingSystem {
    /// Linux runtime.
    Linux,
    /// macOS runtime.
    Macos,
    /// Windows runtime.
    Windows,
    /// Platform-independent artifact.
    Any,
}

/// CPU architecture selected by an artifact.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CpuArchitecture {
    /// AMD64/x86-64.
    Amd64,
    /// ARM64/AArch64.
    Arm64,
    /// Platform-independent artifact.
    Any,
}

/// Optional execution platform attached to an artifact.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactPlatform {
    /// Operating system.
    pub operating_system: OperatingSystem,
    /// CPU architecture.
    pub architecture: CpuArchitecture,
}

/// One immutable release artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactDescriptor {
    /// Stable artifact name.
    pub name: ArtifactName,
    /// Artifact class.
    pub kind: ArtifactKind,
    /// Artifact semantic version.
    pub version: ReleaseVersion,
    /// Installation-resolved distribution coordinate.
    pub coordinate: ArtifactCoordinate,
    /// SHA-256 identity of the artifact bytes or OCI manifest.
    pub digest: ArtifactDigest,
    /// Optional execution platform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<ArtifactPlatform>,
    /// Optional registered media type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
}
