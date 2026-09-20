use axum::{
    Json,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
};
use veoveo_mcp_contract::{GatewayAction as Action, WorkContextId, agent_management as wire};
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::agent_management::{AgentDefinition, AgentPublicationContext};

use super::{
    AgentManagementState, Api, Fault,
    authority::{self, Admission},
    projection,
};
use crate::workspace::operations::Caller;

pub(super) async fn audience(
    state: &AgentManagementState,
    actor: &Admission,
    targets: &[WorkContextId],
) -> Result<Vec<AgentPublicationContext>, Fault> {
    if targets.is_empty()
        || targets.len() > 64
        || targets
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != targets.len()
    {
        return Err(Fault::status(StatusCode::BAD_REQUEST));
    }
    let mut contexts = Vec::new();
    for target in targets {
        let admitted = authority::context(state, &actor.subject, target).await?;
        if target != &actor.subject.authority.work_context && !admitted.manage_context {
            return Err(Fault::status(StatusCode::FORBIDDEN));
        }
        contexts.push(AgentPublicationContext {
            work_context: admitted.work_context,
            context_digest: admitted.context_digest,
        });
    }
    Ok(contexts)
}

pub(super) async fn check(
    state: &AgentManagementState,
    actor: &Admission,
    definition: &AgentDefinition,
    audience: &[WorkContextId],
    headers: &HeaderMap,
) -> Result<(wire::Validation, Vec<AgentPublicationContext>), Fault> {
    let content = projection::public_content(definition.draft.clone())?;
    let mut findings = Vec::new();
    let mut contexts = Vec::new();
    let mut finding = |code, field: &str, message: &str| {
        findings.push(wire::Finding {
            code,
            field: field.to_owned(),
            message: message.to_owned(),
        })
    };
    match self::audience(state, actor, audience).await {
        Ok(approved) => contexts = approved,
        Err(_) => finding(
            wire::FindingCode::AudienceForbidden,
            "audience",
            "Choose one to 64 distinct Work Contexts. Publication into another context requires current governance authority there.",
        ),
    }
    match state.models.iter().find(|m| {
        m.id == content.model.id && m.permits(&actor.subject, &actor.subject.authority.work_context)
    }) {
        None => finding(
            wire::FindingCode::ModelUnavailable,
            "model",
            "This model connection is not available in the current Work Context.",
        ),
        Some(model) => {
            if model.revision() != content.model.revision {
                finding(
                    wire::FindingCode::ModelChanged,
                    "model",
                    "The approved model configuration changed. Select its current revision.",
                );
            }
            if !model.admits(&content.budgets) {
                finding(
                    wire::FindingCode::CapacityExceeded,
                    "budgets",
                    "The requested budgets exceed this model's approved limits.",
                );
            }
            if audience.iter().any(|c| !model.permits(&actor.subject, c)) {
                finding(
                    wire::FindingCode::ModelUnavailable,
                    "audience",
                    "The model must be admitted in every selected Work Context.",
                );
            }
        }
    }
    if let wire::Execution::Managed {
        template,
        template_revision,
        parameters,
        resource_subscriptions,
    } = &content.execution
    {
        use veoveo_mcp_gateway::managed_agents::{
            ManagedTemplateCatalog, runtime_template_revision,
        };
        match state.gateway.managed_templates().get(template).filter(|t| {
            ManagedTemplateCatalog::permits(
                t,
                &actor.subject.principal,
                &actor.subject.authority.work_context,
            ) && runtime_template_revision(t) == *template_revision
        }) {
            None => finding(
                wire::FindingCode::TemplateUnavailable,
                "execution",
                "Select a currently approved runtime template in this Work Context.",
            ),
            Some(template) => {
                if !ManagedTemplateCatalog::parameters(template, parameters) {
                    finding(
                        wire::FindingCode::InvalidContent,
                        "execution.parameters",
                        "Complete the template's parameters using the permitted values.",
                    );
                }
                if !template.models.contains(&content.model.id)
                    || audience.iter().any(|c| !template.work_contexts.contains(c))
                {
                    finding(
                        wire::FindingCode::TemplateUnavailable,
                        "execution",
                        "The template must admit the selected model and every publication context.",
                    );
                }
                if content
                    .tools
                    .iter()
                    .any(|tool| !template.tools.contains(tool))
                    || resource_subscriptions
                        .iter()
                        .any(|uri| !template.resource_subscriptions.contains(uri))
                {
                    finding(
                        wire::FindingCode::CapabilityUnavailable,
                        "execution",
                        "The selected capabilities exceed the runtime template's approved authority.",
                    );
                }
                if let Some(model) = state.models.iter().find(|m| m.id == content.model.id)
                    && !template
                        .workload
                        .model_secrets
                        .iter()
                        .any(|binding| binding.reference == model.api_key)
                {
                    finding(
                        wire::FindingCode::ModelUnavailable,
                        "model",
                        "The runtime template has no approved credential binding for this model.",
                    );
                }
            }
        }
    }
    if !content.tools.is_empty() {
        let caller = Caller::new(actor.profile.id.clone(), actor.subject.clone(), headers)
            .map_err(Fault::status)?;
        match state
            .operations
            .authoring_capabilities(&caller, &content.tools)
            .await
        {
            Ok(tools) => {
                let available = tools
                    .iter()
                    .map(|t| t.name.as_ref())
                    .collect::<std::collections::BTreeSet<_>>();
                for required in &content.tools {
                    if !available.contains(required.as_str()) {
                        findings.push(wire::Finding {
                            code: wire::FindingCode::CapabilityUnavailable,
                            field: "tools".into(),
                            message: format!(
                                "Capability {required} is not currently available to you."
                            ),
                        });
                    }
                }
            }
            Err(_) => findings.push(wire::Finding {
                code: wire::FindingCode::CapabilityUnavailable,
                field: "tools".into(),
                message: "Capability discovery is unavailable. Publication has not changed.".into(),
            }),
        }
    }
    Ok((
        wire::Validation {
            revision: definition.revision,
            digest: projection::digest(&definition.draft_digest)?,
            findings,
        },
        contexts,
    ))
}

