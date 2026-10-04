use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
use veoveo_gateway_contract::AppResourceOperation;
use veoveo_gateway_contract::{AppResourceDependency, AppToolDependency};
use veoveo_gateway_contract::{
    AuthorizationServerId, CertificateAuthoritySource, ProtectedResourceId, ProtectedResourceName,
    SecretOwner, SecretPurpose, SecretReference, SecretReferenceId, UpstreamTransportSecurity,
    UpstreamUrl,
};
use veoveo_types::{
    DataLabelId, DelegationId, GroupId, IdentifierError, PolicyVersion, PrincipalId,
    ResourceScheme, ResourceUri, RoleId, ScopeName, TenantId, WorkContextId,
};
pub use veoveo_types::{ResourceSelector, ResourceUriPrefix, ResourceUriTemplate};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use validation::{
    validate_app_resource_dependencies, validate_app_tool_dependencies,
    validate_oauth_client_registration, validate_oidc_client_registration, validate_policy_set,
    validate_profile_auth_modes, validate_profile_server_exposure, validate_server_apps,
    validate_server_capabilities, validate_server_compatibility_helpers, validate_server_upstream,
    validate_server_upstream_tls_material,
};
use wire::{
    validate_https_url, validate_local_file_path, validate_mount_path, validate_oauth_endpoint_url,
    validate_oauth_redirect_uri,
};

pub const MCP_ENTERPRISE_MANAGED_AUTHORIZATION_EXTENSION: &str =
    "io.modelcontextprotocol/enterprise-managed-authorization";
pub const MCP_OAUTH_CLIENT_CREDENTIALS_EXTENSION: &str =
    "io.modelcontextprotocol/oauth-client-credentials";
