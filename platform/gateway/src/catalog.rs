use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::Arc,
};

use anyhow::{Context, Result};
use parking_lot::RwLock;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    AuthorizationServerId, DataLabelDefinition, DataLabelId, GatewayControlPlane, GatewayProfile,
    GatewayProfileId, IdentityProvider, IdentityProviderId, InvocationAuthority, InvocationMode,
    InvocationProvenance, OAuthClientId, OAuthClientRegistration, OidcClientRegistrationId,
    PolicySet, PolicyVersion, Principal, PrincipalId, PrincipalKind, ProtectedResourceName,
    RecordingIngestResource, RecordingProducerId, RecordingProducerRegistration,
    ResourceAuthorizationServer, ResourceProjectionMode, SecretReference, SecretReferenceId,
    ServerManifest, ServerSlug, TenantDefinition, TenantId, TokenSubject, WorkContextDefinition,
    WorkContextId, WorkContextMembershipLevel,
};

use crate::policy::{exposure_contains, resource_scheme};
use crate::{AuthenticatedSubject, VerifiedAccessToken};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayAuthorityError {
    UnknownOAuthClient(OAuthClientId),
    UnknownWorkContext(WorkContextId),
    MissingTenant(PrincipalId),
    TenantMismatch {
        context: WorkContextId,
        principal: PrincipalId,
    },
    InvocationModeMismatch {
        client: OAuthClientId,
        expected: InvocationMode,
        received: InvocationMode,
    },
    InvalidProvenance(InvocationMode),
    MembershipDenied {
        context: WorkContextId,
        principal: PrincipalId,
    },
    InvalidActorIdentity,
}

impl std::fmt::Display for GatewayAuthorityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownOAuthClient(client) => {
                write!(formatter, "unknown OAuth client `{client}`")
            }
            Self::UnknownWorkContext(context) => {
                write!(formatter, "unknown Work Context `{context}`")
            }
            Self::MissingTenant(principal) => {
                write!(formatter, "principal `{principal}` has no tenant")
            }
            Self::TenantMismatch { context, principal } => write!(
                formatter,
                "Work Context `{context}` and principal `{principal}` belong to different tenants"
            ),
            Self::InvocationModeMismatch {
                client,
                expected,
                received,
            } => write!(
                formatter,
                "OAuth client `{client}` requires `{expected:?}` invocation and received `{received:?}`"
            ),
            Self::InvalidProvenance(mode) => {
                write!(formatter, "`{mode:?}` invocation provenance is invalid")
            }
            Self::MembershipDenied { context, principal } => write!(
                formatter,
                "principal `{principal}` has no membership in Work Context `{context}`"
            ),
            Self::InvalidActorIdentity => formatter.write_str("resolved actor identity is invalid"),
        }
    }
}

impl std::error::Error for GatewayAuthorityError {}

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
    configuration_sha256: [u8; 32],
    identity_providers: BTreeMap<IdentityProviderId, usize>,
    authorization_servers: BTreeMap<AuthorizationServerId, usize>,
    servers: BTreeMap<ServerSlug, usize>,
    profiles: BTreeMap<GatewayProfileId, usize>,
    recording_ingest_resources: BTreeMap<ProtectedResourceName, usize>,
    recording_producers: BTreeMap<RecordingProducerId, (usize, usize)>,
    policies: BTreeMap<PolicyVersion, usize>,
    data_labels: BTreeMap<DataLabelId, usize>,
    tenants: BTreeMap<TenantId, usize>,
    work_contexts: BTreeMap<WorkContextId, usize>,
    oauth_clients: BTreeMap<OAuthClientId, usize>,
    oidc_clients: BTreeMap<OidcClientRegistrationId, usize>,
    secrets: BTreeMap<SecretReferenceId, usize>,
}

