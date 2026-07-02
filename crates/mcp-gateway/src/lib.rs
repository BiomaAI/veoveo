use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::Path,
    sync::Arc,
};

pub mod auth;
pub mod mcp;
pub mod secrets;
pub mod state;

use anyhow::{Context, Result};
pub use auth::{
    AuthError, AuthenticatedSubject, BearerToken, ClientAssertionConfig, ClientAssertionVerifier,
    IdJagConfig, IdJagVerifier, JwtAuthConfig, JwtVerifier, OidcIdTokenConfig, OidcIdTokenVerifier,
    VerifiedClientAssertion, VerifiedIdJag, VerifiedOidcIdentity,
};
pub use mcp::GatewayMcp;
use parking_lot::RwLock;
pub use secrets::{GatewaySecretResolver, ResolvedSecretString, SecretResolverError};
use serde::{Deserialize, Serialize};
pub use state::{GatewayAuditCounts, GatewayAuditRetentionSummary, GatewayState};
use veoveo_mcp_contract::{
    AuthMode, AuthorizationServerId, GatewayAction, GatewayControlPlane, GatewayProfile,
    GatewayProfileId, GatewayToolName, IdentityProvider, IdentityProviderId, JwksSource,
    LocalToolName, McpMethodName, OAuthClientAuthMethod, OAuthClientId, OAuthClientRegistration,
    OAuthGrantType, OidcClientRegistrationId, PolicyDecision, PolicyEffect, PolicyReasonCode,
    PolicyRule, PolicyRuleId, PolicySet, PolicyTarget, PolicyVersion, Principal,
    ResourceAuthorizationServer, ResourceScheme, ScopeName, SecretReference, SecretReferenceId,
    ServerManifest, ServerSlug, TraceId,
};

const ID_JAG_GRANT_PROFILE: &str = "urn:ietf:params:oauth:grant-profile:id-jag";

#[derive(Debug, Clone)]
pub struct GatewayCatalogHandle {
    state: Arc<RwLock<GatewayCatalogHandleState>>,
}

#[derive(Debug)]
struct GatewayCatalogHandleState {
    catalog: Arc<GatewayCatalog>,
    generation: u64,
}

#[derive(Debug, Clone)]
pub struct GatewayCatalogSnapshot {
    catalog: Arc<GatewayCatalog>,
    generation: u64,
}

impl GatewayCatalogHandle {
    pub fn new(catalog: Arc<GatewayCatalog>) -> Self {
        Self {
            state: Arc::new(RwLock::new(GatewayCatalogHandleState {
                catalog,
                generation: 0,
            })),
        }
    }

    pub fn current(&self) -> Arc<GatewayCatalog> {
        self.snapshot().catalog
    }

    pub fn snapshot(&self) -> GatewayCatalogSnapshot {
        let state = self.state.read();
        GatewayCatalogSnapshot {
            catalog: state.catalog.clone(),
            generation: state.generation,
        }
    }

    pub fn replace(&self, catalog: Arc<GatewayCatalog>) {
        let mut state = self.state.write();
        state.catalog = catalog;
        state.generation = state.generation.saturating_add(1);
    }
}

