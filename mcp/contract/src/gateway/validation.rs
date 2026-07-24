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

pub(super) fn validate_server_compatibility_helpers(
    server: &ServerManifest,
) -> Result<(), GatewayControlPlaneError> {
    for helper in &server.compatibility_helpers {
        if !server.tools.iter().any(|tool| tool == helper) {
            return Err(GatewayControlPlaneError::UnknownServerCompatibilityHelper {
                server: server.slug.clone(),
                tool: helper.clone(),
            });
        }
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
                        if !resource_text_belongs_to_server(prefix.as_str(), server) {
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
                        if !resource_text_belongs_to_server(uri_template.as_str(), server) {
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

pub(super) fn validate_server_apps(
    server: &ServerManifest,
) -> Result<(), GatewayControlPlaneError> {
    if server.capabilities.apps
        && !(server.capabilities.resources
            && server.resource_projection == ResourceProjectionMode::ServerOwned)
    {
        return Err(GatewayControlPlaneError::ServerAppsRequireOwnedResources(
            server.slug.clone(),
        ));
    }
    Ok(())
}

pub(super) fn validate_server_capabilities(
    server: &ServerManifest,
) -> Result<(), GatewayControlPlaneError> {
    let invalid = if server.capabilities.tools_list_changed && !server.capabilities.tools {
        Some("tools_list_changed requires tools")
    } else if server.capabilities.prompts_list_changed && !server.capabilities.prompts {
        Some("prompts_list_changed requires prompts")
    } else if server.capabilities.resources_list_changed && !server.capabilities.resources {
        Some("resources_list_changed requires resources")
    } else if server.capabilities.resource_subscriptions && !server.capabilities.resources {
        Some("resource_subscriptions requires resources")
    } else {
        None
    };
    match invalid {
        Some(reason) => Err(GatewayControlPlaneError::InvalidServerCapabilities {
            server: server.slug.clone(),
            reason,
        }),
        None => Ok(()),
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
        UpstreamTransportSecurity::ClusterInternalHttp => {
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

fn resource_text_belongs_to_server(text: &str, server: &ServerManifest) -> bool {
    resource_text_uses_scheme(text, &server.uri_scheme)
        || (server.resource_projection == ResourceProjectionMode::ServerOwned
            && text.starts_with(&format!("ui://{}/", server.slug.as_str())))
}

fn server_owns_policy_resource_scheme(server: &ServerManifest, scheme: &ResourceScheme) -> bool {
    scheme == &server.uri_scheme
        || (server.resource_projection == ResourceProjectionMode::ServerOwned
            && scheme.as_str() == "ui")
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
    protected_resources: &BTreeSet<ProtectedResourceId>,
    servers: &BTreeMap<ServerSlug, &ServerManifest>,
    resource_schemes: &BTreeSet<ResourceScheme>,
    data_labels: &BTreeSet<DataLabelId>,
    tenants: &BTreeSet<TenantId>,
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
        for resource in &rule.protected_resources {
            if !protected_resources.contains(resource) {
                return Err(
                    GatewayControlPlaneError::UnknownPolicyRuleProtectedResource {
                        policy: policy.version.clone(),
                        rule: rule.id.clone(),
                        resource: resource.clone(),
                    },
                );
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
                    .any(|server| server_owns_policy_resource_scheme(server, scheme))
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
        for label in &rule.required_data_labels {
            if !data_labels.contains(label) {
                return Err(GatewayControlPlaneError::UnknownPolicyRuleDataLabel {
                    policy: policy.version.clone(),
                    rule: rule.id.clone(),
                    label: label.clone(),
                });
            }
        }
        for tenant in &rule.tenant_ids {
            if !tenants.contains(tenant) {
                return Err(GatewayControlPlaneError::UnknownPolicyRuleTenant {
                    policy: policy.version.clone(),
                    rule: rule.id.clone(),
                    tenant: tenant.clone(),
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
        if action.is_recording_ingest() {
            if rule.protected_resources.is_empty()
                || !rule.servers.is_empty()
                || !rule.tools.is_empty()
                || !rule.resource_schemes.is_empty()
                || !rule.prompts.is_empty()
            {
                return Err(
                    GatewayControlPlaneError::PolicyRuleActionUnsupportedByServerScope {
                        policy: policy.version.clone(),
                        rule: rule.id.clone(),
                        action: *action,
                    },
                );
            }
            continue;
        }
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
        GatewayAction::TasksGet
        | GatewayAction::TasksUpdate
        | GatewayAction::TasksResult
        | GatewayAction::TasksCancel
        | GatewayAction::TasksSubscribe => server.capabilities.tasks,
        GatewayAction::ArtifactRead | GatewayAction::UsageRead => server.capabilities.resources,
        GatewayAction::AdminRead | GatewayAction::AdminWrite => true,
        GatewayAction::RecordingStreamOpen
        | GatewayAction::RecordingStreamStatus
        | GatewayAction::RecordingBatchAppend
        | GatewayAction::RecordingStreamFinish => false,
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
    recording_ingest_resources: &BTreeMap<ProtectedResourceName, &RecordingIngestResource>,
    policies: &BTreeMap<PolicyVersion, &PolicySet>,
    servers: &BTreeMap<ServerSlug, &ServerManifest>,
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
    if client.allowed_resources.is_empty() {
        return Err(
            GatewayControlPlaneError::OAuthClientWithoutAllowedResources(client.id.clone()),
        );
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
    validate_oauth_client_surface(client, servers)?;

    for resource_id in &client.allowed_resources {
        let profile = profiles
            .values()
            .copied()
            .find(|profile| &profile.protected_resource == resource_id);
        let recording_ingest = recording_ingest_resources
            .values()
            .copied()
            .find(|resource| &resource.protected_resource == resource_id);
        let (resource_authorization_server, required_scopes) = if let Some(profile) = profile {
            let mut required_scopes = profile
                .required_scopes
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>();
            if let Some(policy) = policies.get(&profile.policy_version) {
                for rule in &policy.rules {
                    if (rule.protected_resources.is_empty()
                        && (rule.profiles.is_empty() || rule.profiles.contains(&profile.id)))
                        || rule
                            .protected_resources
                            .contains(&profile.protected_resource)
                    {
                        required_scopes.extend(rule.required_scopes.iter().cloned());
                    }
                }
            }
            (&profile.authorization_server, required_scopes)
        } else if let Some(resource) = recording_ingest {
            let mut required_scopes = resource.required_scopes.clone();
            if let Some(policy) = policies.get(&resource.policy_version) {
                for rule in &policy.rules {
                    if rule
                        .protected_resources
                        .contains(&resource.protected_resource)
                    {
                        required_scopes.extend(rule.required_scopes.iter().cloned());
                    }
                }
            }
            (&resource.authorization_server, required_scopes)
        } else {
            return Err(GatewayControlPlaneError::UnknownOAuthClientResource {
                client: client.id.clone(),
                resource: resource_id.clone(),
            });
        };
        if resource_authorization_server != &client.authorization_server {
            return Err(
                GatewayControlPlaneError::OAuthClientResourceAuthorizationServerMismatch {
                    client: client.id.clone(),
                    resource: resource_id.clone(),
                    client_authorization_server: client.authorization_server.clone(),
                    resource_authorization_server: resource_authorization_server.clone(),
                },
            );
        }
        for scope in required_scopes {
            if !client.allowed_scopes.contains(&scope) {
                return Err(GatewayControlPlaneError::OAuthClientMissingAllowedScope {
                    client: client.id.clone(),
                    resource: resource_id.clone(),
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

fn validate_oauth_client_surface(
    client: &OAuthClientRegistration,
    servers: &BTreeMap<ServerSlug, &ServerManifest>,
) -> Result<(), GatewayControlPlaneError> {
    if client.client_surface == OAuthClientSurface::FullMcp
        && (!client.allowed_compatibility_helpers.is_empty() || client.direct_task_call_adapter)
    {
        return Err(
            GatewayControlPlaneError::OAuthClientFullMcpWithCompatibility {
                client: client.id.clone(),
            },
        );
    }
    for helper in &client.allowed_compatibility_helpers {
        let Some((server_slug, tool_name)) = helper.as_str().split_once('.') else {
            return Err(
                GatewayControlPlaneError::UnknownOAuthClientCompatibilityHelper {
                    client: client.id.clone(),
                    helper: helper.clone(),
                },
            );
        };
        let Ok(server_slug) = ServerSlug::new(server_slug.to_string()) else {
            return Err(
                GatewayControlPlaneError::UnknownOAuthClientCompatibilityHelper {
                    client: client.id.clone(),
                    helper: helper.clone(),
                },
            );
        };
        let Ok(tool_name) = LocalToolName::new(tool_name.to_string()) else {
            return Err(
                GatewayControlPlaneError::UnknownOAuthClientCompatibilityHelper {
                    client: client.id.clone(),
                    helper: helper.clone(),
                },
            );
        };
        let Some(server) = servers.get(&server_slug) else {
            return Err(
                GatewayControlPlaneError::UnknownOAuthClientCompatibilityHelper {
                    client: client.id.clone(),
                    helper: helper.clone(),
                },
            );
        };
        if !server
            .compatibility_helpers
            .iter()
            .any(|declared| declared == &tool_name)
        {
            return Err(
                GatewayControlPlaneError::UnknownOAuthClientCompatibilityHelper {
                    client: client.id.clone(),
                    helper: helper.clone(),
                },
            );
        }
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
    let refresh_grants = client.grant_types.contains(&OAuthGrantType::RefreshToken);
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
        && (!refresh_grants || browser_grants)
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
    profiles: &BTreeMap<GatewayProfileId, &GatewayProfile>,
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
    if client.allowed_resources.is_empty() {
        return Err(GatewayControlPlaneError::OidcClientWithoutAllowedResources(
            client.id.clone(),
        ));
    }
    for resource_id in &client.allowed_resources {
        let Some(profile) = profiles
            .values()
            .copied()
            .find(|profile| &profile.protected_resource == resource_id)
        else {
            return Err(GatewayControlPlaneError::UnknownOidcClientResource {
                client: client.id.clone(),
                resource: resource_id.clone(),
            });
        };
        if profile.authorization_server != client.authorization_server {
            return Err(
                GatewayControlPlaneError::OidcClientResourceAuthorizationServerMismatch {
                    client: client.id.clone(),
                    resource: resource_id.clone(),
                    client_authorization_server: client.authorization_server.clone(),
                    resource_authorization_server: profile.authorization_server.clone(),
                },
            );
        }
        if profile.identity_provider != client.identity_provider {
            return Err(
                GatewayControlPlaneError::OidcClientResourceIdentityProviderMismatch {
                    client: client.id.clone(),
                    resource: resource_id.clone(),
                    client_identity_provider: client.identity_provider.clone(),
                    resource_identity_provider: profile.identity_provider.clone(),
                },
            );
        }
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
