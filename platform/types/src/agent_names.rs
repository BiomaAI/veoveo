#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("agent identifier must contain 1–128 lowercase letters, digits, hyphens or underscores")]
pub struct AgentIdentifierError;

#[veoveo_types::id(text(AgentNames), inner_display)]
pub struct AgentDefinitionId(String);
#[veoveo_types::id(text(AgentNames), inner_display)]
pub struct AgentModelId(String);
#[veoveo_types::id(text(AgentNames), inner_display)]
pub struct AgentTemplateId(String);
#[veoveo_types::id(text(AgentNames), inner_display)]
pub struct AgentManagedInstanceId(String);

#[doc(hidden)]
pub struct AgentNames;
impl crate::IdProfile for AgentNames {
    type Error = AgentIdentifierError;
    const PROFILE: crate::IdProfileSpec<Self::Error> =
        crate::IdProfileSpec::text(|value, _| validate_agent_id(value));
}

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