impl GatewayCatalog {
    pub fn from_control_plane(control_plane: GatewayControlPlane) -> Result<Self> {
        control_plane.validate()?;
        let configuration_sha256 = Sha256::digest(serde_json::to_vec(&control_plane)?).into();

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
        let recording_ingest_resources = control_plane
            .recording_ingest_resources
            .iter()
            .enumerate()
            .map(|(index, resource)| (resource.id.clone(), index))
            .collect();
        let recording_producers = control_plane
            .recording_ingest_resources
            .iter()
            .enumerate()
            .flat_map(|(resource_index, resource)| {
                resource
                    .producers
                    .iter()
                    .enumerate()
                    .map(move |(producer_index, producer)| {
                        (producer.id.clone(), (resource_index, producer_index))
                    })
            })
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
        let work_contexts = control_plane
            .work_contexts
            .iter()
            .enumerate()
            .map(|(index, context)| (context.id.clone(), index))
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
            configuration_sha256,
            identity_providers,
            authorization_servers,
            servers,
            profiles,
            recording_ingest_resources,
            recording_producers,
            policies,
            data_labels,
            tenants,
            work_contexts,
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

    pub fn configuration_sha256(&self) -> [u8; 32] {
        self.configuration_sha256
    }

    pub fn profiles(&self) -> impl Iterator<Item = &GatewayProfile> {
        self.control_plane.profiles.iter()
    }

    pub fn authorization_servers(&self) -> impl Iterator<Item = &ResourceAuthorizationServer> {
        self.control_plane.authorization_servers.iter()
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

    pub fn authorization_server_by_issuer(
        &self,
        issuer: &str,
    ) -> Option<&ResourceAuthorizationServer> {
        self.control_plane
            .authorization_servers
            .iter()
            .find(|authorization_server| authorization_server.issuer.as_str() == issuer)
    }

    pub fn oauth_client(&self, client_id: &OAuthClientId) -> Option<&OAuthClientRegistration> {
        self.oauth_clients
            .get(client_id)
            .map(|index| &self.control_plane.oauth_clients[*index])
    }

    pub fn work_context(&self, context_id: &WorkContextId) -> Option<&WorkContextDefinition> {
        self.work_contexts
            .get(context_id)
            .map(|index| &self.control_plane.work_contexts[*index])
    }

    pub fn work_contexts(&self) -> impl Iterator<Item = &WorkContextDefinition> {
        self.control_plane.work_contexts.iter()
    }

    pub fn work_context_membership(
        &self,
        client_id: &OAuthClientId,
        context_id: &WorkContextId,
        principal: &Principal,
    ) -> Result<WorkContextMembershipLevel, GatewayAuthorityError> {
        let client = self
            .oauth_client(client_id)
            .ok_or_else(|| GatewayAuthorityError::UnknownOAuthClient(client_id.clone()))?;
        let context = self
            .work_context(context_id)
            .ok_or_else(|| GatewayAuthorityError::UnknownWorkContext(context_id.clone()))?;
        let tenant = principal
            .tenant
            .as_ref()
            .ok_or_else(|| GatewayAuthorityError::MissingTenant(principal.id.clone()))?;
        if tenant != &context.tenant {
            return Err(GatewayAuthorityError::TenantMismatch {
                context: context.id.clone(),
                principal: principal.id.clone(),
            });
        }
        context
            .membership_for(principal, &client.id)
            .ok_or_else(|| GatewayAuthorityError::MembershipDenied {
                context: context.id.clone(),
                principal: principal.id.clone(),
            })
    }

    pub fn resolve_authenticated_subject(
        &self,
        verified: VerifiedAccessToken,
    ) -> Result<AuthenticatedSubject, GatewayAuthorityError> {
        let VerifiedAccessToken {
            access_token,
            principal,
        } = verified;
        let client = self
            .oauth_client(&access_token.oauth_client_id)
            .ok_or_else(|| {
                GatewayAuthorityError::UnknownOAuthClient(access_token.oauth_client_id.clone())
            })?;
        let context = self
            .work_context(&access_token.work_context)
            .ok_or_else(|| {
                GatewayAuthorityError::UnknownWorkContext(access_token.work_context.clone())
            })?;
        if access_token.invocation_mode != client.invocation_mode {
            return Err(GatewayAuthorityError::InvocationModeMismatch {
                client: client.id.clone(),
                expected: client.invocation_mode,
                received: access_token.invocation_mode,
            });
        }
        let membership = self.work_context_membership(
            &access_token.oauth_client_id,
            &access_token.work_context,
            &principal,
        )?;
        let (actor, provenance) = match access_token.invocation_mode {
            InvocationMode::Direct => {
                if access_token.initiator.as_ref() != Some(&principal.id)
                    || access_token.delegation_id.is_some()
                {
                    return Err(GatewayAuthorityError::InvalidProvenance(
                        InvocationMode::Direct,
                    ));
                }
                (
                    principal.clone(),
                    InvocationProvenance::Direct {
                        initiator: principal.id.clone(),
                    },
                )
            }
            InvocationMode::Delegated => {
                let Some(delegation_id) = access_token.delegation_id.clone() else {
                    return Err(GatewayAuthorityError::InvalidProvenance(
                        InvocationMode::Delegated,
                    ));
                };
                if principal.kind != PrincipalKind::User
                    || access_token.initiator.as_ref() != Some(&principal.id)
                {
                    return Err(GatewayAuthorityError::InvalidProvenance(
                        InvocationMode::Delegated,
                    ));
                }
                let subject = TokenSubject::new(client.id.as_str())
                    .map_err(|_| GatewayAuthorityError::InvalidActorIdentity)?;
                let actor_id = PrincipalId::new(format!("{}#{subject}", access_token.issuer))
                    .map_err(|_| GatewayAuthorityError::InvalidActorIdentity)?;
                (
                    Principal {
                        id: actor_id,
                        kind: PrincipalKind::Service,
                        issuer: access_token.issuer.clone(),
                        subject,
                        tenant: principal.tenant.clone(),
                        groups: BTreeSet::new(),
                        group_roles: BTreeSet::new(),
                        roles: BTreeSet::new(),
                        scopes: principal.scopes.clone(),
                        data_labels: principal.data_labels.clone(),
                        assurances: principal.assurances.clone(),
                        authenticated_at: principal.authenticated_at,
                    },
                    InvocationProvenance::Delegated {
                        initiator: principal.id.clone(),
                        delegation_id,
                    },
                )
            }
            InvocationMode::Automated => {
                if principal.kind != PrincipalKind::Service
                    || principal.subject.as_str() != client.id.as_str()
                    || access_token.initiator.is_some()
                    || access_token.delegation_id.is_some()
                {
                    return Err(GatewayAuthorityError::InvalidProvenance(
                        InvocationMode::Automated,
                    ));
                }
                (principal.clone(), InvocationProvenance::Automated)
            }
        };
        Ok(AuthenticatedSubject {
            access_token,
            principal,
            actor,
            authority: InvocationAuthority {
                work_context: context.id.clone(),
                tenant: context.tenant.clone(),
                membership,
                policy_revision: context.policy_revision.clone(),
                output_policy: context.output_policy.clone(),
                provenance,
            },
        })
    }

    pub fn profile_oauth_clients(&self, profile: &GatewayProfile) -> Vec<&OAuthClientRegistration> {
        self.control_plane
            .oauth_clients
            .iter()
            .filter(|client| {
                client.authorization_server == profile.authorization_server
                    && client
                        .allowed_resources
                        .contains(&profile.protected_resource)
            })
            .collect()
    }

    pub fn authorization_server_oauth_clients(
        &self,
        authorization_server_id: &AuthorizationServerId,
    ) -> Vec<&OAuthClientRegistration> {
        self.control_plane
            .oauth_clients
            .iter()
            .filter(|client| &client.authorization_server == authorization_server_id)
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
                && client
                    .allowed_resources
                    .contains(&profile.protected_resource)
        })
    }

