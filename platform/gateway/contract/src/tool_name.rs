//! Server namespace projection for gateway tool identities.

use veoveo_types::{IdentifierError, LocalToolName, ServerSlug};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(transparent)]
pub struct GatewayToolNameError(#[from] IdentifierError);

/// Gateway-scoped tool name after server namespace projection.
#[veoveo_types::id(text(ToolNames))]
pub struct GatewayToolName(String);
impl GatewayToolName {
    /// Compose the gateway namespace from typed server and local tool names.
    pub fn from_parts(
        server: &ServerSlug,
        tool: &LocalToolName,
    ) -> Result<Self, GatewayToolNameError> {
        Self::parse(format!("{server}__{tool}"))
    }
}

#[doc(hidden)]
pub struct ToolNames;
impl veoveo_types::IdProfile for ToolNames {
    type Error = GatewayToolNameError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| validate_tool_name(value));
}

fn validate_tool_name(value: &str) -> Result<(), GatewayToolNameError> {
    if value.is_empty() {
        return Err(IdentifierError::new(value, "must not be empty").into());
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(IdentifierError::new(
            value,
            "must contain only lowercase ASCII letters, digits, hyphen, or underscore",
        )
        .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gateway_tool_names_preserve_namespace_parser_and_string_schema() {
        let projected = GatewayToolName::from_parts(
            &ServerSlug::parse("media").unwrap(),
            &LocalToolName::parse("render").unwrap(),
        )
        .unwrap();
        assert_eq!(projected.to_string(), "media__render");
        assert_eq!(serde_json::to_value(&projected).unwrap(), "media__render");
        for valid in ["plain", "under_score", "dash-1", "media__render"] {
            assert_eq!(valid.parse::<GatewayToolName>().unwrap().as_str(), valid);
        }
        for invalid in [
            "",
            "Uppercase",
            "media.render",
            "media/render",
            " spaced",
            "é",
        ] {
            assert!(GatewayToolName::parse(invalid).is_err());
            assert!(invalid.parse::<GatewayToolName>().is_err());
            assert!(serde_json::from_value::<GatewayToolName>(serde_json::json!(invalid)).is_err());
        }
        let schema = serde_json::to_value(schemars::schema_for!(GatewayToolName)).unwrap();
        assert_eq!(schema["title"], "GatewayToolName");
        assert_eq!(schema["type"], "string");
        assert!(schema.get("pattern").is_none());
    }
}