mod knowledge;
mod policy;
use veoveo_gateway_contract::GatewayAction;
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
mod tenant;
pub use tenant::*;
mod branding;
pub use branding::*;
mod console;
pub use console::*;
mod catalog_schema;
mod catalog_wire;
pub use catalog_schema::composed_gateway_schema;
pub const GATEWAY_CORE_FIELDS: &[&str] = &[
    "branding",
    "identity_providers",
    "authorization_servers",
    "servers",
    "profiles",
    "tenants",
    "work_contexts",
    "policies",
    "data_labels",
    "oauth_clients",
    "oidc_clients",
    "secrets",
    "metadata",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct GatewayControlPlane {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branding: Option<InstallationBranding>,
    pub identity_providers: Vec<IdentityProvider>,
    pub authorization_servers: Vec<ResourceAuthorizationServer>,
    pub servers: Vec<ServerManifest>,
    pub profiles: Vec<GatewayProfile>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, Value>,
    pub tenants: Vec<TenantDefinition>,
    pub work_contexts: Vec<crate::WorkContextDefinition>,
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
    SeedFile,
}

impl GatewayControlPlane {
    /// Public files that must be mounted beside this control-plane document.
    #[must_use]
    pub fn jwks_file_paths(&self) -> BTreeSet<&str> {
        self.identity_providers
            .iter()
            .map(|provider| &provider.jwks)
            .chain(self.authorization_servers.iter().map(|server| &server.jwks))
            .chain(
                self.oauth_clients
                    .iter()
                    .filter_map(|client| client.jwks.as_ref()),
            )
            .filter_map(|source| match source {
                JwksSource::File { path } => Some(path.as_str()),
                JwksSource::Remote { .. } => None,
            })
            .collect()
    }

    /// Public CA bundles that must be mounted beside this control-plane document.
    #[must_use]
    pub fn certificate_authority_file_paths(&self) -> BTreeSet<&str> {
        self.identity_providers
            .iter()
            .flat_map(|provider| provider.trusted_certificate_authorities.iter())
            .chain(
                self.servers
                    .iter()
                    .flat_map(|server| server.upstream.trusted_certificate_authorities.iter()),
            )
            .map(|source| match source {
                CertificateAuthoritySource::File { path } => path.as_str(),
            })
            .collect()
    }

    pub fn validate(
        &self,
        registry: &veoveo_gateway_contract::CatalogRegistry,
    ) -> Result<veoveo_gateway_contract::AdmittedCatalogSections, GatewayControlPlaneError> {
        registry
            .check_reserved(
                GATEWAY_CORE_FIELDS,
                &[
                    "gateway",
                    "server",
                    "tool",
                    "resource",
                    "resource_template",
                    "prompt",
                    "task",
                    "platform_task",
                    "artifact",
                    "usage",
                ],
            )
            .map_err(GatewayControlPlaneError::CatalogAdmission)?;
        if self
            .extensions
            .keys()
            .any(|key| GATEWAY_CORE_FIELDS.contains(&key.as_str()))
        {
            return Err(GatewayControlPlaneError::CatalogAdmission(
                veoveo_types::ExtensionError::new("catalog section collides with a core field"),
            ));
        }
        registry
            .action_key::<GatewayAction>()
            .map_err(GatewayControlPlaneError::CatalogAdmission)?;
        let facts = self.catalog_facts();
        let sections = registry
            .admit_sections(&self.extensions, &facts)
            .map_err(GatewayControlPlaneError::CatalogAdmission)?;
        if let Some(branding) = &self.branding {
            branding.validate()?;
        }

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
        let mut mount_paths = BTreeMap::new();
        let mut mcp_paths = BTreeMap::new();
        let mut gateway_routes = BTreeMap::new();
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
            if let Some(first) = mount_paths.insert(server.mount_path.clone(), server.slug.clone())
            {
                return Err(GatewayControlPlaneError::DuplicateMountPath {
                    path: server.mount_path.clone(),
                    first,
                    second: server.slug.clone(),
                });
            }
            if let Some(first) = mcp_paths.insert(server.mcp_path.clone(), server.slug.clone()) {
                return Err(GatewayControlPlaneError::DuplicateMcpPath {
                    path: server.mcp_path.clone(),
                    first,
                    second: server.slug.clone(),
                });
            }
            for path in std::iter::once(&server.mount_path)
                .chain(std::iter::once(&server.mcp_path))
                .chain(server.owned_routes.iter().map(|route| &route.path))
            {
                if let Some(first) = gateway_routes.insert(path.clone(), server.slug.clone()) {
                    return Err(GatewayControlPlaneError::DuplicateGatewayRoute {
                        path: path.clone(),
                        first,
                        second: server.slug.clone(),
                    });
                }
            }
            if server.resource_projection == ResourceProjectionMode::ServerOwned {
                resource_schemes
                    .insert(ResourceScheme::parse("ui").expect("ui is a valid resource scheme"));
            }
            validate_server_compatibility_helpers(server)?;
            validate_server_upstream(server)?;
            validate_server_apps(server)?;
            validate_server_capabilities(server)?;
        }
        for server in &self.servers {
            for scheme in &server.referenced_resource_schemes {
                if !self
                    .servers
                    .iter()
                    .any(|candidate| candidate.uri_scheme == *scheme)
                {
                    return Err(
                        GatewayControlPlaneError::UnknownServerReferencedResourceScheme {
                            server: server.slug.clone(),
                            scheme: scheme.clone(),
                        },
                    );
                }
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

        let mut data_labels = BTreeSet::new();
        for data_label in &self.data_labels {
            if !data_labels.insert(data_label.id.clone()) {
                return Err(GatewayControlPlaneError::DuplicateDataLabel(
                    data_label.id.clone(),
                ));
            }
        }
        for server in &self.servers {
            knowledge::validate_knowledge_approvals(server, &data_labels)?;
            validate_app_resource_dependencies(server, &servers, &data_labels)?;
            validate_app_tool_dependencies(server, &servers, &data_labels)?;
        }

        let mut tenants = BTreeSet::new();
        for tenant in &self.tenants {
            if !tenants.insert(tenant.id.clone()) {
                return Err(GatewayControlPlaneError::DuplicateTenant(tenant.id.clone()));
            }
        }
        for identity_provider in &self.identity_providers {
            if let Some(mapping) = &identity_provider.claim_mapping.tenant {
                for tenant in mapping.values.values() {
                    if !tenants.contains(tenant) {
                        return Err(
                            GatewayControlPlaneError::UnknownIdentityProviderMappedTenant {
                                identity_provider: identity_provider.id.clone(),
                                tenant: tenant.clone(),
                            },
                        );
                    }
                }
            }
        }

        let mut work_contexts = BTreeMap::new();
        for context in &self.work_contexts {
            if work_contexts.insert(context.id.clone(), context).is_some() {
                return Err(GatewayControlPlaneError::DuplicateWorkContext(
                    context.id.clone(),
                ));
            }
            if !tenants.contains(&context.tenant) {
                return Err(GatewayControlPlaneError::InvalidWorkContext {
                    context: context.id.clone(),
                    reason: format!("references unknown tenant `{}`", context.tenant),
                });
            }
            if !policies.contains(&context.policy_revision) {
                return Err(GatewayControlPlaneError::InvalidWorkContext {
                    context: context.id.clone(),
                    reason: format!(
                        "references unknown policy revision `{}`",
                        context.policy_revision
                    ),
                });
            }
            if context.title.trim().is_empty() {
                return Err(GatewayControlPlaneError::InvalidWorkContext {
                    context: context.id.clone(),
                    reason: "title must not be empty".to_owned(),
                });
            }
            if context.memberships.is_empty()
                || context
                    .memberships
                    .iter()
                    .any(|membership| !membership.has_selector())
            {
                return Err(GatewayControlPlaneError::InvalidWorkContext {
                    context: context.id.clone(),
                    reason: "each context requires at least one membership rule with a selector"
                        .to_owned(),
                });
            }
            if let Some(label) = context
                .output_policy
                .data_labels
                .iter()
                .chain(context.output_policy.classification.iter())
                .find(|label| !data_labels.contains(*label))
            {
                return Err(GatewayControlPlaneError::InvalidWorkContext {
                    context: context.id.clone(),
                    reason: format!("output policy references unknown data label `{label}`"),
                });
            }
        }

        let mut profiles = BTreeSet::new();
        let mut profile_by_id = BTreeMap::new();
        let mut protected_resources = BTreeSet::new();
        for profile in &self.profiles {
            if let Some(policy) = &profile.artifact_upload {
                policy.validate().map_err(|_| {
                    GatewayControlPlaneError::InvalidArtifactUploadPolicy(profile.id.clone())
                })?;
            }
            if !profiles.insert(profile.id.clone()) {
                return Err(GatewayControlPlaneError::DuplicateProfile(
                    profile.id.clone(),
                ));
            }
            profile_by_id.insert(profile.id.clone(), profile);
            if !protected_resources.insert(profile.protected_resource.clone()) {
                return Err(GatewayControlPlaneError::DuplicateProtectedResource(
                    profile.protected_resource.clone(),
                ));
            }
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

        let mut contributed_resources = BTreeMap::new();
        for resource in sections.protected_resources() {
            if contributed_resources
                .insert(resource.name.clone(), resource)
                .is_some()
                || !protected_resources.insert(resource.resource.clone())
            {
                return Err(GatewayControlPlaneError::DuplicateProtectedResource(
                    resource.resource.clone(),
                ));
            }
            if !authorization_servers.contains_key(&resource.authorization_server)
                || !policies.contains(&resource.policy_version)
            {
                return Err(GatewayControlPlaneError::CatalogAdmission(
                    veoveo_types::ExtensionError::new(
                        "contributed protected resource references unknown authorization server or policy",
                    ),
                ));
            }
        }

        for policy in &self.policies {
            validate_policy_set(
                policy,
                validation::PolicyValidation {
                    profiles: &profiles,
                    protected_resources: &protected_resources,
                    servers: &servers,
                    resource_schemes: &resource_schemes,
                    data_labels: &data_labels,
                    tenants: &tenants,
                    registry,
                },
            )?;
        }

        let mut secrets = BTreeSet::new();
        let mut secret_refs = BTreeMap::new();
        for secret in &self.secrets {
            if !secrets.insert(secret.id.clone()) {
                return Err(GatewayControlPlaneError::DuplicateSecret(secret.id.clone()));
            }
            secret_refs.insert(secret.id.clone(), secret);
            match &secret.owner {
                SecretOwner::Gateway => {}
                SecretOwner::Tenant { tenant } => {
                    if !tenants.contains(tenant) {
                        return Err(GatewayControlPlaneError::UnknownSecretOwnerTenant {
                            secret: secret.id.clone(),
                            tenant: tenant.clone(),
                        });
                    }
                }
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
                &profile_by_id,
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
                &contributed_resources,
                &policy_by_id,
                &servers,
                &secret_refs,
            )?;
            knowledge::validate_indexing_client(client, &profile_by_id, &servers, &policy_by_id)?;
            let Some(context) = work_contexts.get(&client.default_work_context) else {
                return Err(GatewayControlPlaneError::UnknownOAuthClientWorkContext {
                    client: client.id.clone(),
                    context: client.default_work_context.clone(),
                });
            };
            if client
                .tenant
                .as_ref()
                .is_some_and(|tenant| tenant != &context.tenant)
            {
                return Err(GatewayControlPlaneError::InvalidWorkContext {
                    context: context.id.clone(),
                    reason: format!("OAuth client `{}` belongs to a different tenant", client.id),
                });
            }
            let mode_matches = match client.invocation_mode {
                veoveo_types::InvocationMode::Direct => client
                    .grant_types
                    .contains(&OAuthGrantType::AuthorizationCodePkce),
                veoveo_types::InvocationMode::Delegated => client
                    .grant_types
                    .contains(&OAuthGrantType::EnterpriseManagedAuthorization),
                veoveo_types::InvocationMode::Automated => client
                    .grant_types
                    .contains(&OAuthGrantType::ClientCredentials),
            };
            if !mode_matches {
                return Err(
                    GatewayControlPlaneError::OAuthClientInvocationModeMismatch {
                        client: client.id.clone(),
                        mode: client.invocation_mode,
                    },
                );
            }
        }
        for profile in &self.profiles {
            for auth_mode in &profile.auth_modes {
                // Managed service clients are admitted durably after installation.
                // A profile may start empty; each request still requires an effective
                // registration bound to its issuer, resource, scopes and authority.
                if *auth_mode == AuthMode::OAuthClientCredentials {
                    continue;
                }
                let required_grant = OAuthGrantType::from(*auth_mode);
                let has_client = self.oauth_clients.iter().any(|client| {
                    client.authorization_server == profile.authorization_server
                        && client
                            .allowed_resources
                            .contains(&profile.protected_resource)
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
                        && client
                            .allowed_resources
                            .contains(&profile.protected_resource)
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

        Ok(sections)
    }
}

mod error;
pub use error::*;

#[cfg(test)]
mod tests;
