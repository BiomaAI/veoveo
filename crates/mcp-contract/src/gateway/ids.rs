use std::{fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::wire::{
    validate_claim_text, validate_gateway_name, validate_oauth_authorization_code,
    validate_oauth_state_value, validate_path_id, validate_pkce_code_token, validate_token_text,
    validate_uri_scheme,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentifierError {
    value: String,
    rule: &'static str,
}

impl IdentifierError {
    pub(super) fn new(value: &str, rule: &'static str) -> Self {
        Self {
            value: value.to_string(),
            rule,
        }
    }
}

impl fmt::Display for IdentifierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid identifier {:?}: {}", self.value, self.rule)
    }
}

impl std::error::Error for IdentifierError {}

typed_id!(
    ServerSlug,
    validate_path_id,
    "Canonical hosted MCP server id used in manifests, profiles, and gateway routes."
);
typed_id!(
    GatewayProfileId,
    validate_path_id,
    "Veoveo profile id exposed under `/mcp/{profile}`."
);
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
    LocalToolName,
    validate_gateway_name,
    "Tool name as exposed by one direct MCP server."
);
typed_id!(
    PromptName,
    validate_gateway_name,
    "Prompt name as exposed by one direct MCP server or gateway profile."
);
typed_id!(
    ResourceScheme,
    validate_uri_scheme,
    "Server-owned resource URI scheme, for example `media`."
);
typed_id!(
    ScopeName,
    validate_token_text,
    "OAuth/OIDC scope value. It must not contain whitespace or control characters."
);
typed_id!(
    DataLabelId,
    validate_token_text,
    "Policy data label such as `cui`, `itar`, `pii`, or an IdP-provided clearance label."
);
typed_id!(
    PrincipalId,
    validate_claim_text,
    "Stable authenticated user or service-principal identity."
);
typed_id!(
    TenantId,
    validate_claim_text,
    "Tenant, organization, or customer boundary identifier."
);
typed_id!(
    GroupId,
    validate_claim_text,
    "Identity-provider group identifier used by gateway policy."
);
typed_id!(
    RoleId,
    validate_claim_text,
    "Identity-provider role identifier used by gateway policy."
);
typed_id!(
    PolicyVersion,
    validate_token_text,
    "Immutable policy version identifier emitted with decisions and audit records."
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
    "OAuth protected-resource identifier, usually the gateway profile URL."
);
typed_id!(
    OAuthClientId,
    validate_claim_text,
    "Registered OAuth client id allowed to request gateway-profile tokens."
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
    TokenIssuer,
    validate_claim_text,
    "Expected token issuer identifier."
);
typed_id!(
    TokenSubject,
    validate_claim_text,
    "Subject claim from an authenticated access token or identity assertion."
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
    GatewayTaskId,
    validate_token_text,
    "Gateway task id visible to MCP clients."
);
typed_id!(
    GatewayControlPlaneRevisionId,
    validate_token_text,
    "Durable gateway control-plane revision id."
);
typed_id!(
    UpstreamTaskId,
    validate_token_text,
    "Task id owned by one hosted upstream MCP server."
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
