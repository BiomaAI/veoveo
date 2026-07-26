use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    ArtifactDescriptor, ArtifactDigest, ArtifactKind, ArtifactName, CompatibilityReleaseId,
    ExtensionContractError, ExtensionId, ReleaseVersion, SourceRevision,
};

/// Extension-release schema identifier.
pub const EXTENSION_RELEASE_SCHEMA: &str = "veoveo.io/extension-release/v1";

/// Supported extension-release schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ExtensionReleaseSchema {
    /// Extension release version 1.
    #[serde(rename = "veoveo.io/extension-release/v1")]
    V1,
}

/// Exact independently owned source identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionSource {
    /// Installation-local source name.
    pub name: ArtifactName,
    /// Exact source revision.
    pub revision: SourceRevision,
}

/// Optional canonical simulation-base requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SimulationOverlayRequirement {
    /// Compatibility-manifest runtime profile.
    pub profile: ArtifactName,
    /// Required canonical base image digest.
    pub base_digest: ArtifactDigest,
}

/// One immutable independently published extension release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionReleaseManifest {
    /// Schema identifier.
    pub schema_version: ExtensionReleaseSchema,
    /// Stable extension identifier.
    pub extension: ExtensionId,
    /// Extension semantic version.
    pub version: ReleaseVersion,
    /// Exact extension source.
    pub source: ExtensionSource,
    /// Required Veoveo compatibility release.
    pub compatibility_release: CompatibilityReleaseId,
    /// Extension-owned immutable artifacts.
    pub artifacts: Vec<ArtifactDescriptor>,
    /// Extension Helm application chart.
    pub helm_chart: ArtifactDescriptor,
    /// Extension-owned gateway server fragment.
    pub gateway_fragment: ArtifactDescriptor,
    /// Conformance evidence for this release.
    pub conformance_results: Vec<ArtifactDescriptor>,
    /// Optional canonical simulation-base requirement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simulation_overlay: Option<SimulationOverlayRequirement>,
}

impl ExtensionReleaseManifest {
    /// Validates cross-field release invariants.
    pub fn validate(&self) -> Result<(), ExtensionContractError> {
        if self.artifacts.is_empty() {
            return Err(ExtensionContractError::Empty { field: "artifacts" });
        }
        if self.conformance_results.is_empty() {
            return Err(ExtensionContractError::Empty {
                field: "conformanceResults",
            });
        }
        require_kind("helmChart", &self.helm_chart, ArtifactKind::HelmChart)?;
        require_kind(
            "gatewayFragment",
            &self.gateway_fragment,
            ArtifactKind::GatewayServerFragment,
        )?;

        let mut coordinates = BTreeSet::new();
        for artifact in self
            .artifacts
            .iter()
            .chain(std::iter::once(&self.helm_chart))
            .chain(std::iter::once(&self.gateway_fragment))
            .chain(self.conformance_results.iter())
        {
            if !coordinates.insert(artifact.coordinate.clone()) {
                return Err(ExtensionContractError::Duplicate {
                    kind: "artifact coordinate",
                    identity: artifact.coordinate.to_string(),
                });
            }
        }
        for result in &self.conformance_results {
            require_kind(
                "conformanceResults",
                result,
                ArtifactKind::ConformanceResult,
            )?;
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
