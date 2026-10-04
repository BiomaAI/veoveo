use std::{collections::BTreeSet, fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ServerManifest {
    pub slug: ServerSlug,
    pub uri_scheme: ResourceScheme,
    pub mount_path: MountPath,
    pub mcp_path: MountPath,
    pub upstream: UpstreamEndpoint,
    pub capabilities: McpSurfaceCapabilities,
    /// Installation approval for this server's declared Knowledge collections.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub knowledge: Vec<veoveo_knowledge_contract::KnowledgeCollectionApproval>,
    #[serde(default)]
    pub resource_projection: ResourceProjectionMode,
    /// Canonical schemes owned by other registered servers that remain unchanged when
    /// `server_owned` projection namespaces this server's App and vendor resources.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub referenced_resource_schemes: BTreeSet<ResourceScheme>,
    /// Installation-validated cross-server resource families available to
    /// one exact App resource owned by this server.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub app_resource_dependencies: Vec<AppResourceDependency>,
    /// Installation-validated tools imported by one exact App resource.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub app_tool_dependencies: Vec<AppToolDependency>,
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
#[serde(deny_unknown_fields)]
pub struct GatewayProfile {
    pub id: GatewayProfileId,
    pub identity_provider: IdentityProviderId,
    pub authorization_server: AuthorizationServerId,
    pub protected_resource: ProtectedResourceId,
    pub policy_version: PolicyVersion,
    /// An explicit policy enables the public resumable upload surface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_upload: Option<crate::ArtifactUploadPolicy>,
    pub auth_modes: BTreeSet<AuthMode>,
    /// Whether federated list discovery may return a typed degraded catalog or
    /// must fail until every server in this profile is reachable.
    #[serde(default)]
    pub discovery_failure_mode: DiscoveryFailureMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_scopes: Vec<ScopeName>,
    pub servers: Vec<ProfileServerExposure>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryFailureMode {
    #[default]
    Isolate,
    FailClosed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProfileServerExposure {
    pub server: ServerSlug,
    pub tools: Exposure<LocalToolName>,
    pub resources: Exposure<ResourceSelector>,
    pub prompts: Exposure<PromptName>,
    pub completions: CompletionExposure,
    pub tasks: TaskExposure,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(
    rename_all = "snake_case",
    tag = "mode",
    content = "items",
    deny_unknown_fields
)]
pub enum Exposure<T> {
    All,
    Listed(Vec<T>),
    None,
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Exposure<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case", tag = "mode", deny_unknown_fields)]
        enum Wire<T> {
            All {},
            Listed { items: Vec<T> },
            None {},
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::All {} => Self::All,
            Wire::Listed { items } => Self::Listed(items),
            Wire::None {} => Self::None,
        })
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpstreamEndpoint {
    pub transport: UpstreamTransport,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamTransport {
    StreamableHttp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
    #[serde(default)]
    pub tools_list_changed: bool,
    #[serde(default)]
    pub prompts_list_changed: bool,
    #[serde(default)]
    pub resources_list_changed: bool,
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
