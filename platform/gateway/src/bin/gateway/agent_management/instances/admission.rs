use axum::http::StatusCode;
use veoveo_mcp_contract::{WorkContextMembershipLevel, agent_management as wire};
use veoveo_mcp_gateway::managed_agents::runtime_template_revision;
use veoveo_platform_store::agent_management::{self as domain, instances::*};

use super::super::{AgentManagementState, Fault, authority::Admission, projection};

pub(crate) fn limits() -> anyhow::Result<ManagedAgentLimits> {
    let read = |name, default| -> anyhow::Result<u32> {
        match std::env::var(name) {
            Ok(value) => Ok(value.parse()?),
            Err(std::env::VarError::NotPresent) => Ok(default),
            Err(error) => Err(error.into()),
        }
    };
    let limits = ManagedAgentLimits {
        instances: read("VEOVEO_AGENT_INSTANCE_LIMIT", 32)?,
        storage_gib: read("VEOVEO_AGENT_STORAGE_LIMIT_GIB", 128)?,
    };
    anyhow::ensure!(
        (1..=1000).contains(&limits.instances) && (1..=1_000_000).contains(&limits.storage_gib),
        "invalid managed agent capacity"
    );
    Ok(limits)
}

/// A deployer selects a revision; installation configuration supplies every
/// credential, authority, image, storage and environment destination.
pub(super) async fn template(
    state: &AgentManagementState,
    actor: &Admission,
    definition: &str,
    revision: &str,
) -> Result<wire::RuntimeTemplate, Fault> {
    let revision = state
        .store()
        .agent_revision(&actor.authority, definition, revision)
        .await?;
    let content = projection::public_content(revision.content)?;
    let wire::Execution::Managed {
        template,
        template_revision,
        parameters,
        resource_subscriptions,
    } = &content.execution
    else {
        return Err(Fault::status(StatusCode::CONFLICT));
    };
    let template = state
        .gateway
        .managed_templates()
        .get(template)
        .filter(|t| {
            t.permits(
                &actor.subject.principal,
                &actor.subject.authority.work_context,
            ) && runtime_template_revision(t) == *template_revision
                && t.accepts_parameters(parameters)
                && t.models.contains(&content.model.id)
                && content.tools.iter().all(|tool| t.tools.contains(tool))
                && resource_subscriptions
                    .iter()
                    .all(|uri| t.resource_subscriptions.contains(uri))
        })
        .ok_or_else(|| Fault::status(StatusCode::FORBIDDEN))?;
    let model = state
        .models
        .iter()
        .find(|m| {
            m.id == content.model.id
                && m.revision() == content.model.revision
                && m.permits(
                    &actor.subject.principal,
                    &actor.subject.authority.work_context,
                )
                && m.required_scopes.is_subset(&template.scopes)
                && m.admits(&content.budgets)
        })
        .ok_or_else(|| Fault::status(StatusCode::FORBIDDEN))?;
    if !template
        .workload
        .model_secrets
        .iter()
        .any(|s| s.reference == model.api_key)
    {
        return Err(Fault::status(StatusCode::FORBIDDEN));
    }
    Ok(template.clone())
}

pub(super) async fn provision(
    state: &AgentManagementState,
    actor: &Admission,
    request: &wire::ProvisionInstance,
) -> Result<ManagedAgentOperation, Fault> {
    // Retain the original admitted resource identities for idempotent replay.
    // A new request cannot take over the existing instance: the transaction rejects it.
    let existing = match state
        .store()
        .managed_agent(&actor.authority, request.id.as_str())
        .await
    {
        Ok(instance) => Some(instance),
        Err(domain::AgentManagementError::NotFound) => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(instance) = existing {
        return Ok(state
            .store()
            .mutate_managed_agent(
                &actor.authority,
                request.id.as_str(),
                request.request_id,
                None,
                ManagedAgentMutation::Provision {
                    plan: Box::new(ManagedAgentProvision {
                        name: request.name.clone(),
                        definition_key: request.definition.to_string(),
                        revision: request.revision.hex().to_owned(),
                        identity: instance.identity,
                        resources: instance.resources,
                    }),
                },
                state.instance_limits,
            )
            .await?);
    }
    let template = template(
        state,
        actor,
        request.definition.as_str(),
        request.revision.hex(),
    )
    .await?;
    let profile = actor
        .catalog
        .profile(&template.profile)
        .ok_or_else(Fault::unavailable)?;
    let server = actor
        .catalog
        .authorization_server(&profile.authorization_server)
        .ok_or_else(Fault::unavailable)?;
    let record = managed_agent_record(&actor.authority.tenant, request.id.as_str())?;
    let name = format!("agent-{}", projection::uuid(&record)?.simple());
    let plan = ManagedAgentProvision {
        name: request.name.clone(),
        definition_key: request.definition.to_string(),
        revision: request.revision.hex().to_owned(),
        identity: ManagedAgentIdentity {
            client_id: name.clone(),
            issuer: server.issuer.to_string(),
            authorization_server: profile.authorization_server.to_string(),
            profile: profile.id.to_string(),
            resource: profile.protected_resource.to_string(),
            scopes: template.scopes.iter().map(ToString::to_string).collect(),
            roles: template.roles.iter().map(ToString::to_string).collect(),
            membership: match template.membership {
                WorkContextMembershipLevel::Viewer => {
                    veoveo_platform_store::WorkContextMembershipLevel::Viewer
                }
                WorkContextMembershipLevel::Contributor => {
                    veoveo_platform_store::WorkContextMembershipLevel::Contributor
                }
                WorkContextMembershipLevel::Custodian => {
                    veoveo_platform_store::WorkContextMembershipLevel::Custodian
                }
                WorkContextMembershipLevel::Owner => {
                    veoveo_platform_store::WorkContextMembershipLevel::Owner
                }
            },
        },
        resources: ManagedAgentResources {
            namespace: template.workload.namespace,
            workload: name.clone(),
            credential_secret: format!("{name}-key"),
            volume_claim: format!("{name}-memory"),
            template_config_map: template.workload.config_map,
            image: template.workload.image,
            storage_gib: template.workload.storage_gib,
        },
    };
    super::super::authority::live_session(state, &actor.profile, &actor.subject).await?;
    Ok(state
        .store()
        .mutate_managed_agent(
            &actor.authority,
            request.id.as_str(),
            request.request_id,
            None,
            ManagedAgentMutation::Provision {
                plan: Box::new(plan),
            },
            state.instance_limits,
        )
        .await?)
}

/// Qualify an image-only installation update without retaining another template
/// catalog or weakening the immutable executable revision. The prior digest binds
/// every configuration, storage and authority field that must remain unchanged.
pub(super) fn revision_image(
    previous: &domain::AgentRevision,
    current_image: &str,
    approved: &wire::RuntimeTemplate,
) -> Result<String, Fault> {
    let domain::AgentExecution::Managed {
        template_revision, ..
    } = &previous.content.execution
    else {
        return Err(Fault::status(StatusCode::CONFLICT));
    };
    let mut retained = approved.clone();
    retained.workload.image = current_image.to_owned();
    if runtime_template_revision(&retained).hex() != template_revision {
        return Err(Fault::status(StatusCode::CONFLICT));
    }
    Ok(approved.workload.image.clone())
}
