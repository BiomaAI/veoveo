use std::{collections::BTreeMap, env, error::Error, fmt, num::NonZeroU64, time::Duration};

use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use url::Url;
use vaultrs::{
    client::{VaultClient, VaultClientSettingsBuilder},
    kv2,
};
use veoveo_mcp_contract::{SecretPurpose, SecretReference, SecretReferenceId, SecretSource};

use crate::GatewayCatalog;

const VAULT_SECRET_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Default)]
pub struct GatewaySecretResolver;

impl GatewaySecretResolver {
    pub fn new() -> Self {
        Self
    }

    pub async fn resolve_string(
        &self,
        catalog: &GatewayCatalog,
        secret_id: &SecretReferenceId,
        expected_purpose: SecretPurpose,
    ) -> Result<ResolvedSecretString, SecretResolverError> {
        let secret = catalog
            .secret_reference(secret_id)
            .ok_or_else(|| SecretResolverError::UnknownSecret(secret_id.clone()))?;
        self.resolve_reference(secret, expected_purpose).await
    }

    pub async fn resolve_reference(
        &self,
        secret: &SecretReference,
        expected_purpose: SecretPurpose,
    ) -> Result<ResolvedSecretString, SecretResolverError> {
        if secret.purpose != expected_purpose {
            return Err(SecretResolverError::PurposeMismatch {
                id: secret.id.clone(),
                actual: secret.purpose,
                expected: expected_purpose,
            });
        }

        match secret.source {
            SecretSource::Env => resolve_env_secret(secret),
            SecretSource::Vault | SecretSource::HcpVault => {
                let config = VaultClientConfig::from_env(secret.source)?;
                resolve_vault_kv2_secret(secret, &config).await
            }
            source => Err(SecretResolverError::UnsupportedSource {
                id: secret.id.clone(),
                source,
            }),
        }
    }
}

#[derive(Clone)]
pub struct ResolvedSecretString {
    id: SecretReferenceId,
    source: SecretSource,
    purpose: SecretPurpose,
    value: SecretString,
}

impl ResolvedSecretString {
    pub fn id(&self) -> &SecretReferenceId {
        &self.id
    }

    pub fn source(&self) -> SecretSource {
        self.source
    }

    pub fn purpose(&self) -> SecretPurpose {
        self.purpose
    }

    pub fn expose_secret(&self) -> &str {
        self.value.expose_secret()
    }
}

impl fmt::Debug for ResolvedSecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedSecretString")
            .field("id", &self.id)
            .field("source", &self.source)
            .field("purpose", &self.purpose)
            .field("value", &"[REDACTED]")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretResolverError {
    UnknownSecret(SecretReferenceId),
    UnsupportedSource {
        id: SecretReferenceId,
        source: SecretSource,
    },
    PurposeMismatch {
        id: SecretReferenceId,
        actual: SecretPurpose,
        expected: SecretPurpose,
    },
    MissingEnvSecret {
        id: SecretReferenceId,
        locator: String,
    },
    MissingResolverConfig {
        source: SecretSource,
        variable: &'static str,
    },
    EmptyResolverConfig {
        source: SecretSource,
        variable: &'static str,
    },
    InvalidVaultKv2Locator {
        id: SecretReferenceId,
        locator: String,
        reason: &'static str,
    },
    VaultClientConfig {
        source: SecretSource,
        message: String,
    },
    VaultRead {
        id: SecretReferenceId,
        locator: String,
        message: String,
    },
    VaultFieldMissing {
        id: SecretReferenceId,
        field: String,
    },
    VaultFieldNotString {
        id: SecretReferenceId,
        field: String,
    },
    EmptySecret {
        id: SecretReferenceId,
    },
}

