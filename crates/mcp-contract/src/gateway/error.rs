use std::{collections::BTreeSet, fmt};

use super::validation::resource_selector_description;
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayControlPlaneError {
    DuplicateIdentityProvider(IdentityProviderId),
    DuplicateAuthorizationServer(AuthorizationServerId),
    DuplicateServer(ServerSlug),
    DuplicateResourceScheme(ResourceScheme),
    DuplicateProfile(GatewayProfileId),
    DuplicatePolicy(PolicyVersion),
    DuplicateDataLabel(DataLabelId),
    DuplicateTenant(TenantId),
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
    UnknownPolicyRuleTenant {
        policy: PolicyVersion,
        rule: PolicyRuleId,
        tenant: TenantId,
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
    UnknownSecretOwnerTenant {
        secret: SecretReferenceId,
        tenant: TenantId,
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
            Self::DuplicateTenant(tenant) => write!(f, "duplicate tenant `{tenant}`"),
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
            Self::UnknownPolicyRuleTenant {
                policy,
                rule,
                tenant,
            } => write!(
                f,
                "policy `{policy}` rule `{rule}` references unknown tenant `{tenant}`"
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
            Self::UnknownSecretOwnerTenant { secret, tenant } => write!(
                f,
                "secret reference `{secret}` is owned by unknown tenant `{tenant}`"
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
