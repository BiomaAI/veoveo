use std::collections::{BTreeMap, BTreeSet};

use url::{Host, Url};

use super::*;

pub(super) fn validate_profile_server_exposure(
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

pub(super) fn validate_server_upstream(
    server: &ServerManifest,
) -> Result<(), GatewayControlPlaneError> {
    if upstream_security_allows_url(server.upstream.security, &server.upstream.url) {
        Ok(())
    } else {
        Err(GatewayControlPlaneError::ServerUpstreamSecurityMismatch {
            server: server.slug.clone(),
            security: server.upstream.security,
            url: server.upstream.url.clone(),
        })
    }
}

pub(super) fn validate_server_upstream_tls_material(
    server: &ServerManifest,
    secrets: &BTreeMap<SecretReferenceId, &SecretReference>,
) -> Result<(), GatewayControlPlaneError> {
    let has_client_material = server.upstream.client_certificate.is_some()
        || server.upstream.client_private_key.is_some();
    if server.upstream.security != UpstreamTransportSecurity::MutualTls {
        if has_client_material {
            return Err(
                GatewayControlPlaneError::ServerUpstreamTlsClientMaterialNotAllowed {
                    server: server.slug.clone(),
                    security: server.upstream.security,
                },
            );
        }
        return Ok(());
    }

    validate_server_upstream_tls_secret(
        server,
        server.upstream.client_certificate.as_ref(),
        SecretPurpose::TlsClientCertificate,
    )?;
    validate_server_upstream_tls_secret(
        server,
        server.upstream.client_private_key.as_ref(),
        SecretPurpose::TlsClientPrivateKey,
    )?;

    for (secret_id, expected) in [
        (
            server
                .upstream
                .client_certificate
                .as_ref()
                .expect("client certificate required by prior validation"),
            SecretPurpose::TlsClientCertificate,
        ),
        (
            server
                .upstream
                .client_private_key
                .as_ref()
                .expect("client private key required by prior validation"),
            SecretPurpose::TlsClientPrivateKey,
        ),
    ] {
        let Some(secret) = secrets.get(secret_id) else {
            return Err(GatewayControlPlaneError::UnknownServerUpstreamTlsSecret {
                server: server.slug.clone(),
                secret: secret_id.clone(),
            });
        };
        if secret.purpose != expected {
            return Err(
                GatewayControlPlaneError::ServerUpstreamTlsSecretPurposeMismatch {
                    server: server.slug.clone(),
                    secret: secret_id.clone(),
                    actual: secret.purpose,
                    expected,
                },
            );
        }
    }

    Ok(())
}

fn validate_server_upstream_tls_secret(
    server: &ServerManifest,
    secret: Option<&SecretReferenceId>,
    purpose: SecretPurpose,
) -> Result<(), GatewayControlPlaneError> {
    if secret.is_some() {
        Ok(())
    } else {
        Err(GatewayControlPlaneError::ServerUpstreamTlsSecretRequired {
            server: server.slug.clone(),
            purpose,
        })
    }
}

fn upstream_security_allows_url(security: UpstreamTransportSecurity, url: &UpstreamUrl) -> bool {
    let Ok(parsed) = url.parsed() else {
        return false;
    };
    match security {
        UpstreamTransportSecurity::LoopbackHttp => {
            parsed.scheme() == "http" && url_host_is_loopback(&parsed)
        }
        UpstreamTransportSecurity::ComposeInternalHttp => {
            parsed.scheme() == "http" && url_host_is_single_label_service_name(&parsed)
        }
        UpstreamTransportSecurity::Tls | UpstreamTransportSecurity::MutualTls => {
            parsed.scheme() == "https"
        }
        UpstreamTransportSecurity::ServiceMeshMtls => match parsed.scheme() {
            "https" => true,
            "http" => url_host_is_mesh_internal_name(&parsed),
            _ => false,
        },
    }
}

fn url_host_is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(host)) => host == "localhost",
        Some(Host::Ipv4(addr)) => addr.is_loopback(),
        Some(Host::Ipv6(addr)) => addr.is_loopback(),
        None => false,
    }
}

fn url_host_is_single_label_service_name(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(host)) => {
            host != "localhost"
                && !host.contains('.')
                && host.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'-' | b'_')
                })
        }
        Some(Host::Ipv4(_)) | Some(Host::Ipv6(_)) | None => false,
    }
}

