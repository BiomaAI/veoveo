use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use validation::{
    validate_oauth_client_registration, validate_oidc_client_registration, validate_policy_set,
    validate_profile_auth_modes, validate_profile_server_exposure, validate_server_apps,
    validate_server_compatibility_helpers, validate_server_upstream,
    validate_server_upstream_tls_material,
};
use wire::{
    resource_uri_template_matches, validate_https_url, validate_local_file_path,
    validate_mount_path, validate_oauth_endpoint_url, validate_oauth_redirect_uri,
    validate_resource_uri, validate_upstream_url, validate_uri_template,
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
mod tenant;
pub use tenant::*;
mod branding;
pub use branding::*;
mod recording_ingest;
pub use recording_ingest::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GatewayControlPlane {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branding: Option<InstallationBranding>,
    pub identity_providers: Vec<IdentityProvider>,
    pub authorization_servers: Vec<ResourceAuthorizationServer>,
    pub servers: Vec<ServerManifest>,
    pub profiles: Vec<GatewayProfile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recording_ingest_resources: Vec<RecordingIngestResource>,
    pub tenants: Vec<TenantDefinition>,
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
    pub fn validate(&self) -> Result<(), GatewayControlPlaneError> {
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
            if server.resource_projection == ResourceProjectionMode::ServerOwned {
                resource_schemes
                    .insert(ResourceScheme::new("ui").expect("ui is a valid resource scheme"));
            }
            validate_server_compatibility_helpers(server)?;
            validate_server_upstream(server)?;
            validate_server_apps(server)?;
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

        let mut profiles = BTreeSet::new();
        let mut profile_by_id = BTreeMap::new();
        let mut protected_resources = BTreeSet::new();
        for profile in &self.profiles {
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

        let mut recording_ingest_resources = BTreeMap::new();
        let mut recording_producers = BTreeSet::new();
        let recording_scope = ScopeName::new("recording:ingest").expect("valid recording scope");
        for resource in &self.recording_ingest_resources {
            if recording_ingest_resources
                .insert(resource.id.clone(), resource)
                .is_some()
            {
                return Err(GatewayControlPlaneError::DuplicateRecordingIngestResource(
                    resource.id.clone(),
                ));
            }
            if !protected_resources.insert(resource.protected_resource.clone()) {
                return Err(GatewayControlPlaneError::DuplicateProtectedResource(
                    resource.protected_resource.clone(),
                ));
            }
            if !authorization_servers.contains_key(&resource.authorization_server) {
                return Err(GatewayControlPlaneError::InvalidRecordingIngestResource {
                    resource: resource.id.clone(),
                    reason: format!(
                        "unknown authorization server `{}`",
                        resource.authorization_server
                    ),
                });
            }
            if !policies.contains(&resource.policy_version) {
                return Err(GatewayControlPlaneError::InvalidRecordingIngestResource {
                    resource: resource.id.clone(),
                    reason: format!("unknown policy version `{}`", resource.policy_version),
                });
            }
            if resource.maximum_batch_bytes == 0 {
                return Err(GatewayControlPlaneError::InvalidRecordingIngestResource {
                    resource: resource.id.clone(),
                    reason: "maximum_batch_bytes must be positive".to_owned(),
                });
            }
            if !resource.required_scopes.contains(&recording_scope) {
                return Err(GatewayControlPlaneError::InvalidRecordingIngestResource {
                    resource: resource.id.clone(),
                    reason: "required_scopes must contain recording:ingest".to_owned(),
                });
            }
            if resource.upstream.security != UpstreamTransportSecurity::ClusterInternalHttp
                || resource
                    .upstream
                    .url
                    .parsed()
                    .ok()
                    .is_none_or(|url| url.scheme() != "http")
            {
                return Err(GatewayControlPlaneError::InvalidRecordingIngestResource {
                    resource: resource.id.clone(),
                    reason: "upstream must use cluster_internal_http over HTTP".to_owned(),
                });
            }
            if resource.producers.is_empty() {
                return Err(GatewayControlPlaneError::InvalidRecordingIngestResource {
                    resource: resource.id.clone(),
                    reason: "at least one recording producer is required".to_owned(),
                });
            }
            for producer in &resource.producers {
                if !recording_producers.insert(producer.id.clone()) {
                    return Err(GatewayControlPlaneError::DuplicateRecordingProducer(
                        producer.id.clone(),
                    ));
                }
                if !tenants.contains(&producer.tenant) {
                    return Err(GatewayControlPlaneError::InvalidRecordingIngestResource {
                        resource: resource.id.clone(),
                        reason: format!(
                            "producer `{}` references unknown tenant `{}`",
                            producer.id, producer.tenant
                        ),
                    });
                }
                if producer.allowed_application_ids.is_empty()
                    || producer.classification.trim().is_empty()
                    || producer.quotas.maximum_concurrent_streams == 0
                    || producer.quotas.maximum_batches_per_minute == 0
                    || producer.quotas.maximum_bytes_per_day == 0
                    || producer.quotas.maximum_stream_bytes == 0
                    || producer.retention.journal_grace_seconds == 0
                    || producer.retention.open_stream_days == 0
                {
                    return Err(GatewayControlPlaneError::InvalidRecordingIngestResource {
                        resource: resource.id.clone(),
                        reason: format!(
                            "producer `{}` has an empty allowlist or non-positive policy limit",
                            producer.id
                        ),
                    });
                }
                if let Some(label) = producer
                    .labels
                    .iter()
                    .find(|label| !data_labels.contains(*label))
                {
                    return Err(GatewayControlPlaneError::InvalidRecordingIngestResource {
                        resource: resource.id.clone(),
                        reason: format!(
                            "producer `{}` references unknown data label `{label}`",
                            producer.id
                        ),
                    });
                }
            }
        }

        for policy in &self.policies {
            validate_policy_set(
                policy,
                &profiles,
                &servers,
                &resource_schemes,
                &data_labels,
                &tenants,
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
                &recording_ingest_resources,
                &policy_by_id,
                &servers,
                &secret_refs,
            )?;
        }
        for resource in &self.recording_ingest_resources {
            for producer in &resource.producers {
                let Some(client) = self
                    .oauth_clients
                    .iter()
                    .find(|client| client.id == producer.oauth_client)
                else {
                    return Err(GatewayControlPlaneError::InvalidRecordingIngestResource {
                        resource: resource.id.clone(),
                        reason: format!(
                            "producer `{}` references unknown OAuth client `{}`",
                            producer.id, producer.oauth_client
                        ),
                    });
                };
                if client.authorization_server != resource.authorization_server
                    || !client
                        .allowed_resources
                        .contains(&resource.protected_resource)
                    || client.tenant.as_ref() != Some(&producer.tenant)
                    || !client
                        .grant_types
                        .contains(&OAuthGrantType::ClientCredentials)
                    || !client
                        .auth_methods
                        .contains(&OAuthClientAuthMethod::PrivateKeyJwt)
                {
                    return Err(GatewayControlPlaneError::InvalidRecordingIngestResource {
                        resource: resource.id.clone(),
                        reason: format!(
                            "producer `{}` OAuth client is not bound to the resource, tenant, and private_key_jwt client-credentials grant",
                            producer.id
                        ),
                    });
                }
            }
        }
        for profile in &self.profiles {
            for auth_mode in &profile.auth_modes {
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

        Ok(())
    }
}

mod error;
pub use error::*;

#[cfg(test)]
mod tests;
