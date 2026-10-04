//! Installation secret references contain locators, never credential values.
use crate::{SecretLocator, SecretReferenceId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use veoveo_types::{GatewayProfileId, ServerSlug, TenantId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SecretReference {
    pub id: SecretReferenceId,
    pub source: SecretSource,
    pub purpose: SecretPurpose,
    pub locator: SecretLocator,
    pub owner: SecretOwner,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_hint: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SecretSource {
    Env,
    Vault,
    HcpVault,
    CloudSecretManager,
    KmsBackedStore,
    EnterpriseManaged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SecretPurpose {
    ProviderApiKey,
    WebhookSecret,
    #[serde(rename = "oauth_client_secret")]
    OAuthClientSecret,
    GatewaySigningKey,
    JwksPrivateKey,
    TokenExchangeCredential,
    ObjectStoreCredential,
    TlsClientCertificate,
    TlsClientPrivateKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum SecretOwner {
    Gateway,
    Profile { profile: GatewayProfileId },
    Server { server: ServerSlug },
    Tenant { tenant: TenantId },
}

impl<'de> Deserialize<'de> for SecretOwner {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
        enum Wire {
            Gateway {},
            Profile { profile: GatewayProfileId },
            Server { server: ServerSlug },
            Tenant { tenant: TenantId },
        }
        Ok(match Wire::deserialize(deserializer)? {
            Wire::Gateway {} => Self::Gateway,
            Wire::Profile { profile } => Self::Profile { profile },
            Wire::Server { server } => Self::Server { server },
            Wire::Tenant { tenant } => Self::Tenant { tenant },
        })
    }
}