impl fmt::Display for SecretResolverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSecret(id) => write!(f, "unknown secret `{id}`"),
            Self::UnsupportedSource { id, source } => {
                write!(f, "secret `{id}` uses unsupported source {source:?}")
            }
            Self::PurposeMismatch {
                id,
                actual,
                expected,
            } => write!(
                f,
                "secret `{id}` has purpose {actual:?}, expected {expected:?}"
            ),
            Self::MissingEnvSecret { id, locator } => {
                write!(f, "missing env secret `{locator}` for secret `{id}`")
            }
            Self::MissingResolverConfig { source, variable } => {
                write!(f, "secret source {source:?} requires `{variable}`")
            }
            Self::EmptyResolverConfig { source, variable } => {
                write!(f, "secret source {source:?} config `{variable}` is empty")
            }
            Self::InvalidVaultKv2Locator {
                id,
                locator,
                reason,
            } => write!(
                f,
                "secret `{id}` has invalid Vault KV2 locator `{locator}`: {reason}"
            ),
            Self::VaultClientConfig { source, message } => {
                write!(f, "failed to configure {source:?} secret client: {message}")
            }
            Self::VaultRead {
                id,
                locator,
                message,
            } => write!(
                f,
                "failed to read Vault secret `{locator}` for secret `{id}`: {message}"
            ),
            Self::VaultFieldMissing { id, field } => {
                write!(f, "Vault secret for `{id}` is missing field `{field}`")
            }
            Self::VaultFieldNotString { id, field } => {
                write!(f, "Vault secret field `{field}` for `{id}` is not a string")
            }
            Self::EmptySecret { id } => write!(f, "secret `{id}` resolved to an empty value"),
        }
    }
}

impl Error for SecretResolverError {}

fn resolve_env_secret(
    secret: &SecretReference,
) -> Result<ResolvedSecretString, SecretResolverError> {
    let value =
        env::var(secret.locator.as_str()).map_err(|_| SecretResolverError::MissingEnvSecret {
            id: secret.id.clone(),
            locator: secret.locator.to_string(),
        })?;
    if value.is_empty() {
        return Err(SecretResolverError::EmptySecret {
            id: secret.id.clone(),
        });
    }
    Ok(ResolvedSecretString {
        id: secret.id.clone(),
        source: secret.source,
        purpose: secret.purpose,
        value: SecretString::from(value),
    })
}

#[derive(Debug, Clone)]
struct VaultClientConfig {
    source: SecretSource,
    address: String,
    token: SecretString,
    namespace: Option<String>,
}

impl VaultClientConfig {
    fn from_env(source: SecretSource) -> Result<Self, SecretResolverError> {
        let address = required_resolver_env(source, "VAULT_ADDR")?;
        let token = SecretString::from(required_resolver_env(source, "VAULT_TOKEN")?);
        let namespace = env::var("VAULT_NAMESPACE")
            .ok()
            .filter(|value| !value.is_empty());
        Ok(Self {
            source,
            address,
            token,
            namespace,
        })
    }

    fn client(&self) -> Result<VaultClient, SecretResolverError> {
        let mut builder = VaultClientSettingsBuilder::default();
        builder
            .address(&self.address)
            .token(self.token.expose_secret().to_owned())
            .timeout(Some(VAULT_SECRET_TIMEOUT));
        if let Some(namespace) = &self.namespace {
            builder.set_namespace(namespace.clone());
        }
        let settings =
            builder
                .build()
                .map_err(|message| SecretResolverError::VaultClientConfig {
                    source: self.source,
                    message: message.to_string(),
                })?;
        VaultClient::new(settings).map_err(|err| SecretResolverError::VaultClientConfig {
            source: self.source,
            message: err.to_string(),
        })
    }
}

