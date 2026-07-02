use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    str::FromStr,
};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::{Host, Url};

pub const MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION: &str =
    "io.modelcontextprotocol/enterprise-managed-authorization";
pub const MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION: &str =
    "io.modelcontextprotocol/oauth-client-credentials";

macro_rules! typed_id {
    ($name:ident, $validator:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                let value = value.into();
                $validator(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl FromStr for $name {
            type Err = IdentifierError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value.to_string())
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentifierError {
    value: String,
    rule: &'static str,
}

impl IdentifierError {
    fn new(value: &str, rule: &'static str) -> Self {
        Self {
            value: value.to_string(),
            rule,
        }
    }
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid identifier {:?}: {}", self.value, self.rule)
    }
}

impl std::error::Error for IdentifierError {}

typed_id!(
    ServerSlug,
    validate_path_id,
    "Canonical hosted MCP server id used in manifests, profiles, and gateway routes."
);
typed_id!(
    GatewayProfileId,
    validate_path_id,
    "Gateway profile id exposed under `/mcp/{profile}`."
);
typed_id!(
    IdentityProviderId,
    validate_path_id,
    "Configured identity provider id used by gateway profiles."
);
typed_id!(
    AuthorizationServerId,
    validate_path_id,
    "Resource authorization server id that issues profile-scoped MCP access tokens."
);
typed_id!(
    GatewayToolName,
    validate_gateway_name,
    "Gateway-scoped tool name after server namespace projection."
);
typed_id!(
    LocalToolName,
    validate_gateway_name,
    "Tool name as exposed by one direct MCP server."
);
typed_id!(
    PromptName,
    validate_gateway_name,
    "Prompt name as exposed by one direct MCP server or gateway profile."
);
typed_id!(
    ResourceScheme,
    validate_uri_scheme,
    "Server-owned resource URI scheme, for example `media`."
);
typed_id!(
    ScopeName,
    validate_token_text,
    "OAuth/OIDC scope value. It must not contain whitespace or control characters."
);
typed_id!(
    DataLabelId,
    validate_token_text,
    "Policy data label such as `cui`, `itar`, `pii`, or an IdP-provided clearance label."
);
typed_id!(
    PrincipalId,
    validate_claim_text,
    "Stable authenticated user or service-principal identity."
);
typed_id!(
    TenantId,
    validate_claim_text,
    "Tenant, organization, or customer boundary identifier."
);
typed_id!(
    GroupId,
    validate_claim_text,
    "Identity-provider group identifier used by gateway policy."
);
typed_id!(
    RoleId,
    validate_claim_text,
    "Identity-provider role identifier used by gateway policy."
);
typed_id!(
    PolicyVersion,
    validate_token_text,
    "Immutable policy version identifier emitted with decisions and audit records."
);
typed_id!(
    PolicyRuleId,
    validate_token_text,
    "Policy rule identifier used for decision evidence."
);
typed_id!(
    SecretReferenceId,
    validate_token_text,
    "Reference to a secret managed outside control data."
);
typed_id!(
    ProtectedResourceId,
    validate_claim_text,
    "OAuth protected-resource identifier, usually the gateway profile URL."
);
typed_id!(
    OAuthClientId,
    validate_claim_text,
    "Registered OAuth client id allowed to request gateway-profile tokens."
);
typed_id!(
    TokenIssuer,
    validate_claim_text,
    "Expected token issuer identifier."
);
typed_id!(
    TokenSubject,
    validate_claim_text,
    "Subject claim from an authenticated access token or identity assertion."
);
typed_id!(
    JwtId,
    validate_claim_text,
    "JWT id used for replay protection or revocation tracking."
);
typed_id!(
    TraceId,
    validate_token_text,
    "Request trace/correlation id used in audit and runtime state."
);
typed_id!(
    GatewayTaskId,
    validate_token_text,
    "Gateway task id visible to MCP clients."
);
typed_id!(
    UpstreamTaskId,
    validate_token_text,
    "Task id owned by one hosted upstream MCP server."
);
typed_id!(
    McpMethodName,
    validate_token_text,
    "MCP JSON-RPC method name used in policy and audit events."
);
typed_id!(
    SecretLocator,
    validate_claim_text,
    "External secret locator. This is a reference path, not a secret value."
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayControlPlane {
    pub identity_providers: Vec<IdentityProvider>,
    pub authorization_servers: Vec<ResourceAuthorizationServer>,
    pub servers: Vec<ServerManifest>,
    pub profiles: Vec<GatewayProfile>,
    pub policies: Vec<PolicySet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub oauth_clients: Vec<OAuthClientRegistration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<SecretReference>,
    #[serde(default)]
    pub metadata: Value,
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
            validate_policy_set(policy, &profiles, &servers, &resource_schemes)?;
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

        for authorization_server in &self.authorization_servers {
            let Some(secret) = secret_refs.get(&authorization_server.access_token_signing_key)
            else {
                return Err(GatewayControlPlaneError::UnknownAuthorizationServerSigningKey {
                    authorization_server: authorization_server.id.clone(),
                    secret: authorization_server.access_token_signing_key.clone(),
                });
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
    DuplicateSecret(SecretReferenceId),
    DuplicateOAuthClient(OAuthClientId),
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
    PolicyRuleActionUnsupportedByServerScope {
        policy: PolicyVersion,
        rule: PolicyRuleId,
        action: GatewayAction,
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
    MissingOAuthClientForAuthMode {
        profile: GatewayProfileId,
        auth_mode: AuthMode,
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
            Self::DuplicateSecret(secret) => write!(f, "duplicate secret reference `{secret}`"),
            Self::DuplicateOAuthClient(client) => {
                write!(f, "duplicate OAuth client registration `{client}`")
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
            Self::PolicyRuleActionUnsupportedByServerScope {
                policy,
                rule,
                action,
            } => write!(
                f,
                "policy `{policy}` rule `{rule}` allows action `{action:?}` outside its server capability scope"
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
            Self::MissingOAuthClientForAuthMode { profile, auth_mode } => write!(
                f,
                "gateway profile `{profile}` advertises auth mode `{auth_mode:?}` without a matching OAuth client registration"
            ),
        }
    }
}

impl std::error::Error for GatewayControlPlaneError {}

fn validate_profile_server_exposure(
    profile: &GatewayProfile,
    exposure: &ProfileServerExposure,
    server: &ServerManifest,
) -> Result<(), GatewayControlPlaneError> {
    validate_tool_exposure(profile, exposure, server)?;
    validate_resource_exposure(profile, exposure, server)?;
    validate_prompt_exposure(profile, exposure, server)?;
    if matches!(exposure.completions, CompletionExposure::Enabled) {
        require_server_capability(
            profile,
            server,
            McpSurfaceCapability::Completions,
            server.capabilities.completions,
        )?;
    }
    if matches!(exposure.tasks, TaskExposure::Enabled) {
        require_server_capability(
            profile,
            server,
            McpSurfaceCapability::Tasks,
            server.capabilities.tasks,
        )?;
    }
    Ok(())
}

fn validate_tool_exposure(
    profile: &GatewayProfile,
    exposure: &ProfileServerExposure,
    server: &ServerManifest,
) -> Result<(), GatewayControlPlaneError> {
    match &exposure.tools {
        Exposure::None => {}
        Exposure::All => {
            require_server_capability(
                profile,
                server,
                McpSurfaceCapability::Tools,
                server.capabilities.tools,
            )?;
        }
        Exposure::Listed(tools) => {
            require_server_capability(
                profile,
                server,
                McpSurfaceCapability::Tools,
                server.capabilities.tools,
            )?;
            for tool in tools {
                if !server.tools.is_empty() && !server.tools.iter().any(|known| known == tool) {
                    return Err(GatewayControlPlaneError::UnknownProfileTool {
                        profile: profile.id.clone(),
                        server: exposure.server.clone(),
                        tool: tool.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_resource_exposure(
    profile: &GatewayProfile,
    exposure: &ProfileServerExposure,
    server: &ServerManifest,
) -> Result<(), GatewayControlPlaneError> {
    match &exposure.resources {
        Exposure::None => {}
        Exposure::All => {
            require_any_server_capability(
                profile,
                exposure,
                &[
                    (
                        McpSurfaceCapability::Resources,
                        server.capabilities.resources,
                    ),
                    (
                        McpSurfaceCapability::ResourceTemplates,
                        server.capabilities.resource_templates,
                    ),
                ],
            )?;
        }
        Exposure::Listed(selectors) => {
            for selector in selectors {
                match selector {
                    ResourceSelector::Scheme { scheme } => {
                        require_server_capability(
                            profile,
                            server,
                            McpSurfaceCapability::Resources,
                            server.capabilities.resources,
                        )?;
                        if scheme != &server.uri_scheme {
                            return Err(
                                GatewayControlPlaneError::ProfileResourceSelectorMismatch {
                                    profile: profile.id.clone(),
                                    server: exposure.server.clone(),
                                    expected_scheme: server.uri_scheme.clone(),
                                    selector: selector.clone(),
                                },
                            );
                        }
                    }
                    ResourceSelector::UriPrefix { prefix } => {
                        require_server_capability(
                            profile,
                            server,
                            McpSurfaceCapability::Resources,
                            server.capabilities.resources,
                        )?;
                        if !resource_text_uses_scheme(prefix.as_str(), &server.uri_scheme) {
                            return Err(
                                GatewayControlPlaneError::ProfileResourceSelectorMismatch {
                                    profile: profile.id.clone(),
                                    server: exposure.server.clone(),
                                    expected_scheme: server.uri_scheme.clone(),
                                    selector: selector.clone(),
                                },
                            );
                        }
                    }
                    ResourceSelector::Template { uri_template } => {
                        require_server_capability(
                            profile,
                            server,
                            McpSurfaceCapability::ResourceTemplates,
                            server.capabilities.resource_templates,
                        )?;
                        if !resource_text_uses_scheme(uri_template.as_str(), &server.uri_scheme) {
                            return Err(
                                GatewayControlPlaneError::ProfileResourceSelectorMismatch {
                                    profile: profile.id.clone(),
                                    server: exposure.server.clone(),
                                    expected_scheme: server.uri_scheme.clone(),
                                    selector: selector.clone(),
                                },
                            );
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn validate_prompt_exposure(
    profile: &GatewayProfile,
    exposure: &ProfileServerExposure,
    server: &ServerManifest,
) -> Result<(), GatewayControlPlaneError> {
    match &exposure.prompts {
        Exposure::None => {}
        Exposure::All => {
            require_server_capability(
                profile,
                server,
                McpSurfaceCapability::Prompts,
                server.capabilities.prompts,
            )?;
        }
        Exposure::Listed(prompts) => {
            require_server_capability(
                profile,
                server,
                McpSurfaceCapability::Prompts,
                server.capabilities.prompts,
            )?;
            for prompt in prompts {
                if !server.prompts.is_empty() && !server.prompts.iter().any(|known| known == prompt)
                {
                    return Err(GatewayControlPlaneError::UnknownProfilePrompt {
                        profile: profile.id.clone(),
                        server: exposure.server.clone(),
                        prompt: prompt.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn require_server_capability(
    profile: &GatewayProfile,
    server: &ServerManifest,
    capability: McpSurfaceCapability,
    enabled: bool,
) -> Result<(), GatewayControlPlaneError> {
    if enabled {
        Ok(())
    } else {
        Err(GatewayControlPlaneError::ProfileExposesDisabledCapability {
            profile: profile.id.clone(),
            server: server.slug.clone(),
            capability,
        })
    }
}

fn require_any_server_capability(
    profile: &GatewayProfile,
    exposure: &ProfileServerExposure,
    capabilities: &[(McpSurfaceCapability, bool)],
) -> Result<(), GatewayControlPlaneError> {
    if capabilities.iter().any(|(_, enabled)| *enabled) {
        Ok(())
    } else {
        Err(GatewayControlPlaneError::ProfileExposesDisabledCapability {
            profile: profile.id.clone(),
            server: exposure.server.clone(),
            capability: capabilities
                .first()
                .map(|(capability, _)| *capability)
                .unwrap_or(McpSurfaceCapability::Resources),
        })
    }
}

fn resource_text_uses_scheme(text: &str, scheme: &ResourceScheme) -> bool {
    text.starts_with(&format!("{}://", scheme.as_str()))
}

fn resource_selector_description(selector: &ResourceSelector) -> String {
    match selector {
        ResourceSelector::Scheme { scheme } => format!("scheme `{scheme}`"),
        ResourceSelector::UriPrefix { prefix } => format!("URI prefix `{prefix}`"),
        ResourceSelector::Template { uri_template } => {
            format!("URI template `{uri_template}`")
        }
    }
}

fn validate_policy_set(
    policy: &PolicySet,
    profiles: &BTreeSet<GatewayProfileId>,
    servers: &BTreeMap<ServerSlug, &ServerManifest>,
    resource_schemes: &BTreeSet<ResourceScheme>,
) -> Result<(), GatewayControlPlaneError> {
    let mut rules = BTreeSet::new();
    for rule in &policy.rules {
        if !rules.insert(rule.id.clone()) {
            return Err(GatewayControlPlaneError::DuplicatePolicyRule {
                policy: policy.version.clone(),
                rule: rule.id.clone(),
            });
        }
        for profile in &rule.profiles {
            if !profiles.contains(profile) {
                return Err(GatewayControlPlaneError::UnknownPolicyRuleProfile {
                    policy: policy.version.clone(),
                    rule: rule.id.clone(),
                    profile: profile.clone(),
                });
            }
        }
        for server in &rule.servers {
            if !servers.contains_key(server) {
                return Err(GatewayControlPlaneError::UnknownPolicyRuleServer {
                    policy: policy.version.clone(),
                    rule: rule.id.clone(),
                    server: server.clone(),
                });
            }
        }
        let server_scope = policy_rule_server_scope(rule, servers);
        validate_policy_rule_actions(policy, rule, &server_scope)?;
        for scheme in &rule.resource_schemes {
            if !resource_schemes.contains(scheme) {
                return Err(GatewayControlPlaneError::UnknownPolicyRuleResourceScheme {
                    policy: policy.version.clone(),
                    rule: rule.id.clone(),
                    scheme: scheme.clone(),
                });
            }
            if !rule.servers.is_empty()
                && !server_scope
                    .iter()
                    .any(|server| &server.uri_scheme == scheme)
            {
                return Err(
                    GatewayControlPlaneError::PolicyRuleResourceSchemeOutsideServerScope {
                        policy: policy.version.clone(),
                        rule: rule.id.clone(),
                        scheme: scheme.clone(),
                    },
                );
            }
        }
        for tool in &rule.tools {
            if !server_scope.iter().any(|server| {
                server.tools.is_empty() || server.tools.iter().any(|known| known == tool)
            }) {
                return Err(GatewayControlPlaneError::UnknownPolicyRuleTool {
                    policy: policy.version.clone(),
                    rule: rule.id.clone(),
                    tool: tool.clone(),
                });
            }
        }
        for prompt in &rule.prompts {
            if !server_scope.iter().any(|server| {
                server.prompts.is_empty() || server.prompts.iter().any(|known| known == prompt)
            }) {
                return Err(GatewayControlPlaneError::UnknownPolicyRulePrompt {
                    policy: policy.version.clone(),
                    rule: rule.id.clone(),
                    prompt: prompt.clone(),
                });
            }
        }
    }
    Ok(())
}

fn validate_policy_rule_actions(
    policy: &PolicySet,
    rule: &PolicyRule,
    server_scope: &[&ServerManifest],
) -> Result<(), GatewayControlPlaneError> {
    for action in &rule.actions {
        let supported = if rule.servers.is_empty() {
            server_scope
                .iter()
                .any(|server| server_supports_gateway_action(server, *action))
        } else {
            server_scope
                .iter()
                .all(|server| server_supports_gateway_action(server, *action))
        };
        if !supported {
            return Err(
                GatewayControlPlaneError::PolicyRuleActionUnsupportedByServerScope {
                    policy: policy.version.clone(),
                    rule: rule.id.clone(),
                    action: *action,
                },
            );
        }
    }
    Ok(())
}

fn server_supports_gateway_action(server: &ServerManifest, action: GatewayAction) -> bool {
    match action {
        GatewayAction::ToolsList | GatewayAction::ToolsCall => server.capabilities.tools,
        GatewayAction::ResourcesList | GatewayAction::ResourcesRead => {
            server.capabilities.resources
        }
        GatewayAction::ResourcesTemplatesList => server.capabilities.resource_templates,
        GatewayAction::ResourcesSubscribe | GatewayAction::ResourcesUnsubscribe => {
            server.capabilities.resource_subscriptions
        }
        GatewayAction::PromptsList | GatewayAction::PromptsGet => server.capabilities.prompts,
        GatewayAction::CompletionComplete => server.capabilities.completions,
        GatewayAction::TasksList
        | GatewayAction::TasksGet
        | GatewayAction::TasksResult
        | GatewayAction::TasksCancel => server.capabilities.tasks,
        GatewayAction::ArtifactRead | GatewayAction::UsageRead => server.capabilities.resources,
        GatewayAction::AdminRead | GatewayAction::AdminWrite => true,
    }
}

fn policy_rule_server_scope<'a>(
    rule: &PolicyRule,
    servers: &'a BTreeMap<ServerSlug, &ServerManifest>,
) -> Vec<&'a ServerManifest> {
    if rule.servers.is_empty() {
        servers.values().copied().collect()
    } else {
        rule.servers
            .iter()
            .filter_map(|server| servers.get(server).copied())
            .collect()
    }
}

fn validate_profile_auth_modes(
    profile: &GatewayProfile,
    identity_provider: &IdentityProvider,
    authorization_server: &ResourceAuthorizationServer,
) -> Result<(), GatewayControlPlaneError> {
    if profile.auth_modes.is_empty() {
        return Err(GatewayControlPlaneError::MissingAuthModes {
            profile: profile.id.clone(),
        });
    }
    for auth_mode in &profile.auth_modes {
        match auth_mode {
            AuthMode::OidcAuthorizationCodePkce => {
                require_authorization_server_endpoint(
                    profile,
                    authorization_server,
                    AuthorizationServerEndpoint::Authorization,
                    authorization_server.authorization_endpoint.is_some(),
                )?;
                require_identity_provider_endpoint(
                    profile,
                    identity_provider,
                    IdentityProviderEndpoint::Token,
                    identity_provider.token_endpoint.is_some(),
                )?;
            }
            AuthMode::EnterpriseManagedAuthorization => {
                require_identity_provider_endpoint(
                    profile,
                    identity_provider,
                    IdentityProviderEndpoint::EnterpriseManagedAuthorization,
                    identity_provider
                        .enterprise_managed_authorization_endpoint
                        .is_some(),
                )?;
            }
            AuthMode::OAuthClientCredentials => {}
        }
    }
    Ok(())
}

fn require_identity_provider_endpoint(
    profile: &GatewayProfile,
    identity_provider: &IdentityProvider,
    endpoint: IdentityProviderEndpoint,
    present: bool,
) -> Result<(), GatewayControlPlaneError> {
    if present {
        Ok(())
    } else {
        Err(GatewayControlPlaneError::MissingIdentityProviderEndpoint {
            profile: profile.id.clone(),
            identity_provider: identity_provider.id.clone(),
            endpoint,
        })
    }
}

fn require_authorization_server_endpoint(
    profile: &GatewayProfile,
    authorization_server: &ResourceAuthorizationServer,
    endpoint: AuthorizationServerEndpoint,
    present: bool,
) -> Result<(), GatewayControlPlaneError> {
    if present {
        Ok(())
    } else {
        Err(
            GatewayControlPlaneError::MissingAuthorizationServerEndpoint {
                profile: profile.id.clone(),
                authorization_server: authorization_server.id.clone(),
                endpoint,
            },
        )
    }
}

fn validate_oauth_client_registration(
    client: &OAuthClientRegistration,
    authorization_servers: &BTreeMap<AuthorizationServerId, &ResourceAuthorizationServer>,
    profiles: &BTreeMap<GatewayProfileId, &GatewayProfile>,
    policies: &BTreeMap<PolicyVersion, &PolicySet>,
    secrets: &BTreeMap<SecretReferenceId, &SecretReference>,
) -> Result<(), GatewayControlPlaneError> {
    if !authorization_servers.contains_key(&client.authorization_server) {
        return Err(
            GatewayControlPlaneError::UnknownOAuthClientAuthorizationServer {
                client: client.id.clone(),
                authorization_server: client.authorization_server.clone(),
            },
        );
    }
    if client.allowed_profiles.is_empty() {
        return Err(GatewayControlPlaneError::OAuthClientWithoutAllowedProfiles(
            client.id.clone(),
        ));
    }
    if client.grant_types.is_empty() {
        return Err(GatewayControlPlaneError::OAuthClientWithoutGrantTypes(
            client.id.clone(),
        ));
    }
    if client.auth_methods.is_empty() {
        return Err(GatewayControlPlaneError::OAuthClientWithoutAuthMethods(
            client.id.clone(),
        ));
    }

    for profile_id in &client.allowed_profiles {
        let Some(profile) = profiles.get(profile_id) else {
            return Err(GatewayControlPlaneError::UnknownOAuthClientProfile {
                client: client.id.clone(),
                profile: profile_id.clone(),
            });
        };
        if profile.authorization_server != client.authorization_server {
            return Err(
                GatewayControlPlaneError::OAuthClientProfileAuthorizationServerMismatch {
                    client: client.id.clone(),
                    profile: profile_id.clone(),
                    client_authorization_server: client.authorization_server.clone(),
                    profile_authorization_server: profile.authorization_server.clone(),
                },
            );
        }
        let mut required_scopes = profile
            .required_scopes
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if let Some(policy) = policies.get(&profile.policy_version) {
            for rule in &policy.rules {
                if rule.profiles.is_empty() || rule.profiles.contains(profile_id) {
                    required_scopes.extend(rule.required_scopes.iter().cloned());
                }
            }
        }
        for scope in required_scopes {
            if !client.allowed_scopes.contains(&scope) {
                return Err(GatewayControlPlaneError::OAuthClientMissingAllowedScope {
                    client: client.id.clone(),
                    profile: profile_id.clone(),
                    scope,
                });
            }
        }
    }

    if client
        .grant_types
        .contains(&OAuthGrantType::AuthorizationCodePkce)
        && client.redirect_uris.is_empty()
    {
        return Err(GatewayControlPlaneError::OAuthClientMissingRedirectUris {
            client: client.id.clone(),
            grant_type: OAuthGrantType::AuthorizationCodePkce,
        });
    }
    if client
        .grant_types
        .contains(&OAuthGrantType::EnterpriseManagedAuthorization)
        && client.redirect_uris.is_empty()
    {
        return Err(GatewayControlPlaneError::OAuthClientMissingRedirectUris {
            client: client.id.clone(),
            grant_type: OAuthGrantType::EnterpriseManagedAuthorization,
        });
    }
    if client
        .grant_types
        .contains(&OAuthGrantType::ClientCredentials)
        && client.auth_methods.contains(&OAuthClientAuthMethod::None)
    {
        return Err(
            GatewayControlPlaneError::OAuthClientPublicClientCredentials(client.id.clone()),
        );
    }

    if client
        .auth_methods
        .iter()
        .any(OAuthClientAuthMethod::requires_secret)
    {
        let Some(secret_id) = &client.credential_secret else {
            let auth_method = client
                .auth_methods
                .iter()
                .copied()
                .find(OAuthClientAuthMethod::requires_secret)
                .expect("requires_secret matched");
            return Err(
                GatewayControlPlaneError::OAuthClientMissingCredentialSecret {
                    client: client.id.clone(),
                    auth_method,
                },
            );
        };
        let Some(secret) = secrets.get(secret_id) else {
            return Err(GatewayControlPlaneError::UnknownOAuthClientSecret {
                client: client.id.clone(),
                secret: secret_id.clone(),
            });
        };
        if secret.purpose != SecretPurpose::OAuthClientSecret
            && secret.purpose != SecretPurpose::TokenExchangeCredential
        {
            return Err(GatewayControlPlaneError::OAuthClientSecretPurposeMismatch {
                client: client.id.clone(),
                secret: secret_id.clone(),
                purpose: secret.purpose,
            });
        }
    }

    if client
        .auth_methods
        .iter()
        .any(OAuthClientAuthMethod::requires_jwks)
        && client.jwks.is_none()
    {
        let auth_method = client
            .auth_methods
            .iter()
            .copied()
            .find(OAuthClientAuthMethod::requires_jwks)
            .expect("requires_jwks matched");
        return Err(GatewayControlPlaneError::OAuthClientMissingJwks {
            client: client.id.clone(),
            auth_method,
        });
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct IdentityProvider {
    pub id: IdentityProviderId,
    pub issuer: TokenIssuer,
    pub jwks: JwksSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_endpoint: Option<HttpsUrl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_endpoint: Option<HttpsUrl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enterprise_managed_authorization_endpoint: Option<HttpsUrl>,
    #[serde(default)]
    pub metadata: Value,
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
    pub authorization_endpoint: Option<HttpsUrl>,
    pub token_endpoint: HttpsUrl,
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
pub struct OAuthClientRegistration {
    pub id: OAuthClientId,
    pub authorization_server: AuthorizationServerId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub allowed_profiles: BTreeSet<GatewayProfileId>,
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
    #[serde(default)]
    pub metadata: Value,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum OAuthGrantType {
    AuthorizationCodePkce,
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
    fn requires_secret(&self) -> bool {
        matches!(self, Self::ClientSecretBasic | Self::ClientSecretPost)
    }

    fn requires_jwks(&self) -> bool {
        matches!(self, Self::PrivateKeyJwt)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ServerManifest {
    pub slug: ServerSlug,
    pub uri_scheme: ResourceScheme,
    pub mount_path: MountPath,
    pub mcp_path: MountPath,
    pub upstream: UpstreamEndpoint,
    pub capabilities: McpSurfaceCapabilities,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<LocalToolName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<PromptName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_scopes: Vec<ScopeName>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owned_routes: Vec<OwnedRoute>,
    #[serde(default)]
    pub metadata: Value,
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
pub struct PolicySet {
    pub version: PolicyVersion,
    pub rules: Vec<PolicyRule>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PolicyRule {
    pub id: PolicyRuleId,
    pub effect: PolicyEffect,
    pub actions: BTreeSet<GatewayAction>,
    #[serde(default)]
    pub profiles: BTreeSet<GatewayProfileId>,
    #[serde(default)]
    pub servers: BTreeSet<ServerSlug>,
    #[serde(default)]
    pub tools: BTreeSet<LocalToolName>,
    #[serde(default)]
    pub resource_schemes: BTreeSet<ResourceScheme>,
    #[serde(default)]
    pub prompts: BTreeSet<PromptName>,
    #[serde(default)]
    pub principal_ids: BTreeSet<PrincipalId>,
    #[serde(default)]
    pub tenant_ids: BTreeSet<TenantId>,
    #[serde(default)]
    pub groups: BTreeSet<GroupId>,
    #[serde(default)]
    pub roles: BTreeSet<RoleId>,
    #[serde(default)]
    pub required_scopes: BTreeSet<ScopeName>,
    #[serde(default)]
    pub required_data_labels: BTreeSet<DataLabelId>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PolicyEffect {
    Allow,
    Deny,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GatewayAction {
    ToolsList,
    ToolsCall,
    ResourcesList,
    ResourcesTemplatesList,
    ResourcesRead,
    ResourcesSubscribe,
    ResourcesUnsubscribe,
    PromptsList,
    PromptsGet,
    CompletionComplete,
    TasksList,
    TasksGet,
    TasksResult,
    TasksCancel,
    ArtifactRead,
    UsageRead,
    AdminRead,
    AdminWrite,
}

impl GatewayAction {
    pub fn mcp_method(self) -> Option<&'static str> {
        match self {
            Self::ToolsList => Some("tools/list"),
            Self::ToolsCall => Some("tools/call"),
            Self::ResourcesList => Some("resources/list"),
            Self::ResourcesTemplatesList => Some("resources/templates/list"),
            Self::ResourcesRead => Some("resources/read"),
            Self::ResourcesSubscribe => Some("resources/subscribe"),
            Self::ResourcesUnsubscribe => Some("resources/unsubscribe"),
            Self::PromptsList => Some("prompts/list"),
            Self::PromptsGet => Some("prompts/get"),
            Self::CompletionComplete => Some("completion/complete"),
            Self::TasksList => Some("tasks/list"),
            Self::TasksGet => Some("tasks/get"),
            Self::TasksResult => Some("tasks/result"),
            Self::TasksCancel => Some("tasks/cancel"),
            Self::ArtifactRead | Self::UsageRead | Self::AdminRead | Self::AdminWrite => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Principal {
    pub id: PrincipalId,
    pub kind: PrincipalKind,
    pub issuer: TokenIssuer,
    pub subject: TokenSubject,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantId>,
    #[serde(default)]
    pub groups: BTreeSet<GroupId>,
    #[serde(default)]
    pub roles: BTreeSet<RoleId>,
    #[serde(default)]
    pub scopes: BTreeSet<ScopeName>,
    #[serde(default)]
    pub data_labels: BTreeSet<DataLabelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authenticated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    User,
    Service,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AccessTokenSubject {
    pub issuer: TokenIssuer,
    pub subject: TokenSubject,
    pub audience: ProtectedResourceId,
    #[serde(default)]
    pub scopes: BTreeSet<ScopeName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwt_id: Option<JwtId>,
    pub issued_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_before: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PolicyDecision {
    pub effect: PolicyEffect,
    pub reason: PolicyReasonCode,
    pub evaluated_at: DateTime<Utc>,
    pub profile: GatewayProfileId,
    pub action: GatewayAction,
    pub target: PolicyTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<PrincipalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_version: Option<PolicyVersion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<PolicyRuleId>,
    pub trace_id: TraceId,
}

impl PolicyDecision {
    pub fn deny(
        profile: GatewayProfileId,
        action: GatewayAction,
        target: PolicyTarget,
        reason: PolicyReasonCode,
        trace_id: TraceId,
    ) -> Self {
        Self {
            effect: PolicyEffect::Deny,
            reason,
            evaluated_at: Utc::now(),
            profile,
            action,
            target,
            principal: None,
            tenant: None,
            policy_version: None,
            rule_id: None,
            trace_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PolicyReasonCode {
    PolicyAllow,
    PolicyDeny,
    UnknownProfile,
    UnknownServer,
    UnknownTool,
    UnknownResource,
    UnknownPrompt,
    UnknownTask,
    UnknownArtifact,
    UnknownPrincipal,
    UnknownScope,
    UnknownDataLabel,
    UnknownTokenIssuer,
    MissingScope,
    MissingDataLabel,
    TokenAudienceMismatch,
    TokenExpired,
    TokenNotYetValid,
    ReplayDetected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PolicyTarget {
    Gateway,
    Server {
        server: ServerSlug,
    },
    Tool {
        server: ServerSlug,
        tool: LocalToolName,
    },
    Resource {
        server: ServerSlug,
        uri: ResourceUri,
    },
    Prompt {
        server: ServerSlug,
        prompt: PromptName,
    },
    Task {
        server: ServerSlug,
        gateway_task_id: GatewayTaskId,
    },
    Artifact {
        server: ServerSlug,
        artifact_uri: ResourceUri,
    },
    Usage {
        server: ServerSlug,
        usage_uri: ResourceUri,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuditEvent {
    pub event_id: TraceId,
    pub timestamp: DateTime<Utc>,
    pub trace_id: TraceId,
    pub profile: GatewayProfileId,
    pub method: McpMethodName,
    pub action: GatewayAction,
    pub target: PolicyTarget,
    pub decision: PolicyDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<PrincipalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_issuer: Option<TokenIssuer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthAuditEvent {
    pub event_id: TraceId,
    pub timestamp: DateTime<Utc>,
    pub trace_id: TraceId,
    pub profile: GatewayProfileId,
    pub protected_resource: ProtectedResourceId,
    pub outcome: AuthOutcome,
    pub reason: AuthReasonCode,
    pub method: AuthMethod,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub principal: Option<PrincipalId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<TenantId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_issuer: Option<TokenIssuer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_subject: Option<TokenSubject>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwt_id: Option<JwtId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthOutcome {
    Allow,
    Deny,
}

impl AuthOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    BearerJwt,
    ClientCredentialsPrivateKeyJwt,
}

impl AuthMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BearerJwt => "bearer_jwt",
            Self::ClientCredentialsPrivateKeyJwt => "client_credentials_private_key_jwt",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AuthReasonCode {
    AuthAllow,
    MissingAuthorizationHeader,
    InvalidAuthorizationHeader,
    UnknownIdentityProvider,
    UnknownAuthorizationServer,
    IdentityProviderUnavailable,
    AuthorizationServerUnavailable,
    InvalidAuthConfig,
    InvalidBearerToken,
    InvalidClient,
    UnsupportedGrantType,
    InvalidClientAssertion,
    ClientAssertionReplay,
    InvalidScope,
    TokenSigningKeyUnavailable,
    TokenRevoked,
}

impl AuthReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AuthAllow => "auth_allow",
            Self::MissingAuthorizationHeader => "missing_authorization_header",
            Self::InvalidAuthorizationHeader => "invalid_authorization_header",
            Self::UnknownIdentityProvider => "unknown_identity_provider",
            Self::UnknownAuthorizationServer => "unknown_authorization_server",
            Self::IdentityProviderUnavailable => "identity_provider_unavailable",
            Self::AuthorizationServerUnavailable => "authorization_server_unavailable",
            Self::InvalidAuthConfig => "invalid_auth_config",
            Self::InvalidBearerToken => "invalid_bearer_token",
            Self::InvalidClient => "invalid_client",
            Self::UnsupportedGrantType => "unsupported_grant_type",
            Self::InvalidClientAssertion => "invalid_client_assertion",
            Self::ClientAssertionReplay => "client_assertion_replay",
            Self::InvalidScope => "invalid_scope",
            Self::TokenSigningKeyUnavailable => "token_signing_key_unavailable",
            Self::TokenRevoked => "token_revoked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayJwtRevocation {
    pub profile: GatewayProfileId,
    pub issuer: TokenIssuer,
    pub jwt_id: JwtId,
    pub revoked_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayTaskMapping {
    pub gateway_task_id: GatewayTaskId,
    pub upstream_server: ServerSlug,
    pub upstream_task_id: UpstreamTaskId,
    pub profile: GatewayProfileId,
    pub owner: PrincipalId,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayResourceSubscription {
    pub profile: GatewayProfileId,
    pub owner: PrincipalId,
    pub upstream_server: ServerSlug,
    pub resource_uri: ResourceUri,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
    OAuthClientSecret,
    GatewaySigningKey,
    JwksPrivateKey,
    TokenExchangeCredential,
    ObjectStoreCredential,
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
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamTransport {
    StreamableHttp,
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

fn validate_path_id(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(
            value,
            "must not be empty and must contain lowercase ASCII letters, digits, hyphen, or underscore",
        ));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(IdentifierError::new(
            value,
            "must contain only lowercase ASCII letters, digits, hyphen, or underscore",
        ));
    }
    Ok(())
}

fn validate_gateway_name(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(IdentifierError::new(
            value,
            "must contain only lowercase ASCII letters, digits, hyphen, or underscore",
        ));
    }
    Ok(())
}

fn validate_uri_scheme(value: &str) -> Result<(), IdentifierError> {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return Err(IdentifierError::new(value, "must not be empty"));
    };
    if !first.is_ascii_lowercase() {
        return Err(IdentifierError::new(
            value,
            "must start with a lowercase ASCII letter",
        ));
    }
    if !bytes.all(|b| {
        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'+' || b == b'-' || b == b'.'
    }) {
        return Err(IdentifierError::new(
            value,
            "must follow URI scheme syntax with lowercase ASCII characters",
        ));
    }
    Ok(())
}

fn validate_token_text(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    Ok(())
}

fn validate_claim_text(value: &str) -> Result<(), IdentifierError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty"));
    }
    if value.chars().any(char::is_control) {
        return Err(IdentifierError::new(
            value,
            "must not contain control characters",
        ));
    }
    Ok(())
}

fn validate_mount_path(value: &str) -> Result<(), IdentifierError> {
    if !value.starts_with('/') || value.len() == 1 {
        return Err(IdentifierError::new(
            value,
            "must be an absolute path with at least one segment",
        ));
    }
    if value.ends_with('/') {
        return Err(IdentifierError::new(value, "must not end with slash"));
    }
    if value.contains("//") || value.contains(['?', '#']) {
        return Err(IdentifierError::new(
            value,
            "must not contain empty segments, query, or fragment",
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

fn validate_https_url(value: &str) -> Result<(), IdentifierError> {
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(IdentifierError::new(
            value,
            "must not contain whitespace or control characters",
        ));
    }
    let url = Url::parse(value).map_err(|_| IdentifierError::new(value, "must be a valid URL"))?;
    if url.scheme() != "https" {
        return Err(IdentifierError::new(value, "must use https://"));
    }
    if url.host().is_none() {
        return Err(IdentifierError::new(value, "must include a host"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(IdentifierError::new(value, "must not contain userinfo"));
    }
    if url.fragment().is_some() {
        return Err(IdentifierError::new(value, "must not contain a fragment"));
    }
    Ok(())
}

fn validate_oauth_redirect_uri(value: &str) -> Result<(), IdentifierError> {
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
    if !url.username().is_empty() || url.password().is_some() {
        return Err(IdentifierError::new(value, "must not contain userinfo"));
    }
    if url.fragment().is_some() {
        return Err(IdentifierError::new(value, "must not contain a fragment"));
    }
    match url.scheme() {
        "https" => {
            if url.host().is_none() {
                return Err(IdentifierError::new(value, "must include a host"));
            }
            Ok(())
        }
        "http" => {
            let is_loopback = match url.host() {
                Some(Host::Domain(host)) => host == "localhost",
                Some(Host::Ipv4(addr)) => addr.is_loopback(),
                Some(Host::Ipv6(addr)) => addr.is_loopback(),
                None => false,
            };
            if is_loopback && url.port().is_some_and(|port| port != 0) {
                return Ok(());
            }
            Err(IdentifierError::new(
                value,
                "http:// redirect URIs must use loopback host and explicit non-zero port",
            ))
        }
        _ => Err(IdentifierError::new(
            value,
            "must use https:// or local loopback http://",
        )),
    }
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

fn validate_resource_uri(value: &str) -> Result<(), IdentifierError> {
    let Some((scheme, rest)) = value.split_once("://") else {
        return Err(IdentifierError::new(
            value,
            "must be an absolute server-owned resource URI",
        ));
    };
    validate_uri_scheme(scheme)?;
    if rest.is_empty() || rest.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(IdentifierError::new(
            value,
            "must include a non-empty path and no whitespace/control characters",
        ));
    }
    Ok(())
}

fn validate_uri_template(value: &str) -> Result<(), IdentifierError> {
    validate_resource_uri(value)?;
    if !value.contains('{') || !value.contains('}') {
        return Err(IdentifierError::new(
            value,
            "must include at least one URI-template variable",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_provider() -> IdentityProvider {
        IdentityProvider {
            id: IdentityProviderId::new("enterprise").unwrap(),
            issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
            jwks: JwksSource::Remote {
                jwks_uri: HttpsUrl::new("https://idp.example.com/.well-known/jwks.json").unwrap(),
            },
            authorization_endpoint: Some(
                HttpsUrl::new("https://idp.example.com/oauth2/authorize").unwrap(),
            ),
            token_endpoint: Some(HttpsUrl::new("https://idp.example.com/oauth2/token").unwrap()),
            enterprise_managed_authorization_endpoint: Some(
                HttpsUrl::new("https://idp.example.com/oauth2/id-jag").unwrap(),
            ),
            metadata: Value::Null,
        }
    }

    fn authorization_server() -> ResourceAuthorizationServer {
        ResourceAuthorizationServer {
            id: AuthorizationServerId::new("veoveo").unwrap(),
            issuer: TokenIssuer::new("https://veoveo.bioma.ai/oauth/default").unwrap(),
            jwks: JwksSource::Remote {
                jwks_uri: HttpsUrl::new("https://veoveo.bioma.ai/oauth/default/jwks.json").unwrap(),
            },
            access_token_key_id: JwtId::new("test-key").unwrap(),
            access_token_signing_key: SecretReferenceId::new("veoveo_access_token_private_key")
                .unwrap(),
            identity_provider: Some(IdentityProviderId::new("enterprise").unwrap()),
            authorization_endpoint: Some(
                HttpsUrl::new("https://veoveo.bioma.ai/oauth/default/authorize").unwrap(),
            ),
            token_endpoint: HttpsUrl::new("https://veoveo.bioma.ai/oauth/default/token").unwrap(),
            metadata: Value::Null,
        }
    }

    fn signing_secret() -> SecretReference {
        SecretReference {
            id: SecretReferenceId::new("veoveo_access_token_private_key").unwrap(),
            source: SecretSource::Env,
            purpose: SecretPurpose::JwksPrivateKey,
            locator: SecretLocator::new("VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64")
                .unwrap(),
            owner: SecretOwner::Gateway,
            rotation_hint: None,
            metadata: Value::Null,
        }
    }

    fn default_secrets() -> Vec<SecretReference> {
        vec![signing_secret()]
    }

    fn media_manifest() -> ServerManifest {
        ServerManifest {
            slug: ServerSlug::new("media").unwrap(),
            uri_scheme: ResourceScheme::new("media").unwrap(),
            mount_path: MountPath::new("/media").unwrap(),
            mcp_path: MountPath::new("/media/mcp").unwrap(),
            upstream: UpstreamEndpoint {
                transport: UpstreamTransport::StreamableHttp,
                url: "http://media-mcp:8787/media/mcp".to_string(),
            },
            capabilities: McpSurfaceCapabilities {
                tools: true,
                resources: true,
                resource_templates: true,
                resource_subscriptions: true,
                prompts: true,
                completions: true,
                tasks: true,
                notifications: true,
            },
            tools: vec![LocalToolName::new("run").unwrap()],
            prompts: vec![PromptName::new("model_help").unwrap()],
            required_scopes: vec![ScopeName::new("media:use").unwrap()],
            owned_routes: vec![OwnedRoute {
                path: MountPath::new("/media/webhooks").unwrap(),
                purpose: OwnedRoutePurpose::Webhook,
            }],
            metadata: Value::Null,
        }
    }

    fn default_policy() -> PolicySet {
        PolicySet {
            version: PolicyVersion::new("2026-07-02").unwrap(),
            rules: vec![PolicyRule {
                id: PolicyRuleId::new("allow_media_use").unwrap(),
                effect: PolicyEffect::Allow,
                actions: BTreeSet::from([GatewayAction::ToolsCall]),
                profiles: BTreeSet::from([GatewayProfileId::new("default").unwrap()]),
                servers: BTreeSet::from([ServerSlug::new("media").unwrap()]),
                tools: BTreeSet::from([LocalToolName::new("run").unwrap()]),
                resource_schemes: BTreeSet::new(),
                prompts: BTreeSet::new(),
                principal_ids: BTreeSet::new(),
                tenant_ids: BTreeSet::new(),
                groups: BTreeSet::new(),
                roles: BTreeSet::new(),
                required_scopes: BTreeSet::from([ScopeName::new("media:use").unwrap()]),
                required_data_labels: BTreeSet::new(),
                metadata: Value::Null,
            }],
            metadata: Value::Null,
        }
    }

    fn default_profile() -> GatewayProfile {
        GatewayProfile {
            id: GatewayProfileId::new("default").unwrap(),
            identity_provider: IdentityProviderId::new("enterprise").unwrap(),
            authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
            protected_resource: ProtectedResourceId::new("https://veoveo.bioma.ai/mcp/default")
                .unwrap(),
            policy_version: PolicyVersion::new("2026-07-02").unwrap(),
            auth_modes: BTreeSet::from([
                AuthMode::EnterpriseManagedAuthorization,
                AuthMode::OAuthClientCredentials,
                AuthMode::OidcAuthorizationCodePkce,
            ]),
            required_scopes: vec![ScopeName::new("media:use").unwrap()],
            servers: vec![ProfileServerExposure {
                server: ServerSlug::new("media").unwrap(),
                tools: Exposure::Listed(vec![LocalToolName::new("run").unwrap()]),
                resources: Exposure::Listed(vec![ResourceSelector::Scheme {
                    scheme: ResourceScheme::new("media").unwrap(),
                }]),
                prompts: Exposure::All,
                completions: CompletionExposure::Enabled,
                tasks: TaskExposure::Enabled,
            }],
            metadata: Value::Null,
        }
    }

    fn default_oauth_clients() -> Vec<OAuthClientRegistration> {
        vec![
            OAuthClientRegistration {
                id: OAuthClientId::new("veoveo-browser").unwrap(),
                authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
                display_name: Some("Veoveo browser client".to_string()),
                allowed_profiles: BTreeSet::from([GatewayProfileId::new("default").unwrap()]),
                grant_types: BTreeSet::from([
                    OAuthGrantType::AuthorizationCodePkce,
                    OAuthGrantType::EnterpriseManagedAuthorization,
                ]),
                auth_methods: BTreeSet::from([OAuthClientAuthMethod::None]),
                redirect_uris: vec![
                    OAuthRedirectUri::new("https://veoveo.bioma.ai/oauth/callback").unwrap(),
                    OAuthRedirectUri::new("http://127.0.0.1:8789/oauth/callback").unwrap(),
                ],
                allowed_scopes: BTreeSet::from([
                    ScopeName::new("media:use").unwrap(),
                    ScopeName::new("gateway:admin").unwrap(),
                ]),
                credential_secret: None,
                jwks: None,
                metadata: Value::Null,
            },
            OAuthClientRegistration {
                id: OAuthClientId::new("veoveo-headless").unwrap(),
                authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
                display_name: Some("Veoveo headless client".to_string()),
                allowed_profiles: BTreeSet::from([GatewayProfileId::new("default").unwrap()]),
                grant_types: BTreeSet::from([OAuthGrantType::ClientCredentials]),
                auth_methods: BTreeSet::from([OAuthClientAuthMethod::PrivateKeyJwt]),
                redirect_uris: vec![],
                allowed_scopes: BTreeSet::from([
                    ScopeName::new("media:use").unwrap(),
                    ScopeName::new("gateway:admin").unwrap(),
                ]),
                credential_secret: None,
                jwks: Some(JwksSource::Remote {
                    jwks_uri: HttpsUrl::new("https://idp.example.com/oauth2/clients/jwks.json")
                        .unwrap(),
                }),
                metadata: Value::Null,
            },
        ]
    }

    #[test]
    fn identifiers_reject_invalid_wire_values() {
        assert!(ServerSlug::new("Media").is_err());
        assert!(GatewayProfileId::new("default/profile").is_err());
        assert!(ResourceScheme::new("1media").is_err());
        assert!(MountPath::new("media").is_err());
        assert!(OAuthRedirectUri::new("https://veoveo.bioma.ai/oauth/callback").is_ok());
        assert!(OAuthRedirectUri::new("http://127.0.0.1:8789/oauth/callback").is_ok());
        assert!(OAuthRedirectUri::new("http://[::1]:8789/oauth/callback").is_ok());
        assert!(OAuthRedirectUri::new("http://example.com/oauth/callback").is_err());
        assert!(OAuthRedirectUri::new("http://127.0.0.1/oauth/callback").is_err());
        assert!(OAuthRedirectUri::new("http://127.0.0.1:0/oauth/callback").is_err());
        assert!(ResourceUri::new("media://artifact/abc").is_ok());
        assert!(ResourceUriTemplate::new("media://model").is_err());
    }

    #[test]
    fn auth_modes_expose_mcp_extension_ids() {
        assert_eq!(
            AuthMode::EnterpriseManagedAuthorization.mcp_extension_id(),
            Some(MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION)
        );
        assert_eq!(
            AuthMode::OAuthClientCredentials.mcp_extension_id(),
            Some(MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION)
        );
        assert_eq!(AuthMode::OidcAuthorizationCodePkce.mcp_extension_id(), None);
    }

    #[test]
    fn gateway_actions_expose_subscription_mcp_methods() {
        assert_eq!(
            GatewayAction::ResourcesSubscribe.mcp_method(),
            Some("resources/subscribe")
        );
        assert_eq!(
            GatewayAction::ResourcesUnsubscribe.mcp_method(),
            Some("resources/unsubscribe")
        );
    }

    #[test]
    fn control_plane_validates_cross_references() {
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: vec![
                signing_secret(),
                SecretReference {
                    id: SecretReferenceId::new("media_provider_key").unwrap(),
                    source: SecretSource::Env,
                    purpose: SecretPurpose::ProviderApiKey,
                    locator: SecretLocator::new("MEDIA_PROVIDER_API_KEY").unwrap(),
                    owner: SecretOwner::Server {
                        server: ServerSlug::new("media").unwrap(),
                    },
                    rotation_hint: None,
                    metadata: Value::Null,
                },
            ],
            metadata: Value::Null,
        };

        config.validate().expect("valid gateway control plane");
    }

    #[test]
    fn control_plane_rejects_unknown_server_reference() {
        let mut profile = default_profile();
        profile.servers[0].server = ServerSlug::new("simulation").unwrap();
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![profile],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config.validate().expect_err("unknown server must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownServer { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_duplicate_profile_server_reference() {
        let mut profile = default_profile();
        profile.servers.push(profile.servers[0].clone());
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![profile],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("duplicate profile server exposure must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::DuplicateProfileServer { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_unknown_profile_tool() {
        let mut profile = default_profile();
        profile.servers[0].tools = Exposure::Listed(vec![LocalToolName::new("simulate").unwrap()]);
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![profile],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("unknown profile tool must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownProfileTool { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_unknown_profile_prompt() {
        let mut profile = default_profile();
        profile.servers[0].prompts =
            Exposure::Listed(vec![PromptName::new("unknown-prompt").unwrap()]);
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![profile],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("unknown profile prompt must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownProfilePrompt { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_profile_resource_scheme_mismatch() {
        let mut profile = default_profile();
        profile.servers[0].resources = Exposure::Listed(vec![ResourceSelector::Scheme {
            scheme: ResourceScheme::new("simulation").unwrap(),
        }]);
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![profile],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("profile resource selector must stay server-scoped");

        assert!(matches!(
            err,
            GatewayControlPlaneError::ProfileResourceSelectorMismatch { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_disabled_profile_capability() {
        let mut manifest = media_manifest();
        manifest.capabilities.tasks = false;
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![manifest],
            profiles: vec![default_profile()],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("profile cannot expose disabled task capability");

        assert!(matches!(
            err,
            GatewayControlPlaneError::ProfileExposesDisabledCapability {
                capability: McpSurfaceCapability::Tasks,
                ..
            }
        ));
    }

    #[test]
    fn control_plane_rejects_unknown_identity_provider_reference() {
        let mut profile = default_profile();
        profile.identity_provider = IdentityProviderId::new("missing").unwrap();
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![profile],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("unknown identity provider must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownIdentityProvider { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_unknown_authorization_server_reference() {
        let mut profile = default_profile();
        profile.authorization_server = AuthorizationServerId::new("missing").unwrap();
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![profile],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("unknown authorization server must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownAuthorizationServer { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_authorization_server_unknown_identity_provider() {
        let mut authorization_server = authorization_server();
        authorization_server.identity_provider = Some(IdentityProviderId::new("missing").unwrap());
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("authorization server IdP reference must be known");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownAuthorizationServerIdentityProvider { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_profile_without_auth_modes() {
        let mut profile = default_profile();
        profile.auth_modes.clear();
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![profile],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config.validate().expect_err("empty auth modes must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::MissingAuthModes { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_oidc_profile_without_browser_endpoints() {
        let mut auth_server = authorization_server();
        auth_server.authorization_endpoint = None;
        let mut profile = default_profile();
        profile.auth_modes = BTreeSet::from([AuthMode::OidcAuthorizationCodePkce]);
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![auth_server],
            servers: vec![media_manifest()],
            profiles: vec![profile],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("OIDC browser auth requires authorization endpoint");

        assert!(matches!(
            err,
            GatewayControlPlaneError::MissingAuthorizationServerEndpoint {
                endpoint: AuthorizationServerEndpoint::Authorization,
                ..
            }
        ));

        let mut idp = identity_provider();
        idp.token_endpoint = None;
        let mut profile = default_profile();
        profile.auth_modes = BTreeSet::from([AuthMode::OidcAuthorizationCodePkce]);
        let config = GatewayControlPlane {
            identity_providers: vec![idp],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![profile],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("OIDC browser auth requires token endpoint");

        assert!(matches!(
            err,
            GatewayControlPlaneError::MissingIdentityProviderEndpoint {
                endpoint: IdentityProviderEndpoint::Token,
                ..
            }
        ));
    }

    #[test]
    fn control_plane_rejects_extension_auth_modes_without_matching_endpoints() {
        let mut idp = identity_provider();
        idp.enterprise_managed_authorization_endpoint = None;
        let mut profile = default_profile();
        profile.auth_modes = BTreeSet::from([AuthMode::EnterpriseManagedAuthorization]);
        let config = GatewayControlPlane {
            identity_providers: vec![idp],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![profile],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("enterprise-managed auth requires endpoint");

        assert!(matches!(
            err,
            GatewayControlPlaneError::MissingIdentityProviderEndpoint {
                endpoint: IdentityProviderEndpoint::EnterpriseManagedAuthorization,
                ..
            }
        ));
    }

    #[test]
    fn control_plane_rejects_unknown_secret_owner_references() {
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: vec![
                signing_secret(),
                SecretReference {
                    id: SecretReferenceId::new("profile_secret").unwrap(),
                    source: SecretSource::Env,
                    purpose: SecretPurpose::OAuthClientSecret,
                    locator: SecretLocator::new("PROFILE_SECRET").unwrap(),
                    owner: SecretOwner::Profile {
                        profile: GatewayProfileId::new("missing").unwrap(),
                    },
                    rotation_hint: None,
                    metadata: Value::Null,
                },
            ],
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("unknown profile secret owner must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownSecretOwnerProfile { .. }
        ));

        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: vec![
                signing_secret(),
                SecretReference {
                    id: SecretReferenceId::new("server_secret").unwrap(),
                    source: SecretSource::Env,
                    purpose: SecretPurpose::ProviderApiKey,
                    locator: SecretLocator::new("SERVER_SECRET").unwrap(),
                    owner: SecretOwner::Server {
                        server: ServerSlug::new("missing").unwrap(),
                    },
                    rotation_hint: None,
                    metadata: Value::Null,
                },
            ],
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("unknown server secret owner must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownSecretOwnerServer { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_missing_oauth_client_for_auth_mode() {
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![default_policy()],
            oauth_clients: vec![],
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("profile auth modes require OAuth clients");

        assert!(matches!(
            err,
            GatewayControlPlaneError::MissingOAuthClientForAuthMode { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_public_client_credentials() {
        let mut clients = default_oauth_clients();
        clients[1].auth_methods = BTreeSet::from([OAuthClientAuthMethod::None]);
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![default_policy()],
            oauth_clients: clients,
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("client credentials must not be public");

        assert!(matches!(
            err,
            GatewayControlPlaneError::OAuthClientPublicClientCredentials(_)
        ));
    }

    #[test]
    fn control_plane_rejects_private_key_jwt_without_jwks() {
        let mut clients = default_oauth_clients();
        clients[1].jwks = None;
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![default_policy()],
            oauth_clients: clients,
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("private-key JWT clients require a JWKS source");

        assert!(matches!(
            err,
            GatewayControlPlaneError::OAuthClientMissingJwks { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_oauth_client_missing_required_scope() {
        let mut clients = default_oauth_clients();
        clients[0]
            .allowed_scopes
            .remove(&ScopeName::new("media:use").unwrap());
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![default_policy()],
            oauth_clients: clients,
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("client allowed scopes must cover profile and policy scopes");

        assert!(matches!(
            err,
            GatewayControlPlaneError::OAuthClientMissingAllowedScope { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_duplicate_resource_schemes() {
        let mut second_server = media_manifest();
        second_server.slug = ServerSlug::new("simulation").unwrap();
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest(), second_server],
            profiles: vec![default_profile()],
            policies: vec![default_policy()],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("duplicate resource schemes must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::DuplicateResourceScheme(_)
        ));
    }

    #[test]
    fn control_plane_rejects_duplicate_policy_rule_ids() {
        let mut policy = default_policy();
        policy.rules.push(policy.rules[0].clone());
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![policy],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("duplicate policy rule ids must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::DuplicatePolicyRule { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_unknown_policy_rule_references() {
        let mut policy = default_policy();
        policy.rules[0].profiles = BTreeSet::from([GatewayProfileId::new("missing").unwrap()]);
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![policy],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("unknown policy profile must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownPolicyRuleProfile { .. }
        ));

        let mut policy = default_policy();
        policy.rules[0].servers = BTreeSet::from([ServerSlug::new("missing").unwrap()]);
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![policy],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("unknown policy server must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownPolicyRuleServer { .. }
        ));

        let mut policy = default_policy();
        policy.rules[0].resource_schemes =
            BTreeSet::from([ResourceScheme::new("simulation").unwrap()]);
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![policy],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("unknown policy resource scheme must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownPolicyRuleResourceScheme { .. }
        ));

        let mut policy = default_policy();
        policy.rules[0].tools = BTreeSet::from([LocalToolName::new("simulate").unwrap()]);
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![policy],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("unknown policy tool must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownPolicyRuleTool { .. }
        ));

        let mut policy = default_policy();
        policy.rules[0].prompts = BTreeSet::from([PromptName::new("unknown-prompt").unwrap()]);
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![default_profile()],
            policies: vec![policy],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("unknown policy prompt must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::UnknownPolicyRulePrompt { .. }
        ));

        let mut simulation_server = media_manifest();
        simulation_server.slug = ServerSlug::new("simulation").unwrap();
        simulation_server.uri_scheme = ResourceScheme::new("simulation").unwrap();
        let mut policy = default_policy();
        policy.rules[0].resource_schemes =
            BTreeSet::from([ResourceScheme::new("simulation").unwrap()]);
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest(), simulation_server],
            profiles: vec![default_profile()],
            policies: vec![policy],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("policy resource scheme outside server scope must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::PolicyRuleResourceSchemeOutsideServerScope { .. }
        ));
    }

    #[test]
    fn control_plane_rejects_policy_action_outside_server_capabilities() {
        let mut manifest = media_manifest();
        manifest.capabilities.resource_subscriptions = false;
        let mut policy = default_policy();
        policy.rules[0].actions = BTreeSet::from([GatewayAction::ResourcesUnsubscribe]);
        policy.rules[0].tools.clear();
        policy.rules[0].resource_schemes = BTreeSet::from([ResourceScheme::new("media").unwrap()]);
        let config = GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![manifest],
            profiles: vec![default_profile()],
            policies: vec![policy],
            oauth_clients: default_oauth_clients(),
            secrets: default_secrets(),
            metadata: Value::Null,
        };

        let err = config
            .validate()
            .expect_err("policy action outside server capabilities must fail");

        assert!(matches!(
            err,
            GatewayControlPlaneError::PolicyRuleActionUnsupportedByServerScope {
                action: GatewayAction::ResourcesUnsubscribe,
                ..
            }
        ));
    }

    #[test]
    fn policy_decision_defaults_to_explicit_deny() {
        let decision = PolicyDecision::deny(
            GatewayProfileId::new("default").unwrap(),
            GatewayAction::ToolsCall,
            PolicyTarget::Tool {
                server: ServerSlug::new("media").unwrap(),
                tool: LocalToolName::new("run").unwrap(),
            },
            PolicyReasonCode::MissingScope,
            TraceId::new("trace-1").unwrap(),
        );

        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::MissingScope);
    }
}
