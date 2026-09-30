//! Installation and caller names shared by protocol adapters and audit.
use crate::{
    IdentifierError,
    identifier_syntax::{validate_claim_text, validate_path_id},
    names::name,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};
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
name!(
    ServerSlug,
    validate_path_id,
    "Canonical hosted MCP server id used in manifests, profiles, and gateway routes."
);
name!(
    GatewayProfileId,
    validate_path_id,
    "Veoveo profile id exposed under `/mcp/{profile}`."
);
name!(
    LocalToolName,
    validate_path_id,
    "Tool name as exposed by one direct MCP server."
);
name!(
    PromptName,
    validate_path_id,
    "Prompt name as exposed by one direct MCP server or gateway profile."
);
name!(
    OAuthClientId,
    validate_claim_text,
    "Registered OAuth client id allowed to request gateway-profile tokens."
);
name!(
    GatewayRefreshFamilyId,
    validate_uuid_v7,
    "Canonical UUIDv7 identity for one rotating OAuth refresh-token family."
);

fn validate_task_route(value: &str) -> Result<(), IdentifierError> {
    crate::identifier_syntax::validate_token_text(value)?;
    if value.len() > 128 {
        return Err(IdentifierError::new(value, "must be at most 128 bytes"));
    }
    Ok(())
}
name!(
    CanonicalTaskId,
    validate_task_route,
    "Opaque gateway task route bound to current invocation authority."
);