pub(super) async fn validate(
    State(state): State<AgentManagementState>,
    Path((profile, id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(request): Json<wire::ValidateDefinition>,
) -> Api<wire::Validation> {
    let actor = authority::admit(&state, profile, subject, Action::AgentDefinitionsPublish).await?;
    let definition = state
        .store()
        .agent_definition(&actor.authority, &id)
        .await?;
    if definition.revision != request.expected_revision {
        return Err(Fault::status(StatusCode::CONFLICT));
    }
    let (result, _) = check(&state, &actor, &definition, &request.audience, &headers).await?;
    Ok(Json(result))
}

pub(super) async fn capabilities(
    State(state): State<AgentManagementState>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
) -> Api<Vec<wire::CapabilityChoice>> {
    let actor = authority::admit(&state, profile, subject, Action::AgentDefinitionsRead).await?;
    let caller = Caller::new(actor.profile.id, actor.subject, &headers).map_err(Fault::status)?;
    let tools = state
        .operations
        .authoring_capabilities(&caller, &[])
        .await
        .map_err(Fault::status)?;
    let choices = tools
        .into_iter()
        .take(512)
        .map(|t| {
            Ok(wire::CapabilityChoice {
                name: veoveo_mcp_contract::GatewayToolName::new(t.name.to_string())
                    .map_err(|_| Fault::unavailable())?,
                title: t.title.unwrap_or_else(|| t.name.to_string()),
                description: t.description.map(|d| d.into_owned()).unwrap_or_default(),
            })
        })
        .collect::<Result<_, Fault>>()?;
    Ok(Json(choices))
}
