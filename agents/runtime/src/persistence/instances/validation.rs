use super::*;

pub(super) fn mutation(mutation: &ManagedAgentMutation, limits: ManagedAgentLimits) -> Result<()> {
    if !(1..=1_000).contains(&limits.instances) || !(1..=1_000_000).contains(&limits.storage_gib) {
        return Err(AgentManagementError::Invalid("instance limits"));
    }
    match mutation {
        ManagedAgentMutation::Provision { plan } => {
            text(&plan.name, 200)?;
            super::super::validation::key(&plan.definition_key)?;
            super::super::validation::digest(&plan.revision)?;
            let identity = &plan.identity;
            for value in [
                &identity.client_id,
                &identity.authorization_server,
                &identity.profile,
            ] {
                super::super::validation::key(value)?;
            }
            for value in [&identity.issuer, &identity.resource] {
                text(value, 2048)?;
            }
            if identity.scopes.is_empty() || identity.scopes.len() > 64 || identity.roles.len() > 16
            {
                return Err(AgentManagementError::Invalid("automated authority"));
            }
            for value in identity.scopes.iter().chain(&identity.roles) {
                text(value, 200)?;
            }
            let resources = &plan.resources;
            for value in [
                &resources.namespace,
                &resources.workload,
                &resources.credential_secret,
                &resources.volume_claim,
                &resources.template_config_map,
            ] {
                if value.is_empty()
                    || value.len() > 63
                    || value.starts_with('-')
                    || value.ends_with('-')
                    || !value
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                {
                    return Err(AgentManagementError::Invalid("resource identity"));
                }
            }
            image(&resources.image)?;
            if !(1..=1024).contains(&resources.storage_gib) {
                return Err(AgentManagementError::Invalid("storage or image"));
            }
        }
        ManagedAgentMutation::Revision {
            digest,
            image: admitted,
        } => {
            super::super::validation::digest(digest)?;
            image(admitted)?;
        }
        _ => {}
    }
    Ok(())
}

fn image(value: &str) -> Result<()> {
    let (repository, digest) = value
        .rsplit_once("@sha256:")
        .ok_or(AgentManagementError::Invalid("image digest"))?;
    super::super::validation::digest(digest)?;
    if repository.is_empty() || value.len() > 512 || value.contains(char::is_whitespace) {
        return Err(AgentManagementError::Invalid("image"));
    }
    Ok(())
}

pub(super) fn text(value: &str, maximum: usize) -> Result<()> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(AgentManagementError::Invalid("text"));
    }
    Ok(())
}
