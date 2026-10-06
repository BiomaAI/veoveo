//! Installation and caller names shared by protocol adapters and audit.
use crate::IdentifierError;

#[doc = "Canonical hosted MCP server id used in manifests, profiles, and gateway routes."]
#[veoveo_types::id(text(ServerSlugs))]
pub struct ServerSlug(String);
#[doc = "Veoveo profile id exposed under `/mcp/{profile}`."]
#[veoveo_types::id(text(crate::identifier_syntax::PathIdProfile))]
pub struct GatewayProfileId(String);
#[doc = "Tool name as exposed by one direct MCP server."]
#[veoveo_types::id(text(crate::identifier_syntax::PathIdProfile))]
pub struct LocalToolName(String);
#[doc = "Prompt name as exposed by one direct MCP server or gateway profile."]
#[veoveo_types::id(text(crate::identifier_syntax::PathIdProfile))]
pub struct PromptName(String);
#[doc = "Registered OAuth client id allowed to request gateway-profile tokens."]
#[veoveo_types::id(text(ClientIds))]
pub struct OAuthClientId(String);
#[doc = "Canonical UUIDv7 identity for one rotating OAuth refresh-token family."]
#[veoveo_types::id(uuid(RefreshIds))]
pub struct GatewayRefreshFamilyId(String);

fn validate_task_route(value: &str, _: crate::IdMetadata) -> Result<(), IdentifierError> {
    crate::identifier_syntax::validate_token_text(value)?;
    if value.len() > 128 {
        return Err(IdentifierError::new(value, "must be at most 128 bytes"));
    }
    Ok(())
}
#[doc = "Opaque gateway task route bound to current invocation authority."]
#[veoveo_types::id(text(TaskRouteIds))]
pub struct CanonicalTaskId(String);

#[doc(hidden)]
pub struct ClientIds;
impl crate::IdProfile for ClientIds {
    type Error = IdentifierError;
    const PROFILE: crate::IdProfileSpec<Self::Error> = crate::IdProfileSpec {
        schema_id: Some(|_| concat!(module_path!(), "::OAuthClientId").into()),
        schema: crate::IdSchema::Owner {
            schema: client_schema,

            inline: false,
        },
        ..<crate::identifier_syntax::ClaimTextProfile as crate::IdProfile>::PROFILE
    };
}
fn client_schema(_: &mut schemars::SchemaGenerator, _: crate::IdMetadata) -> schemars::Schema {
    schemars::json_schema!({
        "type": "string",
        "description": "Registered OAuth client id allowed to request gateway-profile tokens.",
        "minLength": 1,
        "not": { "pattern": r"[\u0000-\u001f\u007f-\u009f]" }
    })
}
#[doc(hidden)]
pub struct RefreshIds;
impl crate::IdProfile for RefreshIds {
    type Error = IdentifierError;
    const PROFILE: crate::IdProfileSpec<Self::Error> = crate::IdProfileSpec::uuid(
        crate::UuidGrammar {
            versions: &[7],
            variant: crate::UuidVariant::Rfc4122,
            spelling: crate::UuidSpelling::CanonicalLowerHyphenated,
        },
        refresh_id_error,
    );
}
fn refresh_id_error(
    value: &str,
    _: crate::IdMetadata,
    failure: crate::IdFailure,
) -> IdentifierError {
    IdentifierError::new(
        value,
        if matches!(failure, crate::IdFailure::ParseUuid) {
            "must be a UUIDv7"
        } else {
            "must be a canonical RFC UUIDv7"
        },
    )
}
#[doc(hidden)]
pub struct TaskRouteIds;
impl crate::IdProfile for TaskRouteIds {
    type Error = IdentifierError;
    const PROFILE: crate::IdProfileSpec<Self::Error> =
        crate::IdProfileSpec::text(validate_task_route);
}

#[doc(hidden)]
pub struct ServerSlugs;
impl crate::IdProfile for ServerSlugs {
    type Error = IdentifierError;
    const PROFILE: crate::IdProfileSpec<Self::Error> =
        <crate::identifier_syntax::PathIdProfile as crate::IdProfile>::PROFILE;
    fn naming_profile(_: crate::IdMetadata) -> Option<crate::ScalarNaming> {
        Some(crate::ScalarNaming::builtin(
            crate::ScalarGrammar::ServerSlug,
        ))
    }
}
