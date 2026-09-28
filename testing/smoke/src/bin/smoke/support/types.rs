use super::*;

pub(crate) const INTERNAL_SIGNING_KEY_DER_B64: &str =
    "MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl";
pub(crate) const REFRESH_DELIVERY_KEY_B64: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";
pub(crate) const REFRESH_DELIVERY_WINDOW_SECONDS: u64 = 5;
pub(crate) const INTERNAL_TRUST_JWKS: &str = r#"{"keys":[{"kty":"OKP","crv":"Ed25519","x":"OMOoJJu_AQS7UM8u2GVtMVj8W1zcE6QhR0DMBr9HEcg","alg":"EdDSA","use":"sig","kid":"veoveo-internal-1"}]}"#;
pub(crate) const PUBLIC_BASE_URL: &str = "https://veoveo.example";

#[derive(Debug, Deserialize)]
pub(crate) struct SmokeFramesBatchOutput {
    pub(crate) result: Value,
    pub(crate) artifact: Option<veoveo_artifact_contract::ArtifactMetadata>,
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