impl GatewayCatalogSnapshot {
    pub fn catalog(&self) -> &Arc<GatewayCatalog> {
        &self.catalog
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[derive(Debug, Clone)]
pub struct GatewayCatalog {
    control_plane: Arc<GatewayControlPlane>,
    identity_providers: BTreeMap<IdentityProviderId, usize>,
    authorization_servers: BTreeMap<AuthorizationServerId, usize>,
    servers: BTreeMap<ServerSlug, usize>,
    profiles: BTreeMap<GatewayProfileId, usize>,
    policies: BTreeMap<PolicyVersion, usize>,
    oauth_clients: BTreeMap<OAuthClientId, usize>,
    oidc_clients: BTreeMap<OidcClientRegistrationId, usize>,
    secrets: BTreeMap<SecretReferenceId, usize>,
}

impl GatewayCatalog {
    pub fn from_control_plane(control_plane: GatewayControlPlane) -> Result<Self> {
        control_plane.validate()?;

        let identity_providers = control_plane
            .identity_providers
            .iter()
            .enumerate()
            .map(|(index, identity_provider)| (identity_provider.id.clone(), index))
            .collect();
        let authorization_servers = control_plane
            .authorization_servers
            .iter()
            .enumerate()
            .map(|(index, authorization_server)| (authorization_server.id.clone(), index))
            .collect();
        let servers = control_plane
            .servers
            .iter()
            .enumerate()
            .map(|(index, server)| (server.slug.clone(), index))
            .collect();
        let profiles = control_plane
            .profiles
            .iter()
            .enumerate()
            .map(|(index, profile)| (profile.id.clone(), index))
            .collect();
        let policies = control_plane
            .policies
            .iter()
            .enumerate()
            .map(|(index, policy)| (policy.version.clone(), index))
            .collect();
        let oauth_clients = control_plane
            .oauth_clients
            .iter()
            .enumerate()
            .map(|(index, client)| (client.id.clone(), index))
            .collect();
        let oidc_clients = control_plane
            .oidc_clients
            .iter()
            .enumerate()
            .map(|(index, client)| (client.id.clone(), index))
            .collect();
        let secrets = control_plane
            .secrets
            .iter()
            .enumerate()
            .map(|(index, secret)| (secret.id.clone(), index))
            .collect();

        Ok(Self {
            control_plane: Arc::new(control_plane),
            identity_providers,
            authorization_servers,
            servers,
            profiles,
            policies,
            oauth_clients,
            oidc_clients,
            secrets,
        })
    }

    pub fn load_json(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read gateway control plane {}", path.display()))?;
        let control_plane: GatewayControlPlane = serde_json::from_str(&text)
            .with_context(|| format!("failed to parse gateway control plane {}", path.display()))?;
        Self::from_control_plane(control_plane)
            .with_context(|| format!("invalid gateway control plane {}", path.display()))
    }

    pub fn control_plane(&self) -> &GatewayControlPlane {
        &self.control_plane
    }

    pub fn profiles(&self) -> impl Iterator<Item = &GatewayProfile> {
        self.control_plane.profiles.iter()
    }

    pub fn identity_providers(&self) -> impl Iterator<Item = &IdentityProvider> {
        self.control_plane.identity_providers.iter()
    }

    pub fn server_count(&self) -> usize {
        self.control_plane.servers.len()
    }

    pub fn profile_count(&self) -> usize {
        self.control_plane.profiles.len()
    }

    pub fn profile(&self, profile_id: &GatewayProfileId) -> Option<&GatewayProfile> {
        self.profiles
            .get(profile_id)
            .map(|index| &self.control_plane.profiles[*index])
    }

    pub fn identity_provider(
        &self,
        identity_provider_id: &IdentityProviderId,
    ) -> Option<&IdentityProvider> {
        self.identity_providers
            .get(identity_provider_id)
            .map(|index| &self.control_plane.identity_providers[*index])
    }

    pub fn authorization_server(
        &self,
        authorization_server_id: &AuthorizationServerId,
    ) -> Option<&ResourceAuthorizationServer> {
        self.authorization_servers
            .get(authorization_server_id)
            .map(|index| &self.control_plane.authorization_servers[*index])
    }

    pub fn protected_resource_metadata(
        &self,
        profile_id: &GatewayProfileId,
    ) -> Result<ProtectedResourceMetadata, GatewayMetadataError> {
        let profile = self
            .profile(profile_id)
            .ok_or_else(|| GatewayMetadataError::UnknownProfile(profile_id.clone()))?;
        let authorization_server = self
            .authorization_server(&profile.authorization_server)
            .ok_or_else(|| GatewayMetadataError::UnknownAuthorizationServer {
                profile: profile.id.clone(),
                authorization_server: profile.authorization_server.clone(),
            })?;

        Ok(ProtectedResourceMetadata {
            resource: profile.protected_resource.to_string(),
            authorization_servers: vec![authorization_server.issuer.to_string()],
            scopes_supported: self
                .profile_supported_scopes(profile)
                .into_iter()
                .map(|scope| scope.to_string())
                .collect(),
            bearer_methods_supported: vec!["header".to_string()],
            extensions: authorization_extensions(profile),
        })
    }

    pub fn authorization_server_metadata(
        &self,
        profile_id: &GatewayProfileId,
    ) -> Result<AuthorizationServerMetadata, GatewayMetadataError> {
        let profile = self
            .profile(profile_id)
            .ok_or_else(|| GatewayMetadataError::UnknownProfile(profile_id.clone()))?;
        let authorization_server = self
            .authorization_server(&profile.authorization_server)
            .ok_or_else(|| GatewayMetadataError::UnknownAuthorizationServer {
                profile: profile.id.clone(),
                authorization_server: profile.authorization_server.clone(),
            })?;
        let clients = self.profile_oauth_clients(profile);
        let token_auth_methods = clients
            .iter()
            .flat_map(|client| client.auth_methods.iter().copied())
            .map(oauth_client_auth_method_name)
            .collect::<BTreeSet<_>>();
        let grant_types = profile
            .auth_modes
            .iter()
            .copied()
            .map(OAuthGrantType::from)
            .map(oauth_grant_type_name)
            .collect::<BTreeSet<_>>();
        let supports_authorization_code = profile
            .auth_modes
            .contains(&AuthMode::OidcAuthorizationCodePkce);
        let supports_private_key_jwt = clients.iter().any(|client| {
            client
                .auth_methods
                .contains(&OAuthClientAuthMethod::PrivateKeyJwt)
        });

        Ok(AuthorizationServerMetadata {
            issuer: authorization_server.issuer.to_string(),
            authorization_endpoint: authorization_server
                .authorization_endpoint
                .as_ref()
                .map(ToString::to_string),
            token_endpoint: authorization_server.token_endpoint.to_string(),
            jwks_uri: match &authorization_server.jwks {
                JwksSource::Remote { jwks_uri } => Some(jwks_uri.to_string()),
                JwksSource::File { .. } => None,
            },
            scopes_supported: self
                .profile_supported_scopes(profile)
                .into_iter()
                .map(|scope| scope.to_string())
                .collect(),
            response_types_supported: if supports_authorization_code {
                vec!["code".to_string()]
            } else {
                Vec::new()
            },
            grant_types_supported: grant_types.into_iter().map(str::to_string).collect(),
            code_challenge_methods_supported: if supports_authorization_code {
                vec!["S256".to_string()]
            } else {
                Vec::new()
            },
            token_endpoint_auth_methods_supported: token_auth_methods
                .into_iter()
                .map(str::to_string)
                .collect(),
            token_endpoint_auth_signing_alg_values_supported: if supports_private_key_jwt {
                vec![
                    "RS256".to_string(),
                    "RS384".to_string(),
                    "RS512".to_string(),
                    "PS256".to_string(),
                    "PS384".to_string(),
                    "PS512".to_string(),
                    "ES256".to_string(),
                    "ES384".to_string(),
                    "EdDSA".to_string(),
                ]
            } else {
                Vec::new()
            },
            authorization_grant_profiles_supported: if profile
                .auth_modes
                .contains(&AuthMode::EnterpriseManagedAuthorization)
            {
                vec![ID_JAG_GRANT_PROFILE.to_string()]
            } else {
                Vec::new()
            },
        })
    }

    pub fn profile_supported_scopes(&self, profile: &GatewayProfile) -> BTreeSet<ScopeName> {
        let mut scopes = profile
            .required_scopes
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if let Some(policy) = self.policy(&profile.policy_version) {
            for rule in &policy.rules {
                if rule.profiles.is_empty() || rule.profiles.contains(&profile.id) {
                    scopes.extend(rule.required_scopes.iter().cloned());
                }
            }
        }
        scopes
    }

    pub fn oauth_client(&self, client_id: &OAuthClientId) -> Option<&OAuthClientRegistration> {
        self.oauth_clients
            .get(client_id)
            .map(|index| &self.control_plane.oauth_clients[*index])
    }

    pub fn profile_oauth_clients(&self, profile: &GatewayProfile) -> Vec<&OAuthClientRegistration> {
        self.control_plane
            .oauth_clients
            .iter()
            .filter(|client| {
                client.authorization_server == profile.authorization_server
                    && client.allowed_profiles.contains(&profile.id)
            })
            .collect()
    }

    pub fn oidc_client(
        &self,
        client_id: &OidcClientRegistrationId,
    ) -> Option<&veoveo_mcp_contract::IdentityProviderOidcClientRegistration> {
        self.oidc_clients
            .get(client_id)
            .map(|index| &self.control_plane.oidc_clients[*index])
    }

    pub fn profile_oidc_client(
        &self,
        profile: &GatewayProfile,
    ) -> Option<&veoveo_mcp_contract::IdentityProviderOidcClientRegistration> {
        self.control_plane.oidc_clients.iter().find(|client| {
            client.identity_provider == profile.identity_provider
                && client.authorization_server == profile.authorization_server
        })
    }

    pub fn secret_reference(&self, secret_id: &SecretReferenceId) -> Option<&SecretReference> {
        self.secrets
            .get(secret_id)
            .map(|index| &self.control_plane.secrets[*index])
    }

    pub fn server(&self, server_slug: &ServerSlug) -> Option<&ServerManifest> {
        self.servers
            .get(server_slug)
            .map(|index| &self.control_plane.servers[*index])
    }

    pub fn profile_server(
        &self,
        profile_id: &GatewayProfileId,
        server_slug: &ServerSlug,
    ) -> Option<(
        &GatewayProfile,
        &veoveo_mcp_contract::ProfileServerExposure,
        &ServerManifest,
    )> {
        let profile = self.profile(profile_id)?;
        let exposure = profile
            .servers
            .iter()
            .find(|exposure| &exposure.server == server_slug)?;
        let server = self.server(server_slug)?;
        Some((profile, exposure, server))
    }

    pub fn profile_servers(
        &self,
        profile_id: &GatewayProfileId,
    ) -> Vec<(&veoveo_mcp_contract::ProfileServerExposure, &ServerManifest)> {
        let Some(profile) = self.profile(profile_id) else {
            return Vec::new();
        };
        profile
            .servers
            .iter()
            .filter_map(|exposure| {
                self.server(&exposure.server)
                    .map(|server| (exposure, server))
            })
            .collect()
    }

    pub fn server_for_resource_uri(
        &self,
        profile_id: &GatewayProfileId,
        uri: &str,
    ) -> Option<(&veoveo_mcp_contract::ProfileServerExposure, &ServerManifest)> {
        let scheme = resource_scheme(uri)?;
        self.profile_servers(profile_id)
            .into_iter()
            .find(|(_, server)| server.uri_scheme == scheme)
    }

    pub fn prompt_servers(
        &self,
        profile_id: &GatewayProfileId,
        prompt: &veoveo_mcp_contract::PromptName,
    ) -> Vec<(&veoveo_mcp_contract::ProfileServerExposure, &ServerManifest)> {
        self.profile_servers(profile_id)
            .into_iter()
            .filter(|(exposure, server)| {
                (server.prompts.is_empty() || server.prompts.iter().any(|known| known == prompt))
                    && exposure_contains(&exposure.prompts, prompt)
            })
            .collect()
    }

    pub fn policy(&self, version: &PolicyVersion) -> Option<&PolicySet> {
        self.policies
            .get(version)
            .map(|index| &self.control_plane.policies[*index])
    }

    pub fn project_tool_name(
        &self,
        server: &ServerSlug,
        tool: &LocalToolName,
    ) -> Result<GatewayToolName, GatewayNameError> {
        if self.server(server).is_none() {
            return Err(GatewayNameError::UnknownServer(server.clone()));
        }
        GatewayToolProjection::new(server.clone(), tool.clone()).gateway_name()
    }

    pub fn parse_tool_name(
        &self,
        name: &GatewayToolName,
    ) -> Result<GatewayToolProjection, GatewayNameError> {
        let projection = GatewayToolProjection::parse(name)?;
        if self.server(&projection.server).is_none() {
            return Err(GatewayNameError::UnknownServer(projection.server));
        }
        Ok(projection)
    }

    pub fn decide(&self, request: PolicyRequest<'_>) -> PolicyDecision {
        let Some(profile) = self.profile(request.profile) else {
            return deny(
                &request,
                PolicyReasonCode::UnknownProfile,
                PolicyTarget::Gateway,
                None,
            );
        };

        let Some(policy) = self.policy(&profile.policy_version) else {
            return deny(
                &request,
                PolicyReasonCode::PolicyDeny,
                request.target.clone(),
                None,
            );
        };

        if let Err(reason) = self.profile_allows_target(profile, request.action, request.target) {
            return deny(
                &request,
                reason,
                request.target.clone(),
                Some(policy.version.clone()),
            );
        }

        if !has_required_scopes(&request.principal.scopes, &profile.required_scopes) {
            return deny(
                &request,
                PolicyReasonCode::MissingScope,
                request.target.clone(),
                Some(policy.version.clone()),
            );
        }

        let matching_denial = policy
            .rules
            .iter()
            .find(|rule| {
                rule.effect == PolicyEffect::Deny
                    && rule_match_detail(rule, profile, &request) == RuleMatchDetail::Match
            })
            .map(|rule| rule.id.clone());
        if let Some(rule_id) = matching_denial {
            return decision(
                &request,
                PolicyEffect::Deny,
                PolicyReasonCode::PolicyDeny,
                request.target.clone(),
                Some(policy.version.clone()),
                Some(rule_id),
            );
        }

        let mut strongest_missing_requirement: Option<(PolicyReasonCode, PolicyRuleId)> = None;
        for rule in &policy.rules {
            if rule.effect != PolicyEffect::Allow {
                continue;
            }
            match rule_match_detail(rule, profile, &request) {
                RuleMatchDetail::Match => {
                    return decision(
                        &request,
                        PolicyEffect::Allow,
                        PolicyReasonCode::PolicyAllow,
                        request.target.clone(),
                        Some(policy.version.clone()),
                        Some(rule.id.clone()),
                    );
                }
                RuleMatchDetail::MissingDataLabel => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing_requirement,
                        PolicyReasonCode::MissingDataLabel,
                        rule.id.clone(),
                    );
                }
                RuleMatchDetail::MissingRole => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing_requirement,
                        PolicyReasonCode::MissingRole,
                        rule.id.clone(),
                    );
                }
                RuleMatchDetail::MissingGroup => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing_requirement,
                        PolicyReasonCode::MissingGroup,
                        rule.id.clone(),
                    );
                }
                RuleMatchDetail::MissingTenant => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing_requirement,
                        PolicyReasonCode::MissingTenant,
                        rule.id.clone(),
                    );
                }
                RuleMatchDetail::MissingPrincipal => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing_requirement,
                        PolicyReasonCode::MissingPrincipal,
                        rule.id.clone(),
                    );
                }
                RuleMatchDetail::MissingScope => {
                    remember_strongest_missing_requirement(
                        &mut strongest_missing_requirement,
                        PolicyReasonCode::MissingScope,
                        rule.id.clone(),
                    );
                }
                RuleMatchDetail::NoMatch => {}
            }
        }
        if let Some((reason, rule_id)) = strongest_missing_requirement {
            return decision(
                &request,
                PolicyEffect::Deny,
                reason,
                request.target.clone(),
                Some(policy.version.clone()),
                Some(rule_id),
            );
        }

        deny(
            &request,
            PolicyReasonCode::PolicyDeny,
            request.target.clone(),
            Some(policy.version.clone()),
        )
    }

    fn profile_allows_target(
        &self,
        profile: &GatewayProfile,
        action: GatewayAction,
        target: &PolicyTarget,
    ) -> Result<(), PolicyReasonCode> {
        match target {
            PolicyTarget::Gateway => Ok(()),
            PolicyTarget::Server { server } => {
                if self.server(server).is_none() {
                    return Err(PolicyReasonCode::UnknownServer);
                }
                profile
                    .servers
                    .iter()
                    .any(|exposure| &exposure.server == server)
                    .then_some(())
                    .ok_or(PolicyReasonCode::PolicyDeny)
            }
            PolicyTarget::Tool { server, tool } => {
                let manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
                if !manifest.tools.is_empty() && !manifest.tools.iter().any(|known| known == tool) {
                    return Err(PolicyReasonCode::UnknownTool);
                }
                let exposure = profile
                    .servers
                    .iter()
                    .find(|exposure| &exposure.server == server)
                    .ok_or(PolicyReasonCode::PolicyDeny)?;
                if exposure_contains(&exposure.tools, tool) {
                    Ok(())
                } else {
                    Err(PolicyReasonCode::PolicyDeny)
                }
            }
            PolicyTarget::Resource { server, uri }
            | PolicyTarget::Artifact {
                server,
                artifact_uri: uri,
            }
            | PolicyTarget::Usage {
                server,
                usage_uri: uri,
            } => {
                let manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
                let scheme =
                    resource_scheme(uri.as_str()).ok_or(PolicyReasonCode::UnknownResource)?;
                if scheme != manifest.uri_scheme {
                    return Err(PolicyReasonCode::UnknownResource);
                }
                let exposure = profile
                    .servers
                    .iter()
                    .find(|exposure| &exposure.server == server)
                    .ok_or(PolicyReasonCode::PolicyDeny)?;
                if matches!(&exposure.resources, veoveo_mcp_contract::Exposure::All)
                    || exposure.resources.iter().any(|selector| match selector {
                        veoveo_mcp_contract::ResourceSelector::Scheme { scheme: allowed } => {
                            allowed == &scheme
                        }
                        veoveo_mcp_contract::ResourceSelector::UriPrefix { prefix } => {
                            uri.as_str().starts_with(prefix.as_ref())
                        }
                        veoveo_mcp_contract::ResourceSelector::Template { uri_template } => {
                            uri_template.matches_uri(uri)
                        }
                    })
                {
                    Ok(())
                } else {
                    Err(PolicyReasonCode::PolicyDeny)
                }
            }
            PolicyTarget::Prompt { server, prompt } => {
                let manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
                if !manifest.prompts.is_empty() && !manifest.prompts.iter().any(|p| p == prompt) {
                    return Err(PolicyReasonCode::UnknownPrompt);
                }
                let exposure = profile
                    .servers
                    .iter()
                    .find(|exposure| &exposure.server == server)
                    .ok_or(PolicyReasonCode::PolicyDeny)?;
                if exposure_contains(&exposure.prompts, prompt) {
                    Ok(())
                } else {
                    Err(PolicyReasonCode::PolicyDeny)
                }
            }
            PolicyTarget::TaskList { server } => {
                let _manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
                let exposure = profile
                    .servers
                    .iter()
                    .find(|exposure| &exposure.server == server)
                    .ok_or(PolicyReasonCode::PolicyDeny)?;
                if exposure.tasks == veoveo_mcp_contract::TaskExposure::Enabled {
                    Ok(())
                } else {
                    Err(PolicyReasonCode::PolicyDeny)
                }
            }
            PolicyTarget::Task {
                server,
                gateway_task_id: _,
            } => {
                let _manifest = self.server(server).ok_or(PolicyReasonCode::UnknownServer)?;
                let exposure = profile
                    .servers
                    .iter()
                    .find(|exposure| &exposure.server == server)
                    .ok_or(PolicyReasonCode::PolicyDeny)?;
                if exposure.tasks == veoveo_mcp_contract::TaskExposure::Enabled
                    || matches!(
                        action,
                        GatewayAction::ResourcesList | GatewayAction::ResourcesTemplatesList
                    )
                {
                    Ok(())
                } else {
                    Err(PolicyReasonCode::PolicyDeny)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleMatchDetail {
    Match,
    MissingPrincipal,
    MissingTenant,
    MissingGroup,
    MissingRole,
    MissingScope,
    MissingDataLabel,
    NoMatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedResourceMetadata {
    pub resource: String,
    pub authorization_servers: Vec<String>,
    pub scopes_supported: Vec<String>,
    pub bearer_methods_supported: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, AuthorizationExtensionMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationServerMetadata {
    pub issuer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization_endpoint: Option<String>,
    pub token_endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwks_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes_supported: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_types_supported: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grant_types_supported: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_challenge_methods_supported: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub token_endpoint_auth_methods_supported: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub token_endpoint_auth_signing_alg_values_supported: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authorization_grant_profiles_supported: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AuthorizationExtensionMetadata {}

fn authorization_extensions(
    profile: &GatewayProfile,
) -> BTreeMap<String, AuthorizationExtensionMetadata> {
    profile
        .auth_modes
        .iter()
        .filter_map(|mode| mode.mcp_extension_id())
        .map(|extension| {
            (
                extension.to_string(),
                AuthorizationExtensionMetadata::default(),
            )
        })
        .collect()
}

fn oauth_grant_type_name(grant_type: OAuthGrantType) -> &'static str {
    match grant_type {
        OAuthGrantType::AuthorizationCodePkce => "authorization_code",
        OAuthGrantType::ClientCredentials => "client_credentials",
        OAuthGrantType::EnterpriseManagedAuthorization => {
            "urn:ietf:params:oauth:grant-type:jwt-bearer"
        }
    }
}

fn oauth_client_auth_method_name(auth_method: OAuthClientAuthMethod) -> &'static str {
    match auth_method {
        OAuthClientAuthMethod::None => "none",
        OAuthClientAuthMethod::PrivateKeyJwt => "private_key_jwt",
        OAuthClientAuthMethod::ClientSecretBasic => "client_secret_basic",
        OAuthClientAuthMethod::ClientSecretPost => "client_secret_post",
        OAuthClientAuthMethod::TlsClientAuth => "tls_client_auth",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayMetadataError {
    UnknownProfile(GatewayProfileId),
    UnknownAuthorizationServer {
        profile: GatewayProfileId,
        authorization_server: AuthorizationServerId,
    },
}

impl fmt::Display for GatewayMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProfile(profile) => write!(f, "unknown gateway profile `{profile}`"),
            Self::UnknownAuthorizationServer {
                profile,
                authorization_server,
            } => write!(
                f,
                "gateway profile `{profile}` references unknown resource authorization server `{authorization_server}`"
            ),
        }
    }
}

impl std::error::Error for GatewayMetadataError {}

pub fn www_authenticate_challenge(metadata_url: &str, scopes: &[ScopeName]) -> String {
    let scope = scopes
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    if scope.is_empty() {
        format!(r#"Bearer resource_metadata="{metadata_url}""#)
    } else {
        format!(r#"Bearer resource_metadata="{metadata_url}", scope="{scope}""#)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayToolProjection {
    pub server: ServerSlug,
    pub tool: LocalToolName,
}

impl GatewayToolProjection {
    pub fn new(server: ServerSlug, tool: LocalToolName) -> Self {
        Self { server, tool }
    }

    pub fn gateway_name(&self) -> Result<GatewayToolName, GatewayNameError> {
        GatewayToolName::new(format!("{}__{}", self.server, self.tool))
            .map_err(GatewayNameError::InvalidProjectedToolName)
    }

    pub fn parse(name: &GatewayToolName) -> Result<Self, GatewayNameError> {
        let Some((server, tool)) = name.as_str().split_once("__") else {
            return Err(GatewayNameError::MissingNamespace(name.clone()));
        };
        if tool.contains("__") {
            return Err(GatewayNameError::InvalidNamespaceShape(name.clone()));
        }
        Ok(Self {
            server: ServerSlug::new(server).map_err(GatewayNameError::InvalidServerSlug)?,
            tool: LocalToolName::new(tool).map_err(GatewayNameError::InvalidLocalToolName)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayNameError {
    UnknownServer(ServerSlug),
    MissingNamespace(GatewayToolName),
    InvalidNamespaceShape(GatewayToolName),
    InvalidServerSlug(veoveo_mcp_contract::IdentifierError),
    InvalidLocalToolName(veoveo_mcp_contract::IdentifierError),
    InvalidProjectedToolName(veoveo_mcp_contract::IdentifierError),
}

impl fmt::Display for GatewayNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownServer(server) => write!(f, "unknown server `{server}`"),
            Self::MissingNamespace(name) => {
                write!(f, "gateway tool `{name}` is missing server namespace")
            }
            Self::InvalidNamespaceShape(name) => {
                write!(f, "gateway tool `{name}` has an invalid namespace shape")
            }
            Self::InvalidServerSlug(err)
            | Self::InvalidLocalToolName(err)
            | Self::InvalidProjectedToolName(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for GatewayNameError {}

#[derive(Debug, Clone)]
pub struct PolicyRequest<'a> {
    pub principal: &'a Principal,
    pub profile: &'a GatewayProfileId,
    pub action: GatewayAction,
    pub target: &'a PolicyTarget,
    pub trace_id: &'a TraceId,
}

pub fn mcp_method_name(action: GatewayAction) -> Result<McpMethodName> {
    let Some(method) = action.mcp_method() else {
        anyhow::bail!("gateway action {action:?} does not map to one MCP method")
    };
    Ok(McpMethodName::new(method)?)
}

fn deny(
    request: &PolicyRequest<'_>,
    reason: PolicyReasonCode,
    target: PolicyTarget,
    policy_version: Option<PolicyVersion>,
) -> PolicyDecision {
    decision(
        request,
        PolicyEffect::Deny,
        reason,
        target,
        policy_version,
        None,
    )
}

fn decision(
    request: &PolicyRequest<'_>,
    effect: PolicyEffect,
    reason: PolicyReasonCode,
    target: PolicyTarget,
    policy_version: Option<PolicyVersion>,
    rule_id: Option<veoveo_mcp_contract::PolicyRuleId>,
) -> PolicyDecision {
    PolicyDecision {
        effect,
        reason,
        evaluated_at: chrono::Utc::now(),
        profile: request.profile.clone(),
        action: request.action,
        target,
        principal: Some(request.principal.id.clone()),
        tenant: request.principal.tenant.clone(),
        policy_version,
        rule_id,
        trace_id: request.trace_id.clone(),
    }
}

fn rule_match_detail(
    rule: &PolicyRule,
    profile: &GatewayProfile,
    request: &PolicyRequest<'_>,
) -> RuleMatchDetail {
    if !rule.actions.contains(&request.action) {
        return RuleMatchDetail::NoMatch;
    }
    if !rule.profiles.is_empty() && !rule.profiles.contains(&profile.id) {
        return RuleMatchDetail::NoMatch;
    }
    if !matches_target_filters(rule, request.target) {
        return RuleMatchDetail::NoMatch;
    }
    let mut strongest_missing_requirement = RuleMatchDetail::Match;
    if !rule.principal_ids.is_empty() && !rule.principal_ids.contains(&request.principal.id) {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingPrincipal,
        );
    }
    if !rule.tenant_ids.is_empty() {
        match &request.principal.tenant {
            Some(tenant) if rule.tenant_ids.contains(tenant) => {}
            _ => {
                strongest_missing_requirement = strongest_missing_rule_detail(
                    strongest_missing_requirement,
                    RuleMatchDetail::MissingTenant,
                );
            }
        }
    }
    if !rule.groups.is_empty() && !intersects(&rule.groups, &request.principal.groups) {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingGroup,
        );
    }
    if !rule.roles.is_empty() && !intersects(&rule.roles, &request.principal.roles) {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingRole,
        );
    }
    if !rule.required_scopes.is_empty()
        && !rule.required_scopes.is_subset(&request.principal.scopes)
    {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingScope,
        );
    }
    if !rule.required_data_labels.is_empty()
        && !rule
            .required_data_labels
            .is_subset(&request.principal.data_labels)
    {
        strongest_missing_requirement = strongest_missing_rule_detail(
            strongest_missing_requirement,
            RuleMatchDetail::MissingDataLabel,
        );
    }
    strongest_missing_requirement
}

fn strongest_missing_rule_detail(left: RuleMatchDetail, right: RuleMatchDetail) -> RuleMatchDetail {
    if rule_detail_rank(right) > rule_detail_rank(left) {
        right
    } else {
        left
    }
}

fn rule_detail_rank(detail: RuleMatchDetail) -> u8 {
    match detail {
        RuleMatchDetail::MissingDataLabel => 60,
        RuleMatchDetail::MissingRole => 50,
        RuleMatchDetail::MissingGroup => 40,
        RuleMatchDetail::MissingTenant => 30,
        RuleMatchDetail::MissingPrincipal => 20,
        RuleMatchDetail::MissingScope => 10,
        RuleMatchDetail::Match | RuleMatchDetail::NoMatch => 0,
    }
}

fn remember_strongest_missing_requirement(
    current: &mut Option<(PolicyReasonCode, PolicyRuleId)>,
    reason: PolicyReasonCode,
    rule_id: PolicyRuleId,
) {
    let replace = current
        .as_ref()
        .map(|(current_reason, _)| {
            missing_requirement_rank(reason) > missing_requirement_rank(*current_reason)
        })
        .unwrap_or(true);
    if replace {
        *current = Some((reason, rule_id));
    }
}

fn missing_requirement_rank(reason: PolicyReasonCode) -> u8 {
    match reason {
        PolicyReasonCode::MissingDataLabel => 60,
        PolicyReasonCode::MissingRole => 50,
        PolicyReasonCode::MissingGroup => 40,
        PolicyReasonCode::MissingTenant => 30,
        PolicyReasonCode::MissingPrincipal => 20,
        PolicyReasonCode::MissingScope => 10,
        _ => 0,
    }
}

fn matches_target_filters(rule: &PolicyRule, target: &PolicyTarget) -> bool {
    match target {
        PolicyTarget::Gateway => {
            rule.servers.is_empty()
                && rule.tools.is_empty()
                && rule.resource_schemes.is_empty()
                && rule.prompts.is_empty()
        }
        PolicyTarget::Server { server } => {
            filter_matches(&rule.servers, server)
                && rule.tools.is_empty()
                && rule.resource_schemes.is_empty()
                && rule.prompts.is_empty()
        }
        PolicyTarget::Tool { server, tool } => {
            filter_matches(&rule.servers, server) && filter_matches(&rule.tools, tool)
        }
        PolicyTarget::Resource { server, uri }
        | PolicyTarget::Artifact {
            server,
            artifact_uri: uri,
        }
        | PolicyTarget::Usage {
            server,
            usage_uri: uri,
        } => {
            let Some(scheme) = resource_scheme(uri.as_str()) else {
                return false;
            };
            filter_matches(&rule.servers, server) && filter_matches(&rule.resource_schemes, &scheme)
        }
        PolicyTarget::Prompt { server, prompt } => {
            filter_matches(&rule.servers, server) && filter_matches(&rule.prompts, prompt)
        }
        PolicyTarget::TaskList { server } => filter_matches(&rule.servers, server),
        PolicyTarget::Task {
            server,
            gateway_task_id: _,
        } => filter_matches(&rule.servers, server),
    }
}

fn resource_scheme(uri: &str) -> Option<ResourceScheme> {
    let (scheme, _) = uri.split_once("://")?;
    ResourceScheme::new(scheme).ok()
}

pub fn resource_scheme_from_uri(uri: &str) -> Option<ResourceScheme> {
    resource_scheme(uri)
}

fn has_required_scopes(
    principal_scopes: &BTreeSet<veoveo_mcp_contract::ScopeName>,
    required: &[veoveo_mcp_contract::ScopeName],
) -> bool {
    required
        .iter()
        .all(|scope| principal_scopes.contains(scope))
}

fn exposure_contains<T: PartialEq>(exposure: &veoveo_mcp_contract::Exposure<T>, item: &T) -> bool {
    match exposure {
        veoveo_mcp_contract::Exposure::All => true,
        veoveo_mcp_contract::Exposure::Listed(items) => items.iter().any(|allowed| allowed == item),
        veoveo_mcp_contract::Exposure::None => false,
    }
}

trait ExposureResourceIter {
    fn iter(&self) -> Box<dyn Iterator<Item = &veoveo_mcp_contract::ResourceSelector> + '_>;
}

impl ExposureResourceIter for veoveo_mcp_contract::Exposure<veoveo_mcp_contract::ResourceSelector> {
    fn iter(&self) -> Box<dyn Iterator<Item = &veoveo_mcp_contract::ResourceSelector> + '_> {
        match self {
            veoveo_mcp_contract::Exposure::All | veoveo_mcp_contract::Exposure::None => {
                Box::new([].iter())
            }
            veoveo_mcp_contract::Exposure::Listed(items) => Box::new(items.iter()),
        }
    }
}

fn filter_matches<T: Ord>(filter: &BTreeSet<T>, value: &T) -> bool {
    filter.is_empty() || filter.contains(value)
}

fn intersects<T: Ord>(left: &BTreeSet<T>, right: &BTreeSet<T>) -> bool {
    left.iter().any(|value| right.contains(value))
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};
    use rmcp::handler::server::ServerHandler;
    use serde_json::Value;
    use veoveo_mcp_contract::{
        AuthMode, AuthorizationServerId, CompletionExposure, DataLabelId, Exposure,
        GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayControlPlaneError, GatewayInternalTokenIssuer,
        GatewayTaskId, GroupId, HttpsUrl, IdentityProvider, IdentityProviderId,
        IdentityProviderOidcClientRegistration, InternalTokenSecret, JwksSource, JwtId,
        MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION, MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION,
        MountPath, OAuthClientAuthMethod, OAuthClientId, OAuthClientRegistration, OAuthGrantType,
        OAuthRedirectUri, OidcClientAuthMethod, OidcClientId, OidcClientRegistrationId, OwnedRoute,
        OwnedRoutePurpose, PrincipalId, PrincipalKind, ProfileServerExposure, ProtectedResourceId,
        ResourceAuthorizationServer, ResourceSelector, ResourceUri, ResourceUriTemplate, RoleId,
        ScopeName, SecretLocator, SecretOwner, SecretPurpose, SecretReference, SecretReferenceId,
        SecretSource, TaskExposure, TenantId, TokenIssuer, TokenSubject, UpstreamEndpoint,
        UpstreamTransport, UpstreamTransportSecurity, UpstreamUrl,
    };

    use super::*;

    fn identity_provider() -> IdentityProvider {
        IdentityProvider {
            id: IdentityProviderId::new("enterprise").unwrap(),
            issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
            jwks: JwksSource::Remote {
                jwks_uri: HttpsUrl::new("https://idp.example.com/.well-known/jwks.json").unwrap(),
            },
            trusted_certificate_authorities: Vec::new(),
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
            locator: SecretLocator::new("VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64").unwrap(),
            owner: SecretOwner::Gateway,
            rotation_hint: None,
            metadata: Value::Null,
        }
    }

    fn oidc_client_secret() -> SecretReference {
        SecretReference {
            id: SecretReferenceId::new("enterprise_oidc_client_secret").unwrap(),
            source: SecretSource::Env,
            purpose: SecretPurpose::OAuthClientSecret,
            locator: SecretLocator::new("VEOVEO_IDP_OIDC_CLIENT_SECRET").unwrap(),
            owner: SecretOwner::Gateway,
            rotation_hint: None,
            metadata: Value::Null,
        }
    }

    fn media_manifest() -> ServerManifest {
        ServerManifest {
            slug: ServerSlug::new("media").unwrap(),
            uri_scheme: ResourceScheme::new("media").unwrap(),
            mount_path: MountPath::new("/media").unwrap(),
            mcp_path: MountPath::new("/media/mcp").unwrap(),
            upstream: UpstreamEndpoint {
                transport: UpstreamTransport::StreamableHttp,
                url: UpstreamUrl::new("http://media-mcp:8787/media/mcp").unwrap(),
                security: UpstreamTransportSecurity::ComposeInternalHttp,
            },
            capabilities: veoveo_mcp_contract::McpSurfaceCapabilities {
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
            prompts: vec![],
            required_scopes: vec![ScopeName::new("media:use").unwrap()],
            owned_routes: vec![OwnedRoute {
                path: MountPath::new("/media/webhooks").unwrap(),
                purpose: OwnedRoutePurpose::Webhook,
            }],
            metadata: Value::Null,
        }
    }

    fn policy() -> PolicySet {
        PolicySet {
            version: PolicyVersion::new("2026-07-02").unwrap(),
            rules: vec![PolicyRule {
                id: veoveo_mcp_contract::PolicyRuleId::new("allow_media_run").unwrap(),
                effect: PolicyEffect::Allow,
                actions: BTreeSet::from([GatewayAction::ToolsCall]),
                profiles: BTreeSet::from([GatewayProfileId::new("default").unwrap()]),
                servers: BTreeSet::from([ServerSlug::new("media").unwrap()]),
                tools: BTreeSet::from([LocalToolName::new("run").unwrap()]),
                resource_schemes: BTreeSet::new(),
                prompts: BTreeSet::new(),
                principal_ids: BTreeSet::new(),
                tenant_ids: BTreeSet::from([TenantId::new("tenant-a").unwrap()]),
                groups: BTreeSet::new(),
                roles: BTreeSet::new(),
                required_scopes: BTreeSet::from([ScopeName::new("media:use").unwrap()]),
                required_data_labels: BTreeSet::new(),
                metadata: Value::Null,
            }],
            metadata: Value::Null,
        }
    }

    fn profile() -> GatewayProfile {
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
                prompts: Exposure::None,
                completions: CompletionExposure::Enabled,
                tasks: TaskExposure::Enabled,
            }],
            metadata: Value::Null,
        }
    }

    fn oauth_clients() -> Vec<OAuthClientRegistration> {
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
                    ScopeName::new("media:admin").unwrap(),
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
                    ScopeName::new("media:admin").unwrap(),
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

    fn oidc_clients() -> Vec<IdentityProviderOidcClientRegistration> {
        vec![IdentityProviderOidcClientRegistration {
            id: OidcClientRegistrationId::new("enterprise").unwrap(),
            identity_provider: IdentityProviderId::new("enterprise").unwrap(),
            authorization_server: AuthorizationServerId::new("veoveo").unwrap(),
            client_id: OidcClientId::new("veoveo-gateway").unwrap(),
            redirect_uri: OAuthRedirectUri::new("https://veoveo.bioma.ai/oauth/default/callback")
                .unwrap(),
            auth_method: OidcClientAuthMethod::ClientSecretPost,
            credential_secret: SecretReferenceId::new("enterprise_oidc_client_secret").unwrap(),
            scopes: BTreeSet::from([
                ScopeName::new("openid").unwrap(),
                ScopeName::new("profile").unwrap(),
                ScopeName::new("email").unwrap(),
            ]),
            metadata: Value::Null,
        }]
    }

    fn catalog() -> GatewayCatalog {
        catalog_with_policy(policy())
    }

    fn catalog_with_policy(policy: PolicySet) -> GatewayCatalog {
        catalog_with_profile_and_policy(profile(), policy)
    }

    fn catalog_with_profile_and_policy(
        profile: GatewayProfile,
        policy: PolicySet,
    ) -> GatewayCatalog {
        GatewayCatalog::from_control_plane(GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![profile],
            policies: vec![policy],
            oauth_clients: oauth_clients(),
            oidc_clients: oidc_clients(),
            secrets: vec![
                signing_secret(),
                oidc_client_secret(),
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
        })
        .unwrap()
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let unique = uuid::Uuid::new_v4();
        std::env::temp_dir().join(format!("veoveo-gateway-lib-{name}-{unique}.duckdb"))
    }

    fn principal(scopes: &[&str]) -> Principal {
        Principal {
            id: PrincipalId::new("user@example.com").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::new("https://idp.example.com").unwrap(),
            subject: TokenSubject::new("00u123").unwrap(),
            tenant: Some(TenantId::new("tenant-a").unwrap()),
            groups: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: scopes
                .iter()
                .map(|scope| ScopeName::new(*scope).unwrap())
                .collect(),
            data_labels: BTreeSet::<DataLabelId>::new(),
            authenticated_at: Some(
                DateTime::parse_from_rfc3339("2026-07-02T00:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
        }
    }

    #[test]
    fn projects_and_parses_gateway_tool_names() {
        let catalog = catalog();
        let server = ServerSlug::new("media").unwrap();
        let tool = LocalToolName::new("run").unwrap();

        let gateway_name = catalog.project_tool_name(&server, &tool).unwrap();
        let projection = catalog.parse_tool_name(&gateway_name).unwrap();

        assert_eq!(gateway_name.as_str(), "media__run");
        assert_eq!(projection.server, server);
        assert_eq!(projection.tool, tool);
    }

    #[test]
    fn policy_allows_exposed_tool_with_required_scope() {
        let catalog = catalog();
        let principal = principal(&["media:use"]);
        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &PolicyTarget::Tool {
                server: ServerSlug::new("media").unwrap(),
                tool: LocalToolName::new("run").unwrap(),
            },
            trace_id: &TraceId::new("trace-1").unwrap(),
        });

        assert_eq!(decision.effect, PolicyEffect::Allow);
        assert_eq!(decision.reason, PolicyReasonCode::PolicyAllow);
    }

    #[test]
    fn policy_allows_template_exposed_resource_uri() {
        let mut profile = profile();
        profile.servers[0].resources = Exposure::Listed(vec![ResourceSelector::Template {
            uri_template: ResourceUriTemplate::new("media://usage/task/{task_id}").unwrap(),
        }]);
        let mut policy = policy();
        policy.rules[0].actions = BTreeSet::from([GatewayAction::UsageRead]);
        policy.rules[0].tools.clear();
        policy.rules[0].resource_schemes = BTreeSet::from([ResourceScheme::new("media").unwrap()]);
        let catalog = catalog_with_profile_and_policy(profile, policy);
        let principal = principal(&["media:use"]);

        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::UsageRead,
            target: &PolicyTarget::Usage {
                server: ServerSlug::new("media").unwrap(),
                usage_uri: ResourceUri::new("media://usage/task/task-1").unwrap(),
            },
            trace_id: &TraceId::new("trace-template-allow").unwrap(),
        });
        assert_eq!(decision.effect, PolicyEffect::Allow);
        assert_eq!(decision.reason, PolicyReasonCode::PolicyAllow);

        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::UsageRead,
            target: &PolicyTarget::Usage {
                server: ServerSlug::new("media").unwrap(),
                usage_uri: ResourceUri::new("media://usage").unwrap(),
            },
            trace_id: &TraceId::new("trace-template-deny").unwrap(),
        });
        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::PolicyDeny);
    }

    #[test]
    fn policy_allows_task_list_without_fake_task_id() {
        let mut policy = policy();
        policy.rules[0].actions = BTreeSet::from([GatewayAction::TasksList]);
        policy.rules[0].tools = BTreeSet::new();
        let catalog = catalog_with_policy(policy);
        let principal = principal(&["media:use"]);
        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::TasksList,
            target: &PolicyTarget::TaskList {
                server: ServerSlug::new("media").unwrap(),
            },
            trace_id: &TraceId::new("trace-task-list").unwrap(),
        });

        assert_eq!(decision.effect, PolicyEffect::Allow);
        assert_eq!(decision.reason, PolicyReasonCode::PolicyAllow);
    }

    #[test]
    fn policy_denies_missing_required_scope() {
        let catalog = catalog();
        let principal = principal(&[]);
        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &PolicyTarget::Tool {
                server: ServerSlug::new("media").unwrap(),
                tool: LocalToolName::new("run").unwrap(),
            },
            trace_id: &TraceId::new("trace-2").unwrap(),
        });

        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::MissingScope);
    }

    #[test]
    fn policy_denies_missing_rule_required_scope_with_specific_reason() {
        let mut policy = policy();
        policy.rules[0].required_scopes = BTreeSet::from([ScopeName::new("media:admin").unwrap()]);
        let catalog = catalog_with_policy(policy);
        let principal = principal(&["media:use"]);
        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &PolicyTarget::Tool {
                server: ServerSlug::new("media").unwrap(),
                tool: LocalToolName::new("run").unwrap(),
            },
            trace_id: &TraceId::new("trace-rule-scope").unwrap(),
        });

        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::MissingScope);
        assert_eq!(
            decision.rule_id,
            Some(PolicyRuleId::new("allow_media_run").unwrap())
        );
    }

    #[test]
    fn policy_denies_missing_required_data_label_with_specific_reason() {
        let mut policy = policy();
        policy.rules[0].required_data_labels = BTreeSet::from([DataLabelId::new("cui").unwrap()]);
        let catalog = catalog_with_policy(policy);
        let mut principal = principal(&["media:use"]);
        let target = PolicyTarget::Tool {
            server: ServerSlug::new("media").unwrap(),
            tool: LocalToolName::new("run").unwrap(),
        };
        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &target,
            trace_id: &TraceId::new("trace-label-deny").unwrap(),
        });

        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::MissingDataLabel);
        assert_eq!(
            decision.rule_id,
            Some(PolicyRuleId::new("allow_media_run").unwrap())
        );

        principal.data_labels = BTreeSet::from([DataLabelId::new("cui").unwrap()]);
        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &target,
            trace_id: &TraceId::new("trace-label-allow").unwrap(),
        });

        assert_eq!(decision.effect, PolicyEffect::Allow);
        assert_eq!(decision.reason, PolicyReasonCode::PolicyAllow);
    }

    #[test]
    fn policy_denies_missing_tenant_with_specific_reason() {
        let catalog = catalog();
        let mut principal = principal(&["media:use"]);
        principal.tenant = Some(TenantId::new("tenant-b").unwrap());
        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &PolicyTarget::Tool {
                server: ServerSlug::new("media").unwrap(),
                tool: LocalToolName::new("run").unwrap(),
            },
            trace_id: &TraceId::new("trace-tenant-deny").unwrap(),
        });

        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::MissingTenant);
        assert_eq!(
            decision.rule_id,
            Some(PolicyRuleId::new("allow_media_run").unwrap())
        );
    }

    #[test]
    fn policy_denies_missing_group_and_role_with_specific_reasons() {
        let target = PolicyTarget::Tool {
            server: ServerSlug::new("media").unwrap(),
            tool: LocalToolName::new("run").unwrap(),
        };

        let mut group_policy = policy();
        group_policy.rules[0].groups = BTreeSet::from([GroupId::new("engineering").unwrap()]);
        let group_catalog = catalog_with_policy(group_policy);
        let mut principal = principal(&["media:use"]);
        let decision = group_catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &target,
            trace_id: &TraceId::new("trace-group-deny").unwrap(),
        });
        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::MissingGroup);

        principal.groups = BTreeSet::from([GroupId::new("engineering").unwrap()]);
        let decision = group_catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &target,
            trace_id: &TraceId::new("trace-group-allow").unwrap(),
        });
        assert_eq!(decision.effect, PolicyEffect::Allow);

        let mut role_policy = policy();
        role_policy.rules[0].roles = BTreeSet::from([RoleId::new("operator").unwrap()]);
        let role_catalog = catalog_with_policy(role_policy);
        let decision = role_catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &target,
            trace_id: &TraceId::new("trace-role-deny").unwrap(),
        });
        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::MissingRole);

        principal.roles = BTreeSet::from([RoleId::new("operator").unwrap()]);
        let decision = role_catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &target,
            trace_id: &TraceId::new("trace-role-allow").unwrap(),
        });
        assert_eq!(decision.effect, PolicyEffect::Allow);
    }

    #[test]
    fn policy_denies_missing_principal_allowlist_with_specific_reason() {
        let mut policy = policy();
        policy.rules[0].principal_ids =
            BTreeSet::from([PrincipalId::new("allowed@example.com").unwrap()]);
        let catalog = catalog_with_policy(policy);
        let principal = principal(&["media:use"]);
        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &PolicyTarget::Tool {
                server: ServerSlug::new("media").unwrap(),
                tool: LocalToolName::new("run").unwrap(),
            },
            trace_id: &TraceId::new("trace-principal-deny").unwrap(),
        });

        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::MissingPrincipal);
        assert_eq!(
            decision.rule_id,
            Some(PolicyRuleId::new("allow_media_run").unwrap())
        );
    }

    #[test]
    fn policy_denies_unknown_profile() {
        let catalog = catalog();
        let principal = principal(&["media:use"]);
        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("unknown").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &PolicyTarget::Tool {
                server: ServerSlug::new("media").unwrap(),
                tool: LocalToolName::new("run").unwrap(),
            },
            trace_id: &TraceId::new("trace-3").unwrap(),
        });

        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::UnknownProfile);
    }

    #[test]
    fn json_config_round_trips_through_contract_validation() {
        let text = serde_json::to_string(&catalog().control_plane().clone()).unwrap();
        let parsed: GatewayControlPlane = serde_json::from_str(&text).unwrap();
        let catalog = GatewayCatalog::from_control_plane(parsed).unwrap();

        assert_eq!(catalog.server_count(), 1);
        assert_eq!(catalog.profile_count(), 1);
    }

    #[test]
    fn catalog_handle_reads_replaced_catalog_with_new_generation() {
        let handle = GatewayCatalogHandle::new(Arc::new(catalog()));
        let principal = principal(&["media:use"]);
        let target = PolicyTarget::Tool {
            server: ServerSlug::new("media").unwrap(),
            tool: LocalToolName::new("run").unwrap(),
        };
        let first = handle.snapshot();
        let first_decision = first.catalog().decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &target,
            trace_id: &TraceId::new("trace-live-before").unwrap(),
        });

        assert_eq!(first.generation(), 0);
        assert_eq!(first_decision.effect, PolicyEffect::Allow);

        let mut denied_policy = policy();
        denied_policy.rules.clear();
        handle.replace(Arc::new(catalog_with_policy(denied_policy)));
        let second = handle.snapshot();
        let second_decision = second.catalog().decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::ToolsCall,
            target: &target,
            trace_id: &TraceId::new("trace-live-after").unwrap(),
        });

        assert_eq!(second.generation(), 1);
        assert_eq!(second_decision.effect, PolicyEffect::Deny);
    }

    #[test]
    fn gateway_mcp_reads_replaced_catalog_from_existing_handler() {
        let initial_catalog = Arc::new(catalog());
        let mut replacement_control_plane = initial_catalog.control_plane().clone();
        replacement_control_plane.profiles[0].servers.clear();
        replacement_control_plane.policies[0].rules.clear();
        let replacement_catalog =
            Arc::new(GatewayCatalog::from_control_plane(replacement_control_plane).unwrap());
        let handle = GatewayCatalogHandle::new(initial_catalog);
        let state = GatewayState::open(temp_path("live-catalog")).unwrap();
        let internal_token_issuer = GatewayInternalTokenIssuer::new(
            TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER).unwrap(),
            InternalTokenSecret::new("local-dev-internal-token-secret-32-bytes-minimum").unwrap(),
        );
        let mcp = GatewayMcp::new(
            handle.clone(),
            GatewayProfileId::new("default").unwrap(),
            state,
            internal_token_issuer,
        );

        assert!(mcp.get_info().capabilities.tools.is_some());

        handle.replace(replacement_catalog);

        assert!(mcp.get_info().capabilities.tools.is_none());
    }

    #[test]
    fn builds_protected_resource_metadata_for_profile() {
        let catalog = catalog();
        let metadata = catalog
            .protected_resource_metadata(&GatewayProfileId::new("default").unwrap())
            .unwrap();

        assert_eq!(metadata.resource, "https://veoveo.bioma.ai/mcp/default");
        assert_eq!(
            metadata.authorization_servers,
            vec!["https://veoveo.bioma.ai/oauth/default".to_string()]
        );
        assert_eq!(metadata.scopes_supported, vec!["media:use".to_string()]);
        assert_eq!(
            metadata.bearer_methods_supported,
            vec!["header".to_string()]
        );
        assert!(
            metadata
                .extensions
                .contains_key(MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION)
        );
        assert!(
            metadata
                .extensions
                .contains_key(MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION)
        );
    }

    #[test]
    fn builds_authorization_server_metadata_for_profile() {
        let catalog = catalog();
        let metadata = catalog
            .authorization_server_metadata(&GatewayProfileId::new("default").unwrap())
            .unwrap();

        assert_eq!(metadata.issuer, "https://veoveo.bioma.ai/oauth/default");
        assert_eq!(
            metadata.authorization_endpoint.as_deref(),
            Some("https://veoveo.bioma.ai/oauth/default/authorize")
        );
        assert_eq!(
            metadata.token_endpoint,
            "https://veoveo.bioma.ai/oauth/default/token"
        );
        assert_eq!(
            metadata.jwks_uri.as_deref(),
            Some("https://veoveo.bioma.ai/oauth/default/jwks.json")
        );
        assert_eq!(metadata.scopes_supported, vec!["media:use".to_string()]);
        assert_eq!(metadata.response_types_supported, vec!["code".to_string()]);
        assert!(
            metadata
                .grant_types_supported
                .contains(&"authorization_code".to_string())
        );
        assert!(
            metadata
                .grant_types_supported
                .contains(&"client_credentials".to_string())
        );
        assert!(
            metadata
                .grant_types_supported
                .contains(&"urn:ietf:params:oauth:grant-type:jwt-bearer".to_string())
        );
        assert_eq!(
            metadata.code_challenge_methods_supported,
            vec!["S256".to_string()]
        );
        assert!(
            metadata
                .token_endpoint_auth_methods_supported
                .contains(&"private_key_jwt".to_string())
        );
        assert!(
            metadata
                .token_endpoint_auth_methods_supported
                .contains(&"none".to_string())
        );
        assert!(
            metadata
                .token_endpoint_auth_signing_alg_values_supported
                .contains(&"RS256".to_string())
        );
        assert_eq!(
            metadata.authorization_grant_profiles_supported,
            vec!["urn:ietf:params:oauth:grant-profile:id-jag".to_string()]
        );
    }

    #[test]
    fn protected_resource_metadata_includes_policy_required_scopes() {
        let mut policy = policy();
        policy.rules.push(PolicyRule {
            id: PolicyRuleId::new("allow_gateway_admin_write").unwrap(),
            effect: PolicyEffect::Allow,
            actions: BTreeSet::from([GatewayAction::AdminWrite]),
            profiles: BTreeSet::from([GatewayProfileId::new("default").unwrap()]),
            servers: BTreeSet::new(),
            tools: BTreeSet::new(),
            resource_schemes: BTreeSet::new(),
            prompts: BTreeSet::new(),
            principal_ids: BTreeSet::new(),
            tenant_ids: BTreeSet::new(),
            groups: BTreeSet::new(),
            roles: BTreeSet::new(),
            required_scopes: BTreeSet::from([ScopeName::new("gateway:admin").unwrap()]),
            required_data_labels: BTreeSet::new(),
            metadata: Value::Null,
        });
        let catalog = catalog_with_policy(policy);
        let metadata = catalog
            .protected_resource_metadata(&GatewayProfileId::new("default").unwrap())
            .unwrap();

        assert!(
            metadata
                .scopes_supported
                .iter()
                .any(|scope| scope == "media:use")
        );
        assert!(
            metadata
                .scopes_supported
                .iter()
                .any(|scope| scope == "gateway:admin")
        );
    }

    #[test]
    fn gateway_policy_target_ignores_filtered_admin_rules() {
        let mut policy = policy();
        policy.rules[0].actions = BTreeSet::from([GatewayAction::AdminWrite]);
        let catalog = catalog_with_policy(policy);
        let principal = principal(&["media:use"]);
        let decision = catalog.decide(PolicyRequest {
            principal: &principal,
            profile: &GatewayProfileId::new("default").unwrap(),
            action: GatewayAction::AdminWrite,
            target: &PolicyTarget::Gateway,
            trace_id: &TraceId::new("trace-admin-filtered").unwrap(),
        });

        assert_eq!(decision.effect, PolicyEffect::Deny);
        assert_eq!(decision.reason, PolicyReasonCode::PolicyDeny);
    }

    #[test]
    fn builds_www_authenticate_challenge_with_scope() {
        let challenge = www_authenticate_challenge(
            "https://veoveo.bioma.ai/.well-known/oauth-protected-resource/mcp/default",
            &[ScopeName::new("media:use").unwrap()],
        );

        assert_eq!(
            challenge,
            "Bearer resource_metadata=\"https://veoveo.bioma.ai/.well-known/oauth-protected-resource/mcp/default\", scope=\"media:use\""
        );
    }

    #[test]
    fn keeps_contract_validation_errors_visible() {
        let err = GatewayCatalog::from_control_plane(GatewayControlPlane {
            identity_providers: vec![identity_provider()],
            authorization_servers: vec![authorization_server()],
            servers: vec![media_manifest()],
            profiles: vec![{
                let mut profile = profile();
                profile.servers[0].server = ServerSlug::new("simulation").unwrap();
                profile
            }],
            policies: vec![policy()],
            oauth_clients: oauth_clients(),
            oidc_clients: oidc_clients(),
            secrets: vec![signing_secret(), oidc_client_secret()],
            metadata: Value::Null,
        })
        .expect_err("unknown server should fail");

        let root = err
            .downcast_ref::<GatewayControlPlaneError>()
            .expect("contract error should be preserved");
        assert!(matches!(
            root,
            GatewayControlPlaneError::UnknownServer { .. }
        ));
    }

    #[test]
    fn task_id_type_is_available_for_runtime_state() {
        let task = GatewayTaskId::new("gateway-task-1").unwrap();
        assert_eq!(task.as_str(), "gateway-task-1");
    }
}
