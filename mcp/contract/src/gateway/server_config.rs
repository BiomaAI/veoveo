use std::{collections::BTreeSet, fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServerManifest {
    pub slug: ServerSlug,
    pub uri_scheme: ResourceScheme,
    pub mount_path: MountPath,
    pub mcp_path: MountPath,
    pub upstream: UpstreamEndpoint,
    pub capabilities: McpSurfaceCapabilities,
    #[serde(default)]
    pub resource_projection: ResourceProjectionMode,
    /// Canonical schemes owned by other registered servers that remain unchanged when
    /// `server_owned` projection namespaces this server's App and vendor resources.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub referenced_resource_schemes: BTreeSet<ResourceScheme>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<LocalToolName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compatibility_helpers: Vec<LocalToolName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<PromptName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_scopes: Vec<ScopeName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owned_routes: Vec<OwnedRoute>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ResourceProjectionMode {
    #[default]
    Identity,
    ServerOwned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayProfile {
    pub id: GatewayProfileId,
    pub identity_provider: IdentityProviderId,
    pub authorization_server: AuthorizationServerId,
    pub protected_resource: ProtectedResourceId,
    pub policy_version: PolicyVersion,
    pub auth_modes: BTreeSet<AuthMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_scopes: Vec<ScopeName>,
    pub servers: Vec<ProfileServerExposure>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileServerExposure {
    pub server: ServerSlug,
    pub tools: Exposure<LocalToolName>,
    pub resources: Exposure<ResourceSelector>,
    pub prompts: Exposure<PromptName>,
    pub completions: CompletionExposure,
    pub tasks: TaskExposure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "mode", content = "items")]
pub enum Exposure<T> {
    All,
    Listed(Vec<T>),
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ResourceSelector {
    Scheme { scheme: ResourceScheme },
    UriPrefix { prefix: ResourceUriPrefix },
    Template { uri_template: ResourceUriTemplate },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CompletionExposure {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskExposure {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SecretReference {
    pub id: SecretReferenceId,
    pub source: SecretSource,
    pub purpose: SecretPurpose,
    pub locator: SecretLocator,
    pub owner: SecretOwner,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_hint: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SecretSource {
    Env,
    Vault,
    HcpVault,
    CloudSecretManager,
    KmsBackedStore,
    EnterpriseManaged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecretPurpose {
    ProviderApiKey,
    WebhookSecret,
    #[serde(rename = "oauth_client_secret")]
    OAuthClientSecret,
    GatewaySigningKey,
    JwksPrivateKey,
    TokenExchangeCredential,
    ObjectStoreCredential,
    TlsClientCertificate,
    TlsClientPrivateKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum SecretOwner {
    Gateway,
    Profile { profile: GatewayProfileId },
    Server { server: ServerSlug },
    Tenant { tenant: TenantId },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct MountPath(String);

impl MountPath {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_mount_path(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for MountPath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for MountPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for MountPath {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<MountPath> for String {
    fn from(value: MountPath) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct HttpsUrl(String);

impl HttpsUrl {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_https_url(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for HttpsUrl {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for HttpsUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for HttpsUrl {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<HttpsUrl> for String {
    fn from(value: HttpsUrl) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct OAuthEndpointUrl(String);

impl OAuthEndpointUrl {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_oauth_endpoint_url(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for OAuthEndpointUrl {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for OAuthEndpointUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for OAuthEndpointUrl {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<OAuthEndpointUrl> for String {
    fn from(value: OAuthEndpointUrl) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct UpstreamUrl(String);

impl UpstreamUrl {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_upstream_url(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(super) fn parsed(&self) -> Result<Url, IdentifierError> {
        Url::parse(&self.0).map_err(|_| IdentifierError::new(&self.0, "must be a valid URL"))
    }
}

impl AsRef<str> for UpstreamUrl {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for UpstreamUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for UpstreamUrl {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<UpstreamUrl> for String {
    fn from(value: UpstreamUrl) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct OAuthRedirectUri(String);

impl OAuthRedirectUri {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_oauth_redirect_uri(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for OAuthRedirectUri {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for OAuthRedirectUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for OAuthRedirectUri {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl FromStr for OAuthRedirectUri {
    type Err = IdentifierError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value.to_string())
    }
}

impl From<OAuthRedirectUri> for String {
    fn from(value: OAuthRedirectUri) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct JwksFilePath(String);

impl JwksFilePath {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_local_file_path(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for JwksFilePath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for JwksFilePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for JwksFilePath {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl FromStr for JwksFilePath {
    type Err = IdentifierError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value.to_string())
    }
}

impl From<JwksFilePath> for String {
    fn from(value: JwksFilePath) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct CertificateAuthorityFilePath(String);

impl CertificateAuthorityFilePath {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_local_file_path(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for CertificateAuthorityFilePath {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for CertificateAuthorityFilePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for CertificateAuthorityFilePath {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl FromStr for CertificateAuthorityFilePath {
    type Err = IdentifierError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value.to_string())
    }
}

impl From<CertificateAuthorityFilePath> for String {
    fn from(value: CertificateAuthorityFilePath) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceUri(String);

impl ResourceUri {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_resource_uri(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ResourceUri {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ResourceUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ResourceUri {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ResourceUri> for String {
    fn from(value: ResourceUri) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceUriPrefix(String);

impl ResourceUriPrefix {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_resource_uri(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for ResourceUriPrefix {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ResourceUriPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ResourceUriPrefix {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ResourceUriPrefix> for String {
    fn from(value: ResourceUriPrefix) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceUriTemplate(String);

impl ResourceUriTemplate {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
        let value = value.into();
        validate_uri_template(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn matches_uri(&self, uri: &ResourceUri) -> bool {
        resource_uri_template_matches(self.as_str(), uri.as_str())
    }
}

impl AsRef<str> for ResourceUriTemplate {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for ResourceUriTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ResourceUriTemplate {
    type Error = IdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ResourceUriTemplate> for String {
    fn from(value: ResourceUriTemplate) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct UpstreamEndpoint {
    pub transport: UpstreamTransport,
    pub url: UpstreamUrl,
    pub security: UpstreamTransportSecurity,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_certificate_authorities: Vec<CertificateAuthoritySource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_certificate: Option<SecretReferenceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_private_key: Option<SecretReferenceId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamTransport {
    StreamableHttp,
}

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
pub struct OwnedRoute {
    pub path: MountPath,
    pub purpose: OwnedRoutePurpose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OwnedRoutePurpose {
    Webhook,
    ArtifactBytes,
    ProviderFetchableFiles,
    Health,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct McpSurfaceCapabilities {
    pub tools: bool,
    pub resources: bool,
    /// The server ships MCP App views (`ui://` HTML resources). Requires
    /// `resources` and server-owned resource projection.
    #[serde(default)]
    pub apps: bool,
    pub resource_templates: bool,
    pub resource_subscriptions: bool,
    pub prompts: bool,
    pub completions: bool,
    pub tasks: bool,
    pub notifications: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    OidcAuthorizationCodePkce,
    EnterpriseManagedAuthorization,
    #[serde(rename = "oauth_client_credentials")]
    OAuthClientCredentials,
}

impl AuthMode {
    pub fn mcp_extension_id(self) -> Option<&'static str> {
        match self {
            Self::OidcAuthorizationCodePkce => None,
            Self::EnterpriseManagedAuthorization => {
                Some(MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION)
            }
            Self::OAuthClientCredentials => Some(MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION),
        }
    }
}

impl From<AuthMode> for OAuthGrantType {
    fn from(value: AuthMode) -> Self {
        match value {
            AuthMode::OidcAuthorizationCodePkce => Self::AuthorizationCodePkce,
            AuthMode::EnterpriseManagedAuthorization => Self::EnterpriseManagedAuthorization,
            AuthMode::OAuthClientCredentials => Self::ClientCredentials,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oauth_endpoints_allow_only_https_or_explicit_loopback_http() {
        OAuthEndpointUrl::new("https://idp.example.com/oauth/token").unwrap();
        OAuthEndpointUrl::new("http://localhost:8780/oauth/token").unwrap();
        OAuthEndpointUrl::new("http://127.0.0.1:8780/oauth/token").unwrap();

        assert!(OAuthEndpointUrl::new("http://localhost/oauth/token").is_err());
        assert!(OAuthEndpointUrl::new("http://gateway:8780/oauth/token").is_err());
    }
}
