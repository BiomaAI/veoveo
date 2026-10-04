//! Shared HTTP endpoints and TLS declarations; no MCP transport or runtime.
use crate::SecretReferenceId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use url::Url;
use veoveo_types::IdentifierError;

#[veoveo_types::id(text(UpstreamUrls))]
pub struct UpstreamUrl(String);
impl UpstreamUrl {
    pub fn parsed(&self) -> Result<Url, IdentifierError> {
        Url::parse(self.as_str())
            .map_err(|_| IdentifierError::new(self.as_str(), "must be a valid URL"))
    }
}
#[veoveo_types::id(text(LocalFilePaths))]
pub struct CertificateAuthorityFilePath(String);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamTransportSecurity {
    LoopbackHttp,
    ClusterInternalHttp,
    Tls,
    MutualTls,
    ServiceMeshMtls,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "source")]
pub enum CertificateAuthoritySource {
    File { path: CertificateAuthorityFilePath },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HttpUpstreamEndpoint {
    pub url: UpstreamUrl,
    pub health_url: UpstreamUrl,
    pub security: UpstreamTransportSecurity,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_certificate_authorities: Vec<CertificateAuthoritySource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_certificate: Option<SecretReferenceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_private_key: Option<SecretReferenceId>,
}
fn validate_upstream_url(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    let url = Url::parse(value).map_err(|_| IdentifierError::new(value, "must be a valid URL"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(IdentifierError::new(value, "must use http:// or https://"));
    }
    if url.host().is_none() {
        return Err(IdentifierError::new(value, "must include a host"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(IdentifierError::new(value, "must not contain userinfo"));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(IdentifierError::new(
            value,
            "must not contain a query or fragment",
        ));
    }
    Ok(())
}

fn validate_local_file_path(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.starts_with("http://") || value.starts_with("https://") || value.starts_with("file://")
    {
        return Err(IdentifierError::new(
            value,
            "must be a local filesystem path, not a URL",
        ));
    }
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    Ok(())
}

#[doc(hidden)]
pub struct UpstreamUrls;
impl veoveo_types::IdProfile for UpstreamUrls {
    type Error = IdentifierError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| validate_upstream_url(value));
}

#[doc(hidden)]
pub struct LocalFilePaths;
impl veoveo_types::IdProfile for LocalFilePaths {
    type Error = IdentifierError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| validate_local_file_path(value));
}
