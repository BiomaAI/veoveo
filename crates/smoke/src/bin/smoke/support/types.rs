use super::*;

pub(crate) const INTERNAL_SECRET: &str = "local-smoke-internal-token-secret-32-bytes-minimum";
pub(crate) const PUBLIC_BASE_URL: &str = "https://veoveo.bioma.ai";
/// 64 hex chars = 32 bytes; the artifact plane's per-tenant envelope master key.
/// Test-only value; the real key is provisioned per deployment.
pub(crate) const ARTIFACT_MASTER_KEY: &str =
    "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

#[derive(Debug, Deserialize)]
pub(crate) struct SmokeGenerationRunOutput {
    pub(crate) artifacts: Vec<SmokeArtifactMetadata>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SmokeCoordinatesBatchOutput {
    pub(crate) result: Value,
    pub(crate) artifact: Option<SmokeArtifactMetadata>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SmokeArtifactMetadata {
    pub(crate) sha256: String,
    pub(crate) artifact_uri: String,
    #[serde(default)]
    pub(crate) download_url: Option<String>,
    #[serde(default)]
    pub(crate) metadata: Value,
    #[serde(default)]
    pub(crate) compliance: SmokeCompliance,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct SmokeCompliance {
    pub(crate) tenant_id: Option<String>,
    #[serde(default)]
    pub(crate) data_labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SmokeUsageReport {
    pub(crate) task_id: String,
    pub(crate) usage_uri: String,
    pub(crate) records: Vec<SmokeUsageRecord>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SmokeUsageRecord {
    pub(crate) task_id: String,
    pub(crate) kind: SmokeUsageKind,
    pub(crate) quantity: Option<f64>,
    pub(crate) unit: Option<String>,
    pub(crate) amount: Option<f64>,
    pub(crate) currency: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SmokeUsageKind {
    Estimate,
    Actual,
}
