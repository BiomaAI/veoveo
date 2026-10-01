//! Installation validation for generic source approval and indexing registrations.
use super::*;

pub(super) fn validate_knowledge_approvals(
    server: &ServerManifest,
    labels: &BTreeSet<DataLabelId>,
) -> Result<(), GatewayControlPlaneError> {
    let invalid = |reason| GatewayControlPlaneError::InvalidKnowledgeApproval {
        server: server.slug.clone(),
        reason,
    };
    if server.knowledge.len() > 1024 {
        return Err(invalid("at most 1024 approvals per source"));
    }
    if !server.knowledge.is_empty()
        && (!server.capabilities.resources || !server.capabilities.resource_templates)
    {
        return Err(invalid(
            "knowledge sources require resources and resource templates",
        ));
    }
    let mut collections = BTreeSet::new();
    for entry in &server.knowledge {
        entry.validate().map_err(|error| invalid(error.0))?;
        if entry.collection.server() != &server.slug {
            return Err(invalid("approval collection belongs to another server"));
        }
        if !collections.insert(&entry.collection) {
            return Err(invalid("duplicate collection approval"));
        }
        if !entry.data_labels.is_subset(labels) {
            return Err(invalid("approval references an unknown data label"));
        }
    }
    Ok(())
}

pub(super) fn validate_indexing_client(
    client: &OAuthClientRegistration,
    profiles: &BTreeMap<GatewayProfileId, &GatewayProfile>,
    servers: &BTreeMap<ServerSlug, &ServerManifest>,
    policies: &BTreeMap<PolicyVersion, &PolicySet>,
) -> Result<(), GatewayControlPlaneError> {
    let Some(indexing) = &client.knowledge_indexing else {
        return Ok(());
    };
    let invalid = |reason| GatewayControlPlaneError::InvalidKnowledgeIndexing {
        client: client.id.clone(),
        reason,
    };
    indexing.validate().map_err(|error| invalid(error.0))?;
    if client.tenant.is_none()
        || client.invocation_mode != veoveo_types::InvocationMode::Automated
        || client.client_surface != OAuthClientSurface::FullMcp
        || client.grant_types != BTreeSet::from([OAuthGrantType::ClientCredentials])
        || client.auth_methods != BTreeSet::from([OAuthClientAuthMethod::PrivateKeyJwt])
        || client.allowed_resources.len() != 1
    {
        return Err(invalid(
            "indexing requires a tenant-bound automated full-MCP private_key_jwt client with one profile and only client_credentials",
        ));
    }
    let profile = profiles
        .values()
        .find(|profile| {
            client
                .allowed_resources
                .contains(&profile.protected_resource)
        })
        .ok_or_else(|| invalid("indexing client requires a registered gateway profile"))?;
    if profile.artifact_upload.is_some()
        || profile.servers.iter().any(|entry| {
            !matches!(entry.tools, Exposure::None)
                || !matches!(entry.prompts, Exposure::None)
                || entry.tasks != TaskExposure::Disabled
                || entry.completions != CompletionExposure::Disabled
                || !indexing
                    .collections
                    .iter()
                    .any(|collection| collection.server() == &entry.server)
        })
    {
        return Err(invalid(
            "indexing requires a dedicated profile exposing only approved-source resources and subscriptions",
        ));
    }
    let policy = policies
        .get(&profile.policy_version)
        .ok_or_else(|| invalid("indexing profile has no policy"))?;
    if policy.rules.iter().any(|rule| {
        rule.effect == PolicyEffect::Allow
            && (rule.profiles.is_empty() || rule.profiles.contains(&profile.id))
            && rule.actions.iter().any(|action| {
                !matches!(
                    action,
                    GatewayAction::ResourcesList
                        | GatewayAction::ResourcesTemplatesList
                        | GatewayAction::ResourcesRead
                        | GatewayAction::ArtifactRead
                        | GatewayAction::SubscriptionsListen
                )
            })
    }) {
        return Err(invalid(
            "indexing profile policy may allow only resource discovery, reads and subscriptions",
        ));
    }
    for collection in &indexing.collections {
        let server = servers
            .get(collection.server())
            .ok_or_else(|| invalid("indexing collection has an unknown server"))?;
        if !server
            .knowledge
            .iter()
            .any(|entry| &entry.collection == collection)
        {
            return Err(invalid(
                "knowledge client collection requires an explicit installation approval",
            ));
        }
        let exposure = profile
            .servers
            .iter()
            .find(|entry| &entry.server == collection.server())
            .ok_or_else(|| invalid("indexing source is absent from its profile"))?;
        if matches!(&exposure.resources, Exposure::None)
            || matches!(&exposure.resources, Exposure::Listed(selectors) if selectors.is_empty())
        {
            return Err(invalid(
                "indexing source requires resource exposure in its profile",
            ));
        }
    }
    Ok(())
}