    pub fn profile_by_protected_resource(&self, resource: &str) -> Option<&GatewayProfile> {
        self.control_plane
            .profiles
            .iter()
            .find(|profile| profile.protected_resource.as_str() == resource)
    }

    pub fn client_single_profile(
        &self,
        client: &OAuthClientRegistration,
    ) -> Option<&GatewayProfile> {
        if client.allowed_resources.len() != 1 {
            return None;
        }
        client.allowed_resources.iter().next().and_then(|resource| {
            self.control_plane
                .profiles
                .iter()
                .find(|profile| &profile.protected_resource == resource)
        })
    }

    pub fn recording_ingest_resource(
        &self,
        id: &ProtectedResourceName,
    ) -> Option<&RecordingIngestResource> {
        self.recording_ingest_resources
            .get(id)
            .map(|index| &self.control_plane.recording_ingest_resources[*index])
    }

    pub fn recording_ingest_resources(&self) -> impl Iterator<Item = &RecordingIngestResource> {
        self.control_plane.recording_ingest_resources.iter()
    }

    pub fn single_recording_ingest_resource(&self) -> Option<&RecordingIngestResource> {
        let mut resources = self.control_plane.recording_ingest_resources.iter();
        let resource = resources.next()?;
        resources.next().is_none().then_some(resource)
    }

    pub fn recording_ingest_resource_by_protected_resource(
        &self,
        resource: &str,
    ) -> Option<&RecordingIngestResource> {
        self.control_plane
            .recording_ingest_resources
            .iter()
            .find(|candidate| candidate.protected_resource.as_str() == resource)
    }

    pub fn recording_producer(
        &self,
        producer_id: &RecordingProducerId,
    ) -> Option<(&RecordingIngestResource, &RecordingProducerRegistration)> {
        self.recording_producers
            .get(producer_id)
            .map(|(resource_index, producer_index)| {
                let resource = &self.control_plane.recording_ingest_resources[*resource_index];
                (resource, &resource.producers[*producer_index])
            })
    }

    pub fn recording_producer_for_client<'a>(
        &self,
        resource: &'a RecordingIngestResource,
        client_id: &OAuthClientId,
    ) -> Option<&'a RecordingProducerRegistration> {
        resource
            .producers
            .iter()
            .find(|producer| &producer.oauth_client == client_id)
    }

    pub fn authorization_server_profiles(
        &self,
        authorization_server_id: &AuthorizationServerId,
    ) -> Vec<&GatewayProfile> {
        self.control_plane
            .profiles
            .iter()
            .filter(|profile| &profile.authorization_server == authorization_server_id)
            .collect()
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

    pub fn is_compatibility_helper(
        &self,
        server_slug: &ServerSlug,
        tool: &veoveo_mcp_contract::LocalToolName,
    ) -> bool {
        self.server(server_slug).is_some_and(|server| {
            server
                .compatibility_helpers
                .iter()
                .any(|helper| helper == tool)
        })
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
            .find(|(_, server)| server_owns_gateway_resource_uri(server, uri, &scheme))
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

fn server_owns_gateway_resource_uri(
    server: &ServerManifest,
    uri: &str,
    scheme: &veoveo_mcp_contract::ResourceScheme,
) -> bool {
    server.uri_scheme == *scheme
        || (server.resource_projection == ResourceProjectionMode::ServerOwned
            && scheme.as_str() == "ui"
            && uri.starts_with(&format!("ui://{}/", server.slug.as_str())))
}

#[cfg(test)]
mod tests;
