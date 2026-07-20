use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IdentityProvider {
    pub id: IdentityProviderId,
    pub issuer: TokenIssuer,
    pub jwks: JwksSource,
    #[serde(default)]
    pub claim_mapping: IdentityProviderClaimMapping,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trusted_certificate_authorities: Vec<CertificateAuthoritySource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_endpoint: Option<OAuthEndpointUrl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_endpoint: Option<OAuthEndpointUrl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enterprise_managed_authorization_endpoint: Option<OAuthEndpointUrl>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IdentityProviderClaimMapping {
    #[serde(default)]
    pub subject: IdentityProviderSubjectClaim,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<IdentityProviderTenantClaimMapping>,
}

impl Default for IdentityProviderClaimMapping {
    fn default() -> Self {
        Self {
            subject: IdentityProviderSubjectClaim::Sub,
            tenant: Some(IdentityProviderTenantClaimMapping::default()),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdentityProviderSubjectClaim {
    #[default]
    Sub,
    Oid,
    Email,
    PreferredUsername,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IdentityProviderTenantClaimMapping {
    pub claim: IdentityProviderTenantClaim,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, TenantId>,
}

impl Default for IdentityProviderTenantClaimMapping {
    fn default() -> Self {
        Self {
            claim: IdentityProviderTenantClaim::Tenant,
            values: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IdentityProviderTenantClaim {
    Tenant,
    Tid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResourceAuthorizationServer {
    pub id: AuthorizationServerId,
    pub issuer: TokenIssuer,
    pub jwks: JwksSource,
    pub access_token_key_id: JwtId,
    pub access_token_signing_key: SecretReferenceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_provider: Option<IdentityProviderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_endpoint: Option<OAuthEndpointUrl>,
    pub token_endpoint: OAuthEndpointUrl,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "source")]
pub enum JwksSource {
    Remote { jwks_uri: HttpsUrl },
    File { path: JwksFilePath },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "source")]
pub enum CertificateAuthoritySource {
    File { path: CertificateAuthorityFilePath },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OAuthClientRegistration {
    pub id: OAuthClientId,
    pub authorization_server: AuthorizationServerId,
    /// Canonical context used when this client does not explicitly select a
    /// different context through an authorized interactive session.
    pub default_work_context: WorkContextId,
    /// Invocation provenance this OAuth relationship is allowed to establish.
    pub invocation_mode: crate::InvocationMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default)]
    pub client_surface: OAuthClientSurface,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_compatibility_helpers: BTreeSet<CompatibilityHelperId>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub direct_task_call_adapter: bool,
    pub allowed_resources: BTreeSet<ProtectedResourceId>,
    pub grant_types: BTreeSet<OAuthGrantType>,
    pub auth_methods: BTreeSet<OAuthClientAuthMethod>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redirect_uris: Vec<OAuthRedirectUri>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub allowed_scopes: BTreeSet<ScopeName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_secret: Option<SecretReferenceId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwks: Option<JwksSource>,
    /// Tenant this client's service principal acts under. Client-credentials
    /// tokens carry it as the `tenant` claim, which tenant-scoped services
    /// (the artifact plane, tenant policy checks) require. Browser and
    /// enterprise grants derive the tenant from the user identity instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantId>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum OAuthClientSurface {
    #[default]
    FullMcp,
    ToolsCompat,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IdentityProviderOidcClientRegistration {
    pub id: OidcClientRegistrationId,
    pub identity_provider: IdentityProviderId,
    pub authorization_server: AuthorizationServerId,
    pub allowed_resources: BTreeSet<ProtectedResourceId>,
    pub client_id: OidcClientId,
    pub redirect_uri: OAuthRedirectUri,
    pub auth_method: OidcClientAuthMethod,
    pub credential_secret: SecretReferenceId,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub scopes: BTreeSet<ScopeName>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OidcClientAuthMethod {
    ClientSecretBasic,
    ClientSecretPost,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OAuthGrantType {
    AuthorizationCodePkce,
    RefreshToken,
    ClientCredentials,
    EnterpriseManagedAuthorization,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OAuthClientAuthMethod {
    None,
    PrivateKeyJwt,
    ClientSecretBasic,
    ClientSecretPost,
    TlsClientAuth,
}

impl OAuthClientAuthMethod {
    pub(crate) fn requires_secret(&self) -> bool {
        matches!(self, Self::ClientSecretBasic | Self::ClientSecretPost)
    }

    pub(crate) fn requires_jwks(&self) -> bool {
        matches!(self, Self::PrivateKeyJwt)
    }
}
