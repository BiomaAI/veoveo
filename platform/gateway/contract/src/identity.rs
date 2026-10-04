//! Neutral installation identities shared by HTTP and authorization adapters.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
pub use veoveo_types::OAuthClientId;
use veoveo_types::{
    IdentifierError,
    identifier_syntax::{validate_claim_text, validate_path_id, validate_token_text},
};

/// Resource authorization server id that issues profile-scoped MCP access tokens.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_path_id, error = IdentifierError)]
pub struct AuthorizationServerId(String);

/// Reference to a secret managed outside control data.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_token_text, error = IdentifierError)]
pub struct SecretReferenceId(String);

/// OAuth protected-resource URI for an MCP profile or platform data plane.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_claim_text, error = IdentifierError)]
pub struct ProtectedResourceId(String);

/// Installation-local name for an OAuth protected resource.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_path_id, error = IdentifierError)]
pub struct ProtectedResourceName(String);

/// External secret locator. This is a reference path, not a secret value.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    veoveo_types::Id,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, validate = validate_claim_text, error = IdentifierError)]
pub struct SecretLocator(String);
