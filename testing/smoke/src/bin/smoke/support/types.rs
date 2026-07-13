use super::*;

pub(crate) const INTERNAL_SIGNING_KEY_DER_B64: &str =
    "MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl";
pub(crate) const REFRESH_DELIVERY_KEY_B64: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";
pub(crate) const REFRESH_DELIVERY_WINDOW_SECONDS: u64 = 5;
pub(crate) const INTERNAL_TRUST_JWKS: &str = r#"{"keys":[{"kty":"OKP","crv":"Ed25519","x":"OMOoJJu_AQS7UM8u2GVtMVj8W1zcE6QhR0DMBr9HEcg","alg":"EdDSA","use":"sig","kid":"veoveo-internal-1"}]}"#;
pub(crate) const PUBLIC_BASE_URL: &str = "https://veoveo.example";

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
    pub(crate) artifact_id: String,
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
