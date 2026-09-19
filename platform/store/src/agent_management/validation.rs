use std::collections::BTreeSet;

use sha2::{Digest, Sha256};

use super::{
    AgentContent, AgentDefinitionMutation, AgentExecution, AgentManagementError,
    AgentTemplateParameter, Result,
};

pub(super) fn text(value: &str, field: &'static str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(|c| c == '\0') {
        return Err(AgentManagementError::Invalid(field));
    }
    Ok(())
}

pub(super) fn key(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    {
        return Err(AgentManagementError::Invalid("key"));
    }
    Ok(())
}

pub(super) fn digest(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(AgentManagementError::Invalid("digest"));
    }
    Ok(())
}

impl AgentContent {
    pub fn validate(&self) -> Result<()> {
        key(&self.model.id)?;
        digest(&self.model.revision)?;
        text(&self.instructions, "instructions", 16_384)?;
        if self.tools.len() > 64
            || self.tools.iter().collect::<BTreeSet<_>>().len() != self.tools.len()
        {
            return Err(AgentManagementError::Invalid("tools"));
        }
        for tool in &self.tools {
            text(tool, "tool", 256)?;
            let Some((server, name)) = tool.split_once("__") else {
                return Err(AgentManagementError::Invalid("tool"));
            };
            key(server)?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
            {
                return Err(AgentManagementError::Invalid("tool"));
            }
        }
        let budget = &self.budgets;
        if !(1..=8192).contains(&budget.max_output_tokens)
            || !(1..=32).contains(&budget.max_completion_calls)
            || budget.max_tool_calls > 64
            || !(1..=900).contains(&budget.deadline_seconds)
        {
            return Err(AgentManagementError::Invalid("budgets"));
        }
        if let AgentExecution::Managed {
            template,
            template_revision,
            parameters,
            resource_subscriptions,
        } = &self.execution
        {
            key(template)?;
            digest(template_revision)?;
            if parameters.len() > 32
                || resource_subscriptions.len() > 128
                || resource_subscriptions.iter().collect::<BTreeSet<_>>().len()
                    != resource_subscriptions.len()
            {
                return Err(AgentManagementError::Invalid("template parameters"));
            }
            for (name, value) in parameters {
                key(name)?;
                if let AgentTemplateParameter::Text(value) = value {
                    text(value, "template parameter", 2048)?;
                }
            }
            for uri in resource_subscriptions {
                text(uri, "subscription", 2048)?;
                if url::Url::parse(uri).is_err() {
                    return Err(AgentManagementError::Invalid("subscription"));
                }
            }
        }
        Ok(())
    }

    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        hash(self)
    }
}

pub(super) fn hash(value: &impl serde::Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(value).map_err(|_| AgentManagementError::Unavailable)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

pub(super) fn mutation(value: &AgentDefinitionMutation) -> Result<()> {
    match value {
        AgentDefinitionMutation::Create {
            name,
            description,
            content,
        } => {
            text(name, "name", 200)?;
            text(description, "description", 2000)?;
            content.validate()
        }
        AgentDefinitionMutation::Draft { content } => content.validate(),
        AgentDefinitionMutation::Publish {
            digest: value,
            audience,
        } => {
            digest(value)?;
            if audience.is_empty()
                || audience.len() > 64
                || audience
                    .iter()
                    .map(|a| &a.work_context)
                    .collect::<BTreeSet<_>>()
                    .len()
                    != audience.len()
            {
                return Err(AgentManagementError::Invalid("audience"));
            }
            for context in audience {
                digest(&context.context_digest)?;
            }
            Ok(())
        }
        AgentDefinitionMutation::Metadata { name, description } => {
            text(name, "name", 200)?;
            text(description, "description", 2000)
        }
        AgentDefinitionMutation::Transfer { owner } if owner.table.as_str() != "principal" => {
            Err(AgentManagementError::Invalid("owner"))
        }
        _ => Ok(()),
    }
}
