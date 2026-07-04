use std::{collections::BTreeMap, fs, path::Path, sync::Arc};

use anyhow::{Context, Result};
use parking_lot::RwLock;
use veoveo_mcp_contract::{
    AuthorizationServerId, DataLabelDefinition, DataLabelId, GatewayControlPlane, GatewayProfile,
    GatewayProfileId, IdentityProvider, IdentityProviderId, OAuthClientId, OAuthClientRegistration,
    OidcClientRegistrationId, PolicySet, PolicyVersion, ResourceAuthorizationServer,
    SecretReference, SecretReferenceId, ServerManifest, ServerSlug, TenantDefinition, TenantId,
};

use crate::policy::{exposure_contains, resource_scheme};

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
    data_labels: BTreeMap<DataLabelId, usize>,
    tenants: BTreeMap<TenantId, usize>,
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
        let data_labels = control_plane
            .data_labels
            .iter()
            .enumerate()
            .map(|(index, data_label)| (data_label.id.clone(), index))
            .collect();
        let tenants = control_plane
            .tenants
            .iter()
            .enumerate()
            .map(|(index, tenant)| (tenant.id.clone(), index))
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
            data_labels,
            tenants,
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
                && client.allowed_profiles.contains(&profile.id)
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

    pub fn data_label(&self, label: &DataLabelId) -> Option<&DataLabelDefinition> {
        self.data_labels
            .get(label)
            .map(|index| &self.control_plane.data_labels[*index])
    }

    pub fn tenant(&self, tenant: &TenantId) -> Option<&TenantDefinition> {
        self.tenants
            .get(tenant)
            .map(|index| &self.control_plane.tenants[*index])
    }
}

#[cfg(test)]
mod tests;