fn url_host_is_mesh_internal_name(url: &Url) -> bool {
    if url_host_is_single_label_service_name(url) {
        return true;
    }
    match url.host() {
        Some(Host::Domain(host)) => {
            host.ends_with(".svc")
                || host.ends_with(".svc.cluster.local")
                || host.ends_with(".internal")
        }
        Some(Host::Ipv4(_)) | Some(Host::Ipv6(_)) | None => false,
    }
}

fn resource_text_uses_scheme(text: &str, scheme: &ResourceScheme) -> bool {
    text.starts_with(&format!("{}://", scheme.as_str()))
}

pub(super) fn resource_selector_description(selector: &ResourceSelector) -> String {
    match selector {
        ResourceSelector::Scheme { scheme } => format!("scheme `{scheme}`"),
        ResourceSelector::UriPrefix { prefix } => format!("URI prefix `{prefix}`"),
        ResourceSelector::Template { uri_template } => {
            format!("URI template `{uri_template}`")
        }
    }
}

pub(super) fn validate_policy_set(
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

pub(super) fn validate_profile_auth_modes(
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

pub(super) fn validate_oauth_client_registration(
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
    validate_oauth_client_auth_configuration(client)?;

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

fn validate_oauth_client_auth_configuration(
    client: &OAuthClientRegistration,
) -> Result<(), GatewayControlPlaneError> {
    let browser_grants = client
        .grant_types
        .iter()
        .any(|grant| matches!(grant, OAuthGrantType::AuthorizationCodePkce));
    let enterprise_managed_grants = client
        .grant_types
        .iter()
        .any(|grant| matches!(grant, OAuthGrantType::EnterpriseManagedAuthorization));
    let client_credentials_grants = client
        .grant_types
        .contains(&OAuthGrantType::ClientCredentials);

    let expected = if client_credentials_grants {
        BTreeSet::from([OAuthClientAuthMethod::PrivateKeyJwt])
    } else if browser_grants || enterprise_managed_grants {
        BTreeSet::from([OAuthClientAuthMethod::None])
    } else {
        BTreeSet::new()
    };

    if expected == client.auth_methods
        && !(client_credentials_grants && (browser_grants || enterprise_managed_grants))
    {
        return Ok(());
    }

    Err(
        GatewayControlPlaneError::OAuthClientUnsupportedAuthConfiguration {
            client: client.id.clone(),
            grant_types: client.grant_types.clone(),
            auth_methods: client.auth_methods.clone(),
        },
    )
}

pub(super) fn validate_oidc_client_registration(
    client: &IdentityProviderOidcClientRegistration,
    identity_providers: &BTreeMap<IdentityProviderId, &IdentityProvider>,
    authorization_servers: &BTreeMap<AuthorizationServerId, &ResourceAuthorizationServer>,
    secrets: &BTreeMap<SecretReferenceId, &SecretReference>,
) -> Result<(), GatewayControlPlaneError> {
    if !identity_providers.contains_key(&client.identity_provider) {
        return Err(
            GatewayControlPlaneError::UnknownOidcClientIdentityProvider {
                client: client.id.clone(),
                identity_provider: client.identity_provider.clone(),
            },
        );
    }
    let Some(authorization_server) = authorization_servers.get(&client.authorization_server) else {
        return Err(
            GatewayControlPlaneError::UnknownOidcClientAuthorizationServer {
                client: client.id.clone(),
                authorization_server: client.authorization_server.clone(),
            },
        );
    };
    if authorization_server.identity_provider.as_ref() != Some(&client.identity_provider) {
        return Err(
            GatewayControlPlaneError::OidcClientAuthorizationServerIdentityProviderMismatch {
                client: client.id.clone(),
                identity_provider: client.identity_provider.clone(),
                authorization_server: client.authorization_server.clone(),
                authorization_server_identity_provider: authorization_server
                    .identity_provider
                    .clone(),
            },
        );
    }
    let Some(secret) = secrets.get(&client.credential_secret) else {
        return Err(GatewayControlPlaneError::UnknownOidcClientSecret {
            client: client.id.clone(),
            secret: client.credential_secret.clone(),
        });
    };
    if secret.purpose != SecretPurpose::OAuthClientSecret {
        return Err(GatewayControlPlaneError::OidcClientSecretPurposeMismatch {
            client: client.id.clone(),
            secret: client.credential_secret.clone(),
            purpose: secret.purpose,
        });
    }
    if !client
        .scopes
        .contains(&ScopeName::new("openid").expect("valid literal"))
    {
        return Err(GatewayControlPlaneError::OidcClientMissingOpenIdScope(
            client.id.clone(),
        ));
    }
    Ok(())
}
