use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use validation::{
    resource_selector_description, validate_oauth_client_registration,
    validate_oidc_client_registration, validate_policy_set, validate_profile_auth_modes,
    validate_profile_server_exposure, validate_server_upstream,
    validate_server_upstream_tls_material,
};
use wire::{
    resource_uri_template_matches, validate_https_url, validate_local_file_path,
    validate_mount_path, validate_oauth_redirect_uri, validate_resource_uri, validate_upstream_url,
    validate_uri_template,
};

pub const MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION: &str =
    "io.modelcontextprotocol/enterprise-managed-authorization";
pub const MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION: &str =
    "io.modelcontextprotocol/oauth-client-credentials";

mod policy;
mod validation;
mod wire;
pub use policy::*;
mod runtime_state;
pub use runtime_state::*;
mod server_config;
pub use server_config::*;
mod auth_config;
pub use auth_config::*;
mod ids;
pub use ids::*;
mod data_label;
pub use data_label::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayControlPlane {
    pub identity_providers: Vec<IdentityProvider>,
    pub authorization_servers: Vec<ResourceAuthorizationServer>,
    pub servers: Vec<ServerManifest>,
    pub profiles: Vec<GatewayProfile>,
    pub policies: Vec<PolicySet>,
    pub data_labels: Vec<DataLabelDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub oauth_clients: Vec<OAuthClientRegistration>,
    pub oidc_clients: Vec<IdentityProviderOidcClientRegistration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<SecretReference>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayControlPlaneRevision {
    pub revision_id: GatewayControlPlaneRevisionId,
    pub sha256: String,
    pub source: GatewayControlPlaneRevisionSource,
    pub applied_at: DateTime<Utc>,
    pub applied_by: PrincipalId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantId>,
    pub control_plane: GatewayControlPlane,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GatewayControlPlaneRevisionSource {
    AdminApi,
    MountedFileReload,
}

impl GatewayControlPlane {
    pub fn validate(&self) -> Result<(), GatewayControlPlaneError> {
        let mut identity_providers = BTreeMap::new();
        for identity_provider in &self.identity_providers {
            if identity_providers
                .insert(identity_provider.id.clone(), identity_provider)
                .is_some()
            {
                return Err(GatewayControlPlaneError::DuplicateIdentityProvider(
                    identity_provider.id.clone(),
                ));
            }
        }

        let mut authorization_servers = BTreeMap::new();
        for authorization_server in &self.authorization_servers {
            if authorization_servers
                .insert(authorization_server.id.clone(), authorization_server)
                .is_some()
            {
                return Err(GatewayControlPlaneError::DuplicateAuthorizationServer(
                    authorization_server.id.clone(),
                ));
            }
            if let Some(identity_provider) = &authorization_server.identity_provider
                && !identity_providers.contains_key(identity_provider)
            {
                return Err(
                    GatewayControlPlaneError::UnknownAuthorizationServerIdentityProvider {
                        authorization_server: authorization_server.id.clone(),
                        identity_provider: identity_provider.clone(),
                    },
                );
            }
        }

        let mut servers = BTreeMap::new();
        let mut server_ids = BTreeSet::new();
        let mut resource_schemes = BTreeSet::new();
        for server in &self.servers {
            if !server_ids.insert(server.slug.clone()) {
                return Err(GatewayControlPlaneError::DuplicateServer(
                    server.slug.clone(),
                ));
            }
            servers.insert(server.slug.clone(), server);
            if !resource_schemes.insert(server.uri_scheme.clone()) {
                return Err(GatewayControlPlaneError::DuplicateResourceScheme(
                    server.uri_scheme.clone(),
                ));
            }
            validate_server_upstream(server)?;
        }

        let mut policies = BTreeSet::new();
        let mut policy_by_id = BTreeMap::new();
        for policy in &self.policies {
            if !policies.insert(policy.version.clone()) {
                return Err(GatewayControlPlaneError::DuplicatePolicy(
                    policy.version.clone(),
                ));
            }
            policy_by_id.insert(policy.version.clone(), policy);
        }

        let mut data_labels = BTreeSet::new();
        for data_label in &self.data_labels {
            if !data_labels.insert(data_label.id.clone()) {
                return Err(GatewayControlPlaneError::DuplicateDataLabel(
                    data_label.id.clone(),
                ));
            }
        }

        let mut profiles = BTreeSet::new();
        let mut profile_by_id = BTreeMap::new();
        for profile in &self.profiles {
            if !profiles.insert(profile.id.clone()) {
                return Err(GatewayControlPlaneError::DuplicateProfile(
                    profile.id.clone(),
                ));
            }
            profile_by_id.insert(profile.id.clone(), profile);
            let Some(identity_provider) = identity_providers.get(&profile.identity_provider) else {
                return Err(GatewayControlPlaneError::UnknownIdentityProvider {
                    profile: profile.id.clone(),
                    identity_provider: profile.identity_provider.clone(),
                });
            };
            let Some(authorization_server) =
                authorization_servers.get(&profile.authorization_server)
            else {
                return Err(GatewayControlPlaneError::UnknownAuthorizationServer {
                    profile: profile.id.clone(),
                    authorization_server: profile.authorization_server.clone(),
                });
            };
            if !policies.contains(&profile.policy_version) {
                return Err(GatewayControlPlaneError::UnknownPolicy {
                    profile: profile.id.clone(),
                    policy_version: profile.policy_version.clone(),
                });
            }
            let mut profile_servers = BTreeSet::new();
            for exposure in &profile.servers {
                if !profile_servers.insert(exposure.server.clone()) {
                    return Err(GatewayControlPlaneError::DuplicateProfileServer {
                        profile: profile.id.clone(),
                        server: exposure.server.clone(),
                    });
                }
                let Some(server) = servers.get(&exposure.server) else {
                    return Err(GatewayControlPlaneError::UnknownServer {
                        profile: profile.id.clone(),
                        server: exposure.server.clone(),
                    });
                };
                validate_profile_server_exposure(profile, exposure, server)?;
            }
            validate_profile_auth_modes(profile, identity_provider, authorization_server)?;
        }

        for policy in &self.policies {
            validate_policy_set(policy, &profiles, &servers, &resource_schemes, &data_labels)?;
        }

        let mut secrets = BTreeSet::new();
        let mut secret_refs = BTreeMap::new();
        for secret in &self.secrets {
            if !secrets.insert(secret.id.clone()) {
                return Err(GatewayControlPlaneError::DuplicateSecret(secret.id.clone()));
            }
            secret_refs.insert(secret.id.clone(), secret);
            match &secret.owner {
                SecretOwner::Gateway | SecretOwner::Tenant { .. } => {}
                SecretOwner::Profile { profile } => {
                    if !profiles.contains(profile) {
                        return Err(GatewayControlPlaneError::UnknownSecretOwnerProfile {
                            secret: secret.id.clone(),
                            profile: profile.clone(),
                        });
                    }
                }
                SecretOwner::Server { server } => {
                    if !server_ids.contains(server) {
                        return Err(GatewayControlPlaneError::UnknownSecretOwnerServer {
                            secret: secret.id.clone(),
                            server: server.clone(),
                        });
                    }
                }
            }
        }
        for server in &self.servers {
            validate_server_upstream_tls_material(server, &secret_refs)?;
        }

        for authorization_server in &self.authorization_servers {
            let Some(secret) = secret_refs.get(&authorization_server.access_token_signing_key)
            else {
                return Err(
                    GatewayControlPlaneError::UnknownAuthorizationServerSigningKey {
                        authorization_server: authorization_server.id.clone(),
                        secret: authorization_server.access_token_signing_key.clone(),
                    },
                );
            };
            if secret.purpose != SecretPurpose::JwksPrivateKey {
                return Err(
                    GatewayControlPlaneError::AuthorizationServerSigningKeyPurposeMismatch {
                        authorization_server: authorization_server.id.clone(),
                        secret: authorization_server.access_token_signing_key.clone(),
                        purpose: secret.purpose,
                    },
                );
            }
        }

        let mut oidc_clients = BTreeSet::new();
        for client in &self.oidc_clients {
            if !oidc_clients.insert(client.id.clone()) {
                return Err(GatewayControlPlaneError::DuplicateOidcClient(
                    client.id.clone(),
                ));
            }
            validate_oidc_client_registration(
                client,
                &identity_providers,
                &authorization_servers,
                &secret_refs,
            )?;
        }

        let mut oauth_clients = BTreeSet::new();
        for client in &self.oauth_clients {
            if !oauth_clients.insert(client.id.clone()) {
                return Err(GatewayControlPlaneError::DuplicateOAuthClient(
                    client.id.clone(),
                ));
            }
            validate_oauth_client_registration(
                client,
                &authorization_servers,
                &profile_by_id,
                &policy_by_id,
                &secret_refs,
            )?;
        }
        for profile in &self.profiles {
            for auth_mode in &profile.auth_modes {
                let required_grant = OAuthGrantType::from(*auth_mode);
                let has_client = self.oauth_clients.iter().any(|client| {
                    client.authorization_server == profile.authorization_server
                        && client.allowed_profiles.contains(&profile.id)
                        && client.grant_types.contains(&required_grant)
                });
                if !has_client {
                    return Err(GatewayControlPlaneError::MissingOAuthClientForAuthMode {
                        profile: profile.id.clone(),
                        auth_mode: *auth_mode,
                    });
                }
            }
            if profile
                .auth_modes
                .contains(&AuthMode::OidcAuthorizationCodePkce)
            {
                let has_oidc_client = self.oidc_clients.iter().any(|client| {
                    client.identity_provider == profile.identity_provider
                        && client.authorization_server == profile.authorization_server
                });
                if !has_oidc_client {
                    return Err(GatewayControlPlaneError::MissingOidcClientForProfile {
                        profile: profile.id.clone(),
                        identity_provider: profile.identity_provider.clone(),
                        authorization_server: profile.authorization_server.clone(),
                    });
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayControlPlaneError {
    DuplicateIdentityProvider(IdentityProviderId),
    DuplicateAuthorizationServer(AuthorizationServerId),
    DuplicateServer(ServerSlug),
    DuplicateResourceScheme(ResourceScheme),
    DuplicateProfile(GatewayProfileId),
    DuplicatePolicy(PolicyVersion),
    DuplicateDataLabel(DataLabelId),
    DuplicateSecret(SecretReferenceId),
    DuplicateOAuthClient(OAuthClientId),
    DuplicateOidcClient(OidcClientRegistrationId),
    DuplicateProfileServer {
        profile: GatewayProfileId,
        server: ServerSlug,
    },
    DuplicatePolicyRule {
        policy: PolicyVersion,
        rule: PolicyRuleId,
    },
    UnknownPolicyRuleProfile {
        policy: PolicyVersion,
        rule: PolicyRuleId,
        profile: GatewayProfileId,
    },
    UnknownPolicyRuleServer {
        policy: PolicyVersion,
        rule: PolicyRuleId,
        server: ServerSlug,
    },
    UnknownPolicyRuleResourceScheme {
        policy: PolicyVersion,
        rule: PolicyRuleId,
        scheme: ResourceScheme,
    },
    PolicyRuleResourceSchemeOutsideServerScope {
        policy: PolicyVersion,
        rule: PolicyRuleId,
        scheme: ResourceScheme,
    },
    UnknownPolicyRuleTool {
        policy: PolicyVersion,
        rule: PolicyRuleId,
        tool: LocalToolName,
    },
    UnknownPolicyRulePrompt {
        policy: PolicyVersion,
        rule: PolicyRuleId,
        prompt: PromptName,
    },
    UnknownPolicyRuleDataLabel {
        policy: PolicyVersion,
        rule: PolicyRuleId,
        label: DataLabelId,
    },
    PolicyRuleActionUnsupportedByServerScope {
        policy: PolicyVersion,
        rule: PolicyRuleId,
        action: GatewayAction,
    },
    ServerUpstreamSecurityMismatch {
        server: ServerSlug,
        security: UpstreamTransportSecurity,
        url: UpstreamUrl,
    },
    ServerUpstreamTlsClientMaterialNotAllowed {
        server: ServerSlug,
        security: UpstreamTransportSecurity,
    },
    ServerUpstreamTlsSecretRequired {
        server: ServerSlug,
        purpose: SecretPurpose,
    },
    UnknownServerUpstreamTlsSecret {
        server: ServerSlug,
        secret: SecretReferenceId,
    },
    ServerUpstreamTlsSecretPurposeMismatch {
        server: ServerSlug,
        secret: SecretReferenceId,
        actual: SecretPurpose,
        expected: SecretPurpose,
    },
    UnknownServer {
        profile: GatewayProfileId,
        server: ServerSlug,
    },
    ProfileExposesDisabledCapability {
        profile: GatewayProfileId,
        server: ServerSlug,
        capability: McpSurfaceCapability,
    },
    UnknownProfileTool {
        profile: GatewayProfileId,
        server: ServerSlug,
        tool: LocalToolName,
    },
    UnknownProfilePrompt {
        profile: GatewayProfileId,
        server: ServerSlug,
        prompt: PromptName,
    },
    ProfileResourceSelectorMismatch {
        profile: GatewayProfileId,
        server: ServerSlug,
        expected_scheme: ResourceScheme,
        selector: ResourceSelector,
    },
    UnknownPolicy {
        profile: GatewayProfileId,
        policy_version: PolicyVersion,
    },
    UnknownIdentityProvider {
        profile: GatewayProfileId,
        identity_provider: IdentityProviderId,
    },
    UnknownAuthorizationServer {
        profile: GatewayProfileId,
        authorization_server: AuthorizationServerId,
    },
    UnknownAuthorizationServerIdentityProvider {
        authorization_server: AuthorizationServerId,
        identity_provider: IdentityProviderId,
    },
    UnknownAuthorizationServerSigningKey {
        authorization_server: AuthorizationServerId,
        secret: SecretReferenceId,
    },
    AuthorizationServerSigningKeyPurposeMismatch {
        authorization_server: AuthorizationServerId,
        secret: SecretReferenceId,
        purpose: SecretPurpose,
    },
    MissingAuthModes {
        profile: GatewayProfileId,
    },
    MissingIdentityProviderEndpoint {
        profile: GatewayProfileId,
        identity_provider: IdentityProviderId,
        endpoint: IdentityProviderEndpoint,
    },
    MissingAuthorizationServerEndpoint {
        profile: GatewayProfileId,
        authorization_server: AuthorizationServerId,
        endpoint: AuthorizationServerEndpoint,
    },
    UnknownSecretOwnerProfile {
        secret: SecretReferenceId,
        profile: GatewayProfileId,
    },
    UnknownSecretOwnerServer {
        secret: SecretReferenceId,
        server: ServerSlug,
    },
    UnknownOAuthClientAuthorizationServer {
        client: OAuthClientId,
        authorization_server: AuthorizationServerId,
    },
    UnknownOAuthClientProfile {
        client: OAuthClientId,
        profile: GatewayProfileId,
    },
    UnknownOAuthClientSecret {
        client: OAuthClientId,
        secret: SecretReferenceId,
    },
    OAuthClientSecretPurposeMismatch {
        client: OAuthClientId,
        secret: SecretReferenceId,
        purpose: SecretPurpose,
    },
    OAuthClientProfileAuthorizationServerMismatch {
        client: OAuthClientId,
        profile: GatewayProfileId,
        client_authorization_server: AuthorizationServerId,
        profile_authorization_server: AuthorizationServerId,
    },
    OAuthClientWithoutAllowedProfiles(OAuthClientId),
    OAuthClientWithoutGrantTypes(OAuthClientId),
    OAuthClientWithoutAuthMethods(OAuthClientId),
    OAuthClientMissingRedirectUris {
        client: OAuthClientId,
        grant_type: OAuthGrantType,
    },
    OAuthClientPublicClientCredentials(OAuthClientId),
    OAuthClientMissingCredentialSecret {
        client: OAuthClientId,
        auth_method: OAuthClientAuthMethod,
    },
    OAuthClientMissingJwks {
        client: OAuthClientId,
        auth_method: OAuthClientAuthMethod,
    },
    OAuthClientMissingAllowedScope {
        client: OAuthClientId,
        profile: GatewayProfileId,
        scope: ScopeName,
    },
    OAuthClientUnsupportedAuthConfiguration {
        client: OAuthClientId,
        grant_types: BTreeSet<OAuthGrantType>,
        auth_methods: BTreeSet<OAuthClientAuthMethod>,
    },
    MissingOAuthClientForAuthMode {
        profile: GatewayProfileId,
        auth_mode: AuthMode,
    },
    UnknownOidcClientIdentityProvider {
        client: OidcClientRegistrationId,
        identity_provider: IdentityProviderId,
    },
    UnknownOidcClientAuthorizationServer {
        client: OidcClientRegistrationId,
        authorization_server: AuthorizationServerId,
    },
    OidcClientAuthorizationServerIdentityProviderMismatch {
        client: OidcClientRegistrationId,
        identity_provider: IdentityProviderId,
        authorization_server: AuthorizationServerId,
        authorization_server_identity_provider: Option<IdentityProviderId>,
    },
    UnknownOidcClientSecret {
        client: OidcClientRegistrationId,
        secret: SecretReferenceId,
    },
    OidcClientSecretPurposeMismatch {
        client: OidcClientRegistrationId,
        secret: SecretReferenceId,
        purpose: SecretPurpose,
    },
    OidcClientMissingOpenIdScope(OidcClientRegistrationId),
    MissingOidcClientForProfile {
        profile: GatewayProfileId,
        identity_provider: IdentityProviderId,
        authorization_server: AuthorizationServerId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpSurfaceCapability {
    Tools,
    Resources,
    ResourceTemplates,
    Prompts,
    Completions,
    Tasks,
}

impl McpSurfaceCapability {
    fn description(self) -> &'static str {
        match self {
            Self::Tools => "tools",
            Self::Resources => "resources",
            Self::ResourceTemplates => "resource templates",
            Self::Prompts => "prompts",
            Self::Completions => "completions",
            Self::Tasks => "tasks",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityProviderEndpoint {
    Authorization,
    Token,
    EnterpriseManagedAuthorization,
}

impl IdentityProviderEndpoint {
    fn description(self) -> &'static str {
        match self {
            Self::Authorization => "authorization endpoint",
            Self::Token => "token endpoint",
            Self::EnterpriseManagedAuthorization => "enterprise-managed authorization endpoint",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationServerEndpoint {
    Authorization,
}

impl AuthorizationServerEndpoint {
    fn description(self) -> &'static str {
        match self {
            Self::Authorization => "authorization endpoint",
        }
    }
}

impl fmt::Display for GatewayControlPlaneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateIdentityProvider(identity_provider) => {
                write!(f, "duplicate identity provider `{identity_provider}`")
            }
            Self::DuplicateAuthorizationServer(authorization_server) => {
                write!(
                    f,
                    "duplicate resource authorization server `{authorization_server}`"
                )
            }
            Self::DuplicateServer(server) => write!(f, "duplicate server manifest `{server}`"),
            Self::DuplicateResourceScheme(scheme) => {
                write!(f, "duplicate server resource scheme `{scheme}`")
            }
            Self::DuplicateProfile(profile) => write!(f, "duplicate gateway profile `{profile}`"),
            Self::DuplicatePolicy(policy) => write!(f, "duplicate policy version `{policy}`"),
            Self::DuplicateDataLabel(label) => write!(f, "duplicate data label `{label}`"),
            Self::DuplicateSecret(secret) => write!(f, "duplicate secret reference `{secret}`"),
            Self::DuplicateOAuthClient(client) => {
                write!(f, "duplicate OAuth client registration `{client}`")
            }
            Self::DuplicateOidcClient(client) => {
                write!(f, "duplicate OIDC client registration `{client}`")
            }
            Self::DuplicateProfileServer { profile, server } => write!(
                f,
                "gateway profile `{profile}` exposes server `{server}` more than once"
            ),
            Self::DuplicatePolicyRule { policy, rule } => {
                write!(f, "duplicate policy rule `{rule}` in policy `{policy}`")
            }
            Self::UnknownPolicyRuleProfile {
                policy,
                rule,
                profile,
            } => write!(
                f,
                "policy `{policy}` rule `{rule}` references unknown profile `{profile}`"
            ),
            Self::UnknownPolicyRuleServer {
                policy,
                rule,
                server,
            } => write!(
                f,
                "policy `{policy}` rule `{rule}` references unknown server `{server}`"
            ),
            Self::UnknownPolicyRuleResourceScheme {
                policy,
                rule,
                scheme,
            } => write!(
                f,
                "policy `{policy}` rule `{rule}` references unknown resource scheme `{scheme}`"
            ),
            Self::PolicyRuleResourceSchemeOutsideServerScope {
                policy,
                rule,
                scheme,
            } => write!(
                f,
                "policy `{policy}` rule `{rule}` references resource scheme `{scheme}` outside its server scope"
            ),
            Self::UnknownPolicyRuleTool { policy, rule, tool } => write!(
                f,
                "policy `{policy}` rule `{rule}` references unknown tool `{tool}` in its server scope"
            ),
            Self::UnknownPolicyRulePrompt {
                policy,
                rule,
                prompt,
            } => write!(
                f,
                "policy `{policy}` rule `{rule}` references unknown prompt `{prompt}` in its server scope"
            ),
            Self::UnknownPolicyRuleDataLabel {
                policy,
                rule,
                label,
            } => write!(
                f,
                "policy `{policy}` rule `{rule}` references unknown data label `{label}`"
            ),
            Self::PolicyRuleActionUnsupportedByServerScope {
                policy,
                rule,
                action,
            } => write!(
                f,
                "policy `{policy}` rule `{rule}` allows action `{action:?}` outside its server capability scope"
            ),
            Self::ServerUpstreamSecurityMismatch {
                server,
                security,
                url,
            } => write!(
                f,
                "server `{server}` upstream URL `{url}` does not satisfy declared transport security `{security:?}`"
            ),
            Self::ServerUpstreamTlsClientMaterialNotAllowed { server, security } => write!(
                f,
                "server `{server}` declares upstream TLS client material but transport security `{security:?}` does not use gateway-managed mutual TLS"
            ),
            Self::ServerUpstreamTlsSecretRequired { server, purpose } => write!(
                f,
                "server `{server}` upstream mutual TLS requires a secret with purpose `{purpose:?}`"
            ),
            Self::UnknownServerUpstreamTlsSecret { server, secret } => write!(
                f,
                "server `{server}` upstream TLS references unknown secret `{secret}`"
            ),
            Self::ServerUpstreamTlsSecretPurposeMismatch {
                server,
                secret,
                actual,
                expected,
            } => write!(
                f,
                "server `{server}` upstream TLS references secret `{secret}` with invalid purpose `{actual:?}`, expected `{expected:?}`"
            ),
            Self::UnknownServer { profile, server } => write!(
                f,
                "gateway profile `{profile}` references unknown server `{server}`"
            ),
            Self::ProfileExposesDisabledCapability {
                profile,
                server,
                capability,
            } => write!(
                f,
                "gateway profile `{profile}` exposes {} for server `{server}`, but the server manifest disables that capability",
                capability.description()
            ),
            Self::UnknownProfileTool {
                profile,
                server,
                tool,
            } => write!(
                f,
                "gateway profile `{profile}` exposes unknown tool `{tool}` for server `{server}`"
            ),
            Self::UnknownProfilePrompt {
                profile,
                server,
                prompt,
            } => write!(
                f,
                "gateway profile `{profile}` exposes unknown prompt `{prompt}` for server `{server}`"
            ),
            Self::ProfileResourceSelectorMismatch {
                profile,
                server,
                expected_scheme,
                selector,
            } => write!(
                f,
                "gateway profile `{profile}` exposes resource selector {} for server `{server}`, expected scheme `{expected_scheme}`",
                resource_selector_description(selector)
            ),
            Self::UnknownPolicy {
                profile,
                policy_version,
            } => write!(
                f,
                "gateway profile `{profile}` references unknown policy `{policy_version}`"
            ),
            Self::UnknownIdentityProvider {
                profile,
                identity_provider,
            } => write!(
                f,
                "gateway profile `{profile}` references unknown identity provider `{identity_provider}`"
            ),
            Self::UnknownAuthorizationServer {
                profile,
                authorization_server,
            } => write!(
                f,
                "gateway profile `{profile}` references unknown resource authorization server `{authorization_server}`"
            ),
            Self::UnknownAuthorizationServerIdentityProvider {
                authorization_server,
                identity_provider,
            } => write!(
                f,
                "resource authorization server `{authorization_server}` references unknown identity provider `{identity_provider}`"
            ),
            Self::UnknownAuthorizationServerSigningKey {
                authorization_server,
                secret,
            } => write!(
                f,
                "resource authorization server `{authorization_server}` references unknown signing key secret `{secret}`"
            ),
            Self::AuthorizationServerSigningKeyPurposeMismatch {
                authorization_server,
                secret,
                purpose,
            } => write!(
                f,
                "resource authorization server `{authorization_server}` references signing key secret `{secret}` with invalid purpose `{purpose:?}`"
            ),
            Self::MissingAuthModes { profile } => {
                write!(f, "gateway profile `{profile}` declares no auth modes")
            }
            Self::MissingIdentityProviderEndpoint {
                profile,
                identity_provider,
                endpoint,
            } => write!(
                f,
                "gateway profile `{profile}` requires {} on identity provider `{identity_provider}`",
                endpoint.description()
            ),
            Self::MissingAuthorizationServerEndpoint {
                profile,
                authorization_server,
                endpoint,
            } => write!(
                f,
                "gateway profile `{profile}` requires {} on resource authorization server `{authorization_server}`",
                endpoint.description()
            ),
            Self::UnknownSecretOwnerProfile { secret, profile } => write!(
                f,
                "secret reference `{secret}` is owned by unknown gateway profile `{profile}`"
            ),
            Self::UnknownSecretOwnerServer { secret, server } => write!(
                f,
                "secret reference `{secret}` is owned by unknown server `{server}`"
            ),
            Self::UnknownOAuthClientAuthorizationServer {
                client,
                authorization_server,
            } => write!(
                f,
                "OAuth client `{client}` references unknown resource authorization server `{authorization_server}`"
            ),
            Self::UnknownOAuthClientProfile { client, profile } => write!(
                f,
                "OAuth client `{client}` references unknown gateway profile `{profile}`"
            ),
            Self::UnknownOAuthClientSecret { client, secret } => write!(
                f,
                "OAuth client `{client}` references unknown secret `{secret}`"
            ),
            Self::OAuthClientSecretPurposeMismatch {
                client,
                secret,
                purpose,
            } => write!(
                f,
                "OAuth client `{client}` references secret `{secret}` with invalid purpose `{purpose:?}`"
            ),
            Self::OAuthClientProfileAuthorizationServerMismatch {
                client,
                profile,
                client_authorization_server,
                profile_authorization_server,
            } => write!(
                f,
                "OAuth client `{client}` uses resource authorization server `{client_authorization_server}` but profile `{profile}` uses `{profile_authorization_server}`"
            ),
            Self::OAuthClientWithoutAllowedProfiles(client) => {
                write!(
                    f,
                    "OAuth client `{client}` does not allow any gateway profile"
                )
            }
            Self::OAuthClientWithoutGrantTypes(client) => {
                write!(f, "OAuth client `{client}` does not declare any grant type")
            }
            Self::OAuthClientWithoutAuthMethods(client) => {
                write!(
                    f,
                    "OAuth client `{client}` does not declare any auth method"
                )
            }
            Self::OAuthClientMissingRedirectUris { client, grant_type } => write!(
                f,
                "OAuth client `{client}` grant `{grant_type:?}` requires at least one redirect URI"
            ),
            Self::OAuthClientPublicClientCredentials(client) => write!(
                f,
                "OAuth client `{client}` cannot use unauthenticated public client credentials"
            ),
            Self::OAuthClientMissingCredentialSecret {
                client,
                auth_method,
            } => write!(
                f,
                "OAuth client `{client}` auth method `{auth_method:?}` requires a credential secret reference"
            ),
            Self::OAuthClientMissingJwks {
                client,
                auth_method,
            } => write!(
                f,
                "OAuth client `{client}` auth method `{auth_method:?}` requires a trusted JWKS source"
            ),
            Self::OAuthClientMissingAllowedScope {
                client,
                profile,
                scope,
            } => write!(
                f,
                "OAuth client `{client}` allows profile `{profile}` but does not allow required scope `{scope}`"
            ),
            Self::OAuthClientUnsupportedAuthConfiguration {
                client,
                grant_types,
                auth_methods,
            } => write!(
                f,
                "OAuth client `{client}` uses unsupported grant/auth method combination: grants `{grant_types:?}`, auth methods `{auth_methods:?}`"
            ),
            Self::MissingOAuthClientForAuthMode { profile, auth_mode } => write!(
                f,
                "gateway profile `{profile}` advertises auth mode `{auth_mode:?}` without a matching OAuth client registration"
            ),
            Self::UnknownOidcClientIdentityProvider {
                client,
                identity_provider,
            } => write!(
                f,
                "OIDC client `{client}` references unknown identity provider `{identity_provider}`"
            ),
            Self::UnknownOidcClientAuthorizationServer {
                client,
                authorization_server,
            } => write!(
                f,
                "OIDC client `{client}` references unknown resource authorization server `{authorization_server}`"
            ),
            Self::OidcClientAuthorizationServerIdentityProviderMismatch {
                client,
                identity_provider,
                authorization_server,
                authorization_server_identity_provider,
            } => write!(
                f,
                "OIDC client `{client}` uses identity provider `{identity_provider}` but resource authorization server `{authorization_server}` is bound to `{authorization_server_identity_provider:?}`"
            ),
            Self::UnknownOidcClientSecret { client, secret } => write!(
                f,
                "OIDC client `{client}` references unknown secret `{secret}`"
            ),
            Self::OidcClientSecretPurposeMismatch {
                client,
                secret,
                purpose,
            } => write!(
                f,
                "OIDC client `{client}` references secret `{secret}` with invalid purpose `{purpose:?}`"
            ),
            Self::OidcClientMissingOpenIdScope(client) => {
                write!(f, "OIDC client `{client}` must request the `openid` scope")
            }
            Self::MissingOidcClientForProfile {
                profile,
                identity_provider,
                authorization_server,
            } => write!(
                f,
                "gateway profile `{profile}` advertises OIDC auth without an OIDC client for identity provider `{identity_provider}` and resource authorization server `{authorization_server}`"
            ),
        }
    }
}

impl std::error::Error for GatewayControlPlaneError {}

#[cfg(test)]
mod tests;
