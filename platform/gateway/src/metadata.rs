use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use serde::{Deserialize, Serialize};
use veoveo_mcp_contract::{
    AuthMode, AuthorizationServerId, GatewayProfile, GatewayProfileId, JwksSource,
    OAuthClientAuthMethod, OAuthGrantType, ScopeName,
};

use crate::GatewayCatalog;

const ID_JAG_GRANT_PROFILE: &str = "urn:ietf:params:oauth:grant-profile:id-jag";

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
    pub revocation_endpoint: Option<String>,
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
    pub revocation_endpoint_auth_methods_supported: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authorization_grant_profiles_supported: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AuthorizationExtensionMetadata {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayMetadataError {
    UnknownProfile(GatewayProfileId),
    UnknownAuthorizationServerId(AuthorizationServerId),
    UnknownAuthorizationServer {
        profile: GatewayProfileId,
        authorization_server: AuthorizationServerId,
    },
}

impl fmt::Display for GatewayMetadataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProfile(profile) => write!(f, "unknown gateway profile `{profile}`"),
            Self::UnknownAuthorizationServerId(authorization_server) => {
                write!(
                    f,
                    "unknown resource authorization server `{authorization_server}`"
                )
            }
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

impl GatewayCatalog {
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
        let grant_types = clients
            .iter()
            .flat_map(|client| client.grant_types.iter().copied())
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
        let supports_public_refresh_revocation = clients.iter().any(|client| {
            client.grant_types.contains(&OAuthGrantType::RefreshToken)
                && client.auth_methods.contains(&OAuthClientAuthMethod::None)
        });

        Ok(AuthorizationServerMetadata {
            issuer: authorization_server.issuer.to_string(),
            authorization_endpoint: authorization_server
                .authorization_endpoint
                .as_ref()
                .map(ToString::to_string),
            token_endpoint: authorization_server.token_endpoint.to_string(),
            revocation_endpoint: supports_public_refresh_revocation.then(|| {
                format!(
                    "{}/revoke",
                    authorization_server.issuer.as_str().trim_end_matches('/')
                )
            }),
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
            revocation_endpoint_auth_methods_supported: if supports_public_refresh_revocation {
                vec!["none".to_string()]
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

    pub fn authorization_server_metadata_for_server(
        &self,
        authorization_server_id: &AuthorizationServerId,
    ) -> Result<AuthorizationServerMetadata, GatewayMetadataError> {
        let authorization_server = self
            .authorization_server(authorization_server_id)
            .ok_or_else(|| {
                GatewayMetadataError::UnknownAuthorizationServerId(authorization_server_id.clone())
            })?;
        let clients = self.authorization_server_oauth_clients(authorization_server_id);
        let profiles = self.authorization_server_profiles(authorization_server_id);
        let token_auth_methods = clients
            .iter()
            .flat_map(|client| client.auth_methods.iter().copied())
            .map(oauth_client_auth_method_name)
            .collect::<BTreeSet<_>>();
        let auth_modes = profiles
            .iter()
            .flat_map(|profile| profile.auth_modes.iter().copied())
            .collect::<BTreeSet<_>>();
        let grant_types = clients
            .iter()
            .flat_map(|client| client.grant_types.iter().copied())
            .map(oauth_grant_type_name)
            .collect::<BTreeSet<_>>();
        let scopes = profiles
            .iter()
            .flat_map(|profile| self.profile_supported_scopes(profile).into_iter())
            .collect::<BTreeSet<_>>();
        let supports_authorization_code = auth_modes.contains(&AuthMode::OidcAuthorizationCodePkce);
        let supports_private_key_jwt = clients.iter().any(|client| {
            client
                .auth_methods
                .contains(&OAuthClientAuthMethod::PrivateKeyJwt)
        });
        let supports_public_refresh_revocation = clients.iter().any(|client| {
            client.grant_types.contains(&OAuthGrantType::RefreshToken)
                && client.auth_methods.contains(&OAuthClientAuthMethod::None)
        });

        Ok(AuthorizationServerMetadata {
            issuer: authorization_server.issuer.to_string(),
            authorization_endpoint: authorization_server
                .authorization_endpoint
                .as_ref()
                .map(ToString::to_string),
            token_endpoint: authorization_server.token_endpoint.to_string(),
            revocation_endpoint: supports_public_refresh_revocation.then(|| {
                format!(
                    "{}/revoke",
                    authorization_server.issuer.as_str().trim_end_matches('/')
                )
            }),
            jwks_uri: match &authorization_server.jwks {
                JwksSource::Remote { jwks_uri } => Some(jwks_uri.to_string()),
                JwksSource::File { .. } => None,
            },
            scopes_supported: scopes.into_iter().map(|scope| scope.to_string()).collect(),
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
            revocation_endpoint_auth_methods_supported: if supports_public_refresh_revocation {
                vec!["none".to_string()]
            } else {
                Vec::new()
            },
            authorization_grant_profiles_supported: if auth_modes
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
                if (rule.protected_resources.is_empty()
                    && (rule.profiles.is_empty() || rule.profiles.contains(&profile.id)))
                    || rule
                        .protected_resources
                        .contains(&profile.protected_resource)
                {
                    scopes.extend(rule.required_scopes.iter().cloned());
                }
            }
        }
        scopes
    }
}

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
        OAuthGrantType::RefreshToken => "refresh_token",
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
