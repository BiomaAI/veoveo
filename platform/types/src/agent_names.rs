use std::fmt;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("agent identifier must contain 1–128 lowercase letters, digits, hyphens or underscores")]
pub struct AgentIdentifierError;

#[derive(
    veoveo_types::Id,
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, no_display, error = AgentIdentifierError, validate = validate_agent_id)]
pub struct AgentDefinitionId(String);
#[derive(
    veoveo_types::Id,
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, no_display, error = AgentIdentifierError, validate = validate_agent_id)]
pub struct AgentModelId(String);
#[derive(
    veoveo_types::Id,
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, no_display, error = AgentIdentifierError, validate = validate_agent_id)]
pub struct AgentTemplateId(String);
#[derive(
    veoveo_types::Id,
    Clone,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
#[id(string, no_display, error = AgentIdentifierError, validate = validate_agent_id)]
pub struct AgentManagedInstanceId(String);

fn validate_agent_id(value: &str) -> Result<(), AgentIdentifierError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(AgentIdentifierError);
    }
    Ok(())
}

impl fmt::Display for AgentDefinitionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for AgentModelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for AgentTemplateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for AgentManagedInstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
