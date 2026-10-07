use super::*;

pub const INTERNAL_SIGNING_KEY_DER_B64: &str =
    "MC4CAQAwBQYDK2VwBCIEII4AsVspz8h7mpqvOkgslJP07HfqpiWMZA+6Ii90lVBl";
pub const REFRESH_DELIVERY_KEY_B64: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY=";
// Public fixture seed, isolated from installation and assertion keys.
pub const AUDIT_SIGNING_KEY_B64: &str = "ERERERERERERERERERERERERERERERERERERERERERE=";
pub const REFRESH_DELIVERY_WINDOW_SECONDS: u64 = 5;
pub const INTERNAL_TRUST_JWKS: &str = r#"{"keys":[{"kty":"OKP","crv":"Ed25519","x":"OMOoJJu_AQS7UM8u2GVtMVj8W1zcE6QhR0DMBr9HEcg","alg":"EdDSA","use":"sig","kid":"veoveo-internal-1"}]}"#;
pub const PUBLIC_BASE_URL: &str = "https://veoveo.example";

#[derive(Debug, Deserialize)]
pub struct SmokeFramesBatchOutput {
    pub result: Value,
    pub artifact: Option<veoveo_artifact_contract::ArtifactMetadata>,
}

pub use veoveo_mcp_contract::{UsageKind, UsageReport};
