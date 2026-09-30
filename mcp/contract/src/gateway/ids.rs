use std::{fmt, str::FromStr};
pub use veoveo_types::{
    CanonicalTaskId, GatewayProfileId, GatewayRefreshFamilyId, LocalToolName, OAuthClientId,
    PromptName, ServerSlug, TokenIssuer, TokenSubject,
};
use veoveo_types::{
    IdentifierError,
    identifier_syntax::{validate_claim_text, validate_path_id, validate_token_text},
};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::wire::{
    validate_compatibility_helper_id, validate_gateway_name, validate_oauth_authorization_code,
    validate_oauth_state_value, validate_pkce_code_token, validate_principal_display_name,
};

macro_rules! typed_id {
    ($name:ident, $validator:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                let value = value.into();
                $validator(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl FromStr for $name {
            type Err = IdentifierError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value.to_string())
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

macro_rules! secret_typed_id {
    ($name:ident, $validator:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                let value = value.into();
                $validator(&value)?;
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!(stringify!($name), "([REDACTED])"))
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("[REDACTED]")
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl FromStr for $name {
            type Err = IdentifierError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value.to_owned())
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

fn validate_uuid_v7(value: &str) -> Result<(), IdentifierError> {
    let uuid = uuid::Uuid::parse_str(value)
        .map_err(|_| IdentifierError::new(value, "must be a UUIDv7"))?;
    if uuid.get_version_num() != 7 {
        return Err(IdentifierError::new(value, "must be a UUIDv7"));
    }
    Ok(())
}

typed_id!(
    IdentityProviderId,
    validate_path_id,
    "Configured identity provider id used by gateway profiles."
);
typed_id!(
    AuthorizationServerId,
    validate_path_id,
    "Resource authorization server id that issues profile-scoped MCP access tokens."
);
typed_id!(
    GatewayToolName,
    validate_gateway_name,
    "Gateway-scoped tool name after server namespace projection."
);
typed_id!(
    ArtifactAudience,
    validate_gateway_name,
    "Artifact-service audience admitted for one hosted server."
);
typed_id!(
    PlatformCapabilityId,
    validate_path_id,
    "Installation platform capability required by one hosted server."
);
typed_id!(
    PrincipalDisplayName,
    validate_principal_display_name,
    "Human-readable label for the authenticated principal. It is display metadata, never an authorization identity."
);
typed_id!(
    PolicyRuleId,
    validate_token_text,
    "Policy rule identifier used for decision evidence."
);
typed_id!(
    SecretReferenceId,
    validate_token_text,
    "Reference to a secret managed outside control data."
);
typed_id!(
    ProtectedResourceId,
    validate_claim_text,
    "OAuth protected-resource URI for an MCP profile or platform data plane."
);
typed_id!(
    ProtectedResourceName,
    validate_path_id,
    "Installation-local name for an OAuth protected resource."
);
typed_id!(
    RecordingProducerId,
    validate_path_id,
    "Configured identity of one governed recording producer."
);
typed_id!(
    RecordingDatasetName,
    validate_gateway_name,
    "Installation-owned dataset name assigned to a recording producer."
);
typed_id!(
    RecordingApplicationId,
    validate_claim_text,
    "Rerun application id admitted for a recording producer."
);
typed_id!(
    RecordingIngestStreamId,
    validate_uuid_v7,
    "Canonical UUIDv7 identity of one authenticated recording ingest stream."
);
typed_id!(
    CompatibilityHelperId,
    validate_compatibility_helper_id,
    "Explicit compatibility helper id exposed to limited MCP clients, for example `media.models`."
);
typed_id!(
    OidcClientRegistrationId,
    validate_path_id,
    "Gateway registration id for its OIDC client relationship with an enterprise identity provider."
);
typed_id!(
    OidcClientId,
    validate_claim_text,
    "OIDC client id assigned to the gateway by an enterprise identity provider."
);
typed_id!(
    OidcNonce,
    validate_oauth_state_value,
    "OIDC nonce bound to an enterprise identity-provider authorization request."
);
typed_id!(
    JwtId,
    validate_claim_text,
    "JWT id used for replay protection or revocation tracking."
);
typed_id!(
    OAuthStateValue,
    validate_oauth_state_value,
    "Opaque OAuth state value stored for browser authorization continuity."
);
typed_id!(
    OAuthAuthorizationCode,
    validate_oauth_authorization_code,
    "Gateway-issued OAuth authorization code exchanged once for a profile access token."
);
secret_typed_id!(
    OAuthRefreshToken,
    validate_oauth_authorization_code,
    "Opaque, rotating OAuth refresh token. Only its SHA-256 digest is persisted."
);
typed_id!(
    PkceCodeChallenge,
    validate_pkce_code_token,
    "PKCE code challenge bound to a gateway-issued authorization code."
);
typed_id!(
    PkceCodeVerifier,
    validate_pkce_code_token,
    "PKCE code verifier presented to the gateway token endpoint."
);
typed_id!(
    TraceId,
    validate_token_text,
    "Request trace/correlation id used in audit and runtime state."
);

typed_id!(
    OpaqueTaskId,
    validate_token_text,
    "Provider-server task identifier used by the final MCP task identity."
);
typed_id!(
    GatewayControlPlaneRevisionId,
    validate_token_text,
    "Durable gateway control-plane revision id."
);
typed_id!(
    McpMethodName,
    validate_token_text,
    "MCP JSON-RPC method name used in policy and audit events."
);
typed_id!(
    SecretLocator,
    validate_claim_text,
    "External secret locator. This is a reference path, not a secret value."
);