fn required_resolver_env(
    source: SecretSource,
    variable: &'static str,
) -> Result<String, SecretResolverError> {
    let value = env::var(variable)
        .map_err(|_| SecretResolverError::MissingResolverConfig { source, variable })?;
    if value.is_empty() {
        return Err(SecretResolverError::EmptyResolverConfig { source, variable });
    }
    Ok(value)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VaultKv2Locator {
    mount: String,
    path: String,
    field: String,
    version: Option<NonZeroU64>,
}

impl VaultKv2Locator {
    fn parse(secret: &SecretReference) -> Result<Self, SecretResolverError> {
        let locator = secret.locator.as_str();
        let url = Url::parse(locator).map_err(|_| SecretResolverError::InvalidVaultKv2Locator {
            id: secret.id.clone(),
            locator: locator.to_owned(),
            reason: "must be a URI such as kv2://secret/path/to/item#value",
        })?;
        if url.scheme() != "kv2" {
            return Err(SecretResolverError::InvalidVaultKv2Locator {
                id: secret.id.clone(),
                locator: locator.to_owned(),
                reason: "scheme must be kv2",
            });
        }
        if url.username() != "" || url.password().is_some() || url.port().is_some() {
            return Err(SecretResolverError::InvalidVaultKv2Locator {
                id: secret.id.clone(),
                locator: locator.to_owned(),
                reason: "must not contain userinfo or port",
            });
        }
        let Some(mount) = url.host_str().filter(|mount| !mount.is_empty()) else {
            return Err(SecretResolverError::InvalidVaultKv2Locator {
                id: secret.id.clone(),
                locator: locator.to_owned(),
                reason: "must include a KV2 mount name",
            });
        };
        let path = url.path().trim_start_matches('/');
        if path.is_empty() || path.ends_with('/') {
            return Err(SecretResolverError::InvalidVaultKv2Locator {
                id: secret.id.clone(),
                locator: locator.to_owned(),
                reason: "must include a non-empty secret path",
            });
        }
        let Some(field) = url.fragment().filter(|field| !field.is_empty()) else {
            return Err(SecretResolverError::InvalidVaultKv2Locator {
                id: secret.id.clone(),
                locator: locator.to_owned(),
                reason: "must include a secret field fragment",
            });
        };

        let mut version = None;
        for (key, value) in url.query_pairs() {
            if key != "version" {
                return Err(SecretResolverError::InvalidVaultKv2Locator {
                    id: secret.id.clone(),
                    locator: locator.to_owned(),
                    reason: "only the version query parameter is supported",
                });
            }
            version = Some(value.parse::<NonZeroU64>().map_err(|_| {
                SecretResolverError::InvalidVaultKv2Locator {
                    id: secret.id.clone(),
                    locator: locator.to_owned(),
                    reason: "version must be a non-zero integer",
                }
            })?);
        }

        Ok(Self {
            mount: mount.to_owned(),
            path: path.to_owned(),
            field: field.to_owned(),
            version,
        })
    }
}

async fn resolve_vault_kv2_secret(
    secret: &SecretReference,
    config: &VaultClientConfig,
) -> Result<ResolvedSecretString, SecretResolverError> {
    let locator = VaultKv2Locator::parse(secret)?;
    let client = config.client()?;
    let data = read_vault_kv2_data(secret, &client, &locator).await?;
    let value = data
        .get(&locator.field)
        .ok_or_else(|| SecretResolverError::VaultFieldMissing {
            id: secret.id.clone(),
            field: locator.field.clone(),
        })?
        .as_str()
        .ok_or_else(|| SecretResolverError::VaultFieldNotString {
            id: secret.id.clone(),
            field: locator.field.clone(),
        })?;
    if value.is_empty() {
        return Err(SecretResolverError::EmptySecret {
            id: secret.id.clone(),
        });
    }

    Ok(ResolvedSecretString {
        id: secret.id.clone(),
        source: secret.source,
        purpose: secret.purpose,
        value: SecretString::from(value.to_owned()),
    })
}

async fn read_vault_kv2_data(
    secret: &SecretReference,
    client: &VaultClient,
    locator: &VaultKv2Locator,
) -> Result<BTreeMap<String, Value>, SecretResolverError> {
    let result = if let Some(version) = locator.version {
        kv2::read_version(client, &locator.mount, &locator.path, version.get()).await
    } else {
        kv2::read(client, &locator.mount, &locator.path).await
    };
    result.map_err(|err| SecretResolverError::VaultRead {
        id: secret.id.clone(),
        locator: secret.locator.to_string(),
        message: err.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use axum::{Json, Router, routing::get};
    use serde_json::Value;
    use veoveo_mcp_contract::{SecretLocator, SecretOwner, ServerSlug};

    use super::*;

    fn secret(source: SecretSource, purpose: SecretPurpose) -> SecretReference {
        SecretReference {
            id: SecretReferenceId::new("test_secret").unwrap(),
            source,
            purpose,
            locator: SecretLocator::new("VEOVEO_TEST_SECRET_RESOLVER_VALUE").unwrap(),
            owner: SecretOwner::Server {
                server: ServerSlug::new("media").unwrap(),
            },
            rotation_hint: None,
            metadata: Value::Null,
        }
    }

    fn vault_secret(locator: &str) -> SecretReference {
        let mut secret = secret(SecretSource::Vault, SecretPurpose::OAuthClientSecret);
        secret.locator = SecretLocator::new(locator).unwrap();
        secret
    }

    #[test]
    fn resolved_secret_debug_does_not_expose_value() {
        let secret = secret(SecretSource::Env, SecretPurpose::OAuthClientSecret);
        let resolved = ResolvedSecretString {
            id: secret.id,
            source: secret.source,
            purpose: secret.purpose,
            value: SecretString::from("resolved-secret-value"),
        };

        assert_eq!(resolved.expose_secret(), "resolved-secret-value");
        assert!(!format!("{resolved:?}").contains("resolved-secret-value"));
    }

    #[tokio::test]
    async fn rejects_secret_purpose_mismatch() {
        let err = GatewaySecretResolver::new()
            .resolve_reference(
                &secret(SecretSource::Env, SecretPurpose::OAuthClientSecret),
                SecretPurpose::JwksPrivateKey,
            )
            .await
            .unwrap_err();

        assert!(matches!(err, SecretResolverError::PurposeMismatch { .. }));
    }

    #[tokio::test]
    async fn rejects_unsupported_secret_sources() {
        let err = GatewaySecretResolver::new()
            .resolve_reference(
                &secret(
                    SecretSource::CloudSecretManager,
                    SecretPurpose::OAuthClientSecret,
                ),
                SecretPurpose::OAuthClientSecret,
            )
            .await
            .unwrap_err();

        assert_eq!(
            err,
            SecretResolverError::UnsupportedSource {
                id: SecretReferenceId::new("test_secret").unwrap(),
                source: SecretSource::CloudSecretManager,
            }
        );
    }

    #[test]
    fn parses_vault_kv2_locator() {
        let locator =
            VaultKv2Locator::parse(&vault_secret("kv2://secret/veoveo/gateway?version=7#value"))
                .unwrap();

        assert_eq!(locator.mount, "secret");
        assert_eq!(locator.path, "veoveo/gateway");
        assert_eq!(locator.field, "value");
        assert_eq!(locator.version.unwrap().get(), 7);
    }

    #[test]
    fn rejects_vault_locator_without_field() {
        let err = VaultKv2Locator::parse(&vault_secret("kv2://secret/veoveo/gateway")).unwrap_err();

        assert!(matches!(
            err,
            SecretResolverError::InvalidVaultKv2Locator { .. }
        ));
    }

    #[tokio::test]
    async fn resolves_vault_kv2_secret_with_explicit_config() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new().route(
            "/v1/secret/data/veoveo/gateway",
            get(|| async {
                Json(serde_json::json!({
                    "request_id": "vault-smoke-request",
                    "lease_id": "",
                    "renewable": false,
                    "lease_duration": 0,
                    "wrap_info": null,
                    "warnings": null,
                    "auth": null,
                    "data": {
                        "data": {
                            "value": "vault-secret-value"
                        },
                        "metadata": {
                            "created_time": "2026-07-02T00:00:00Z",
                            "deletion_time": "",
                            "custom_metadata": null,
                            "destroyed": false,
                            "version": 1
                        }
                    }
                }))
            }),
        );
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let config = VaultClientConfig {
            source: SecretSource::Vault,
            address,
            token: SecretString::from("test-vault-token"),
            namespace: None,
        };
        let resolved =
            resolve_vault_kv2_secret(&vault_secret("kv2://secret/veoveo/gateway#value"), &config)
                .await
                .unwrap();

        assert_eq!(resolved.expose_secret(), "vault-secret-value");
        server.abort();
    }
}
