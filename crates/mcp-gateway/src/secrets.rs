use std::{env, error::Error, fmt};

use secrecy::{ExposeSecret, SecretString};
use veoveo_mcp_contract::{SecretPurpose, SecretReference, SecretReferenceId, SecretSource};

use crate::GatewayCatalog;

#[derive(Debug, Clone, Default)]
pub struct GatewaySecretResolver;

impl GatewaySecretResolver {
    pub fn new() -> Self {
        Self
    }

    pub fn resolve_string(
        &self,
        catalog: &GatewayCatalog,
        secret_id: &SecretReferenceId,
        expected_purpose: SecretPurpose,
    ) -> Result<ResolvedSecretString, SecretResolverError> {
        let secret = catalog
            .secret_reference(secret_id)
            .ok_or_else(|| SecretResolverError::UnknownSecret(secret_id.clone()))?;
        self.resolve_reference(secret, expected_purpose)
    }

    pub fn resolve_reference(
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

#[cfg(test)]
mod tests {
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

    #[test]
    fn rejects_secret_purpose_mismatch() {
        let err = GatewaySecretResolver::new()
            .resolve_reference(
                &secret(SecretSource::Env, SecretPurpose::OAuthClientSecret),
                SecretPurpose::JwksPrivateKey,
            )
            .unwrap_err();

        assert!(matches!(err, SecretResolverError::PurposeMismatch { .. }));
    }

    #[test]
    fn rejects_unsupported_secret_sources() {
        let err = GatewaySecretResolver::new()
            .resolve_reference(
                &secret(SecretSource::Vault, SecretPurpose::OAuthClientSecret),
                SecretPurpose::OAuthClientSecret,
            )
            .unwrap_err();

        assert_eq!(
            err,
            SecretResolverError::UnsupportedSource {
                id: SecretReferenceId::new("test_secret").unwrap(),
                source: SecretSource::Vault,
            }
        );
    }
}
