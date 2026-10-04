//! Lightweight Agents-owned public claim admission, independent of runtime services.
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use veoveo_types::{AgentManagedInstanceId, ExtensionError};

mod actions;
pub use actions::{AgentAction, register_catalog};

pub const MANAGED_AGENT_CLAIM: &str = "managed_agent";

/// Signed repository-owned OAuth claim. Current registration still overrides it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedAgentToken {
    pub instance: AgentManagedInstanceId,
    pub generation: i64,
    pub epoch: i64,
}

/// The registry admits the closed wire shape; current authority is checked separately.
pub fn admit_managed_agent_token(
    value: serde_json::Value,
) -> Result<ManagedAgentToken, ExtensionError> {
    serde_json::from_value(value).map_err(|_| ExtensionError::new("invalid managed Agent claim"))
}

impl JsonSchema for ManagedAgentToken {
    fn schema_name() -> Cow<'static, str> {
        "ManagedAgentToken".into()
    }
    fn schema_id() -> Cow<'static, str> {
        // Preserve the published schema identity while transferring Rust ownership.
        "veoveo_mcp_contract::agent_management::instances::ManagedAgentToken".into()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "type": "object",
            "description": "Signed repository-owned OAuth claim. Current registration still overrides it.",
            "properties": {
                "instance": generator.subschema_for::<AgentManagedInstanceId>(),
                "generation": generator.subschema_for::<i64>(),
                "epoch": generator.subschema_for::<i64>()
            },
            "required": ["instance", "generation", "epoch"],
            "additionalProperties": false
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn claim_keeps_wire_shape_and_schema_identity() {
        let value = json!({"instance":"worker-one","generation":2,"epoch":3});
        let claim = admit_managed_agent_token(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(claim).unwrap(), value);
        assert_eq!(ManagedAgentToken::schema_name(), "ManagedAgentToken");
        assert_eq!(
            ManagedAgentToken::schema_id(),
            "veoveo_mcp_contract::agent_management::instances::ManagedAgentToken"
        );
        assert!(
            admit_managed_agent_token(
                json!({"instance":"worker-one","generation":2,"epoch":3,"other":true})
            )
            .is_err()
        );
        assert!(
            admit_managed_agent_token(json!({"instance":"worker-one","generation":2})).is_err()
        );
        assert!(
            admit_managed_agent_token(json!({"instance":"bad\nidentity","generation":2,"epoch":3}))
                .is_err()
        );
    }
}

pub mod authoring;

pub mod control;
