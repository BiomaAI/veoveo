//! Installation and caller names shared by protocol adapters and audit.
use crate::{
    IdentifierError,
    identifier_syntax::{validate_claim_text, validate_path_id},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
fn validate_uuid_v7(value: &str) -> Result<(), IdentifierError> {
    let id = uuid::Uuid::parse_str(value)
        .map_err(|_| IdentifierError::new(value, "must be a UUIDv7"))?;
    if id.get_version_num() != 7
        || id.get_variant() != uuid::Variant::RFC4122
        || id.to_string() != value
    {
        return Err(IdentifierError::new(
            value,
            "must be a canonical RFC UUIDv7",
        ));
    }
    Ok(())
}
#[doc = "Canonical hosted MCP server id used in manifests, profiles, and gateway routes."]
#[derive(
    veoveo_types::Id,
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
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_path_id)]
pub struct ServerSlug(String);
#[doc = "Veoveo profile id exposed under `/mcp/{profile}`."]
#[derive(
    veoveo_types::Id,
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
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_path_id)]
pub struct GatewayProfileId(String);
#[doc = "Tool name as exposed by one direct MCP server."]
#[derive(
    veoveo_types::Id,
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
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_path_id)]
pub struct LocalToolName(String);
#[doc = "Prompt name as exposed by one direct MCP server or gateway profile."]
#[derive(
    veoveo_types::Id,
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
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_path_id)]
pub struct PromptName(String);
#[doc = "Registered OAuth client id allowed to request gateway-profile tokens."]
#[derive(
    veoveo_types::Id, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_claim_text)]
pub struct OAuthClientId(String);
impl JsonSchema for OAuthClientId {
    fn schema_id() -> std::borrow::Cow<'static, str> {
        concat!(module_path!(), "::OAuthClientId").into()
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "OAuthClientId".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "Registered OAuth client id allowed to request gateway-profile tokens.",
            "minLength": 1,
            "not": { "pattern": r"[\u0000-\u001f\u007f-\u009f]" }
        })
    }
}

#[doc = "Canonical UUIDv7 identity for one rotating OAuth refresh-token family."]
#[derive(
    veoveo_types::Id,
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
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_uuid_v7)]
pub struct GatewayRefreshFamilyId(String);

fn validate_task_route(value: &str) -> Result<(), IdentifierError> {
    crate::identifier_syntax::validate_token_text(value)?;
    if value.len() > 128 {
        return Err(IdentifierError::new(value, "must be at most 128 bytes"));
    }
    Ok(())
}
#[doc = "Opaque gateway task route bound to current invocation authority."]
#[derive(
    veoveo_types::Id,
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
)]
#[serde(try_from = "String", into = "String")]
#[id(string, error = IdentifierError, validate = validate_task_route)]
pub struct CanonicalTaskId(String);
