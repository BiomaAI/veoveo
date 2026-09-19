use std::collections::BTreeMap;

use axum::{
    Json,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
};
use uuid::Uuid;
use veoveo_mcp_contract::{GatewayAction as Action, PolicyTarget, agent_management as wire};
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::{PrincipalId, agent_management as domain};

use super::{
    AgentManagementState, Api, Fault,
    authority::{self, Admission},
    projection, validation,
};
use crate::audit::{
    AdminOperationAuditRecord, AdminOperationFailure, AdminOperationStatus,
    record_gateway_operation_audit,
};

async fn finish(
    state: &AgentManagementState,
    actor: &Admission,
    request_id: Uuid,
    result: Result<domain::AgentDefinition, Fault>,
) -> Api<wire::Definition> {
    let result = match result {
        Ok(value) => projection::definitions(state, actor, vec![value])
            .await
            .and_then(|mut values| values.pop().ok_or_else(Fault::unavailable)),
        Err(error) => Err(error),
    };
    let status = match &result {
        Ok(_) => AdminOperationStatus::Succeeded,
        Err(Fault::Status(code)) if code.is_server_error() => AdminOperationStatus::Failed,
        Err(_) => AdminOperationStatus::Rejected,
    };
    record_gateway_operation_audit(
        &state.gateway,
        &actor.profile,
        &actor.subject,
        PolicyTarget::Gateway,
        AdminOperationAuditRecord {
            action: actor.action,
            method: "admin/agent-management/result",
            started_at: actor.started,
            status,
            failure: result
                .as_ref()
                .err()
                .map(|_| AdminOperationFailure::AgentManagement),
            metadata: BTreeMap::from([("request_id".into(), request_id.to_string())]),
        },
    )
    .await
    .map_err(|_| Fault::unavailable())?;
    result.map(Json)
}

pub(super) async fn create(
    State(state): State<AgentManagementState>,
    Path(profile): Path<String>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<wire::CreateDefinition>,
) -> Api<wire::Definition> {
    let actor = authority::admit(&state, profile, subject, Action::AgentDefinitionsCreate).await?;
    let result = async {
        let content = match request.source {
            wire::DefinitionSource::Blank { content } => projection::content(content),
            wire::DefinitionSource::Duplicate { definition, digest } => {
                if !authority::allowed(
                    &actor.catalog,
                    &actor.profile.id,
                    &actor.subject,
                    Action::AgentDefinitionsReadContent,
                ) {
                    return Err(Fault::status(StatusCode::FORBIDDEN));
                }
                state
                    .store()
                    .agent_authored_revision(&actor.authority, definition.as_str(), digest.hex())
                    .await?
                    .content
            }
        };
        Ok(state
            .store()
            .mutate_agent_definition(
                &actor.authority,
                request.id.as_str(),
                request.request_id,
                None,
                domain::AgentDefinitionMutation::Create {
                    name: request.name,
                    description: request.description,
                    content,
                },
            )
            .await?)
    }
    .await;
    finish(&state, &actor, request.request_id, result).await
}

pub(super) async fn draft(
    State(state): State<AgentManagementState>,
    Path((profile, id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<wire::SaveDraft>,
) -> Api<wire::Definition> {
    let actor = authority::admit(&state, profile, subject, Action::AgentDefinitionsEdit).await?;
    let result = state
        .store()
        .mutate_agent_definition(
            &actor.authority,
            &id,
            request.request_id,
            Some(request.expected_revision),
            domain::AgentDefinitionMutation::Draft {
                content: projection::content(request.content),
            },
        )
        .await
        .map_err(Fault::from);
    finish(&state, &actor, request.request_id, result).await
}

pub(super) async fn metadata(
    State(state): State<AgentManagementState>,
    Path((profile, id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<wire::UpdateMetadata>,
) -> Api<wire::Definition> {
    let (action, mutation) = match request.change {
        wire::MetadataChange::Presentation { name, description } => (
            Action::AgentDefinitionsEdit,
            domain::AgentDefinitionMutation::Metadata { name, description },
        ),
        wire::MetadataChange::Transfer { owner } => (
            Action::AgentDefinitionsTransfer,
            domain::AgentDefinitionMutation::Transfer {
                owner: PrincipalId::from_uuid(owner).record_id(),
            },
        ),
    };
    let actor = authority::admit(&state, profile, subject, action).await?;
    let result = state
        .store()
        .mutate_agent_definition(
            &actor.authority,
            &id,
            request.request_id,
            Some(request.expected_revision),
            mutation,
        )
        .await
        .map_err(Fault::from);
    finish(&state, &actor, request.request_id, result).await
}

pub(super) async fn publish(
    State(state): State<AgentManagementState>,
    Path((profile, id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(request): Json<wire::PublishDefinition>,
) -> Api<wire::Definition> {
    let actor = authority::admit(&state, profile, subject, Action::AgentDefinitionsPublish).await?;
    let result = async {
        let audience = validation::audience(&state, &actor, &request.audience).await?;
        let mutation = domain::AgentDefinitionMutation::Publish {
            digest: request.digest.hex().to_owned(),
            audience,
        };
        if let Some(receipt) = state
            .store()
            .replay_agent_definition(
                &actor.authority,
                &id,
                request.request_id,
                Some(request.expected_revision),
                mutation.clone(),
            )
            .await?
        {
            return Ok(receipt);
        }
        let definition = state
            .store()
            .agent_definition(&actor.authority, &id)
            .await?;
        if definition.revision != request.expected_revision
            || definition.draft_digest != request.digest.hex()
        {
            return Err(Fault::status(StatusCode::CONFLICT));
        }
        let (validation, _) =
            validation::check(&state, &actor, &definition, &request.audience, &headers).await?;
        if !validation.findings.is_empty() {
            return Err(Fault::Validation(validation));
        }
        authority::live_session(&state, &actor.profile, &actor.subject).await?;
        Ok(state
            .store()
            .mutate_agent_definition(
                &actor.authority,
                &id,
                request.request_id,
                Some(request.expected_revision),
                mutation,
            )
            .await?)
    }
    .await;
    finish(&state, &actor, request.request_id, result).await
}

async fn status(
    state: AgentManagementState,
    profile: String,
    id: String,
    subject: AuthenticatedSubject,
    request: wire::RevisionRequest,
    headers: HeaderMap,
    status: domain::AgentDefinitionStatus,
) -> Api<wire::Definition> {
    let action = if status == domain::AgentDefinitionStatus::Archived {
        Action::AgentDefinitionsArchive
    } else {
        Action::AgentDefinitionsControl
    };
    let actor = authority::admit(&state, profile, subject, action).await?;
    let result = async {
        let mutation = domain::AgentDefinitionMutation::Status { status };
        if let Some(receipt) = state
            .store()
            .replay_agent_definition(
                &actor.authority,
                &id,
                request.request_id,
                Some(request.expected_revision),
                mutation.clone(),
            )
            .await?
        {
            return Ok(receipt);
        }
        if status == domain::AgentDefinitionStatus::Enabled {
            let mut definition = state
                .store()
                .agent_definition(&actor.authority, &id)
                .await?;
            if definition.revision != request.expected_revision {
                return Err(Fault::status(StatusCode::CONFLICT));
            }
            let audience = if definition.audience.is_empty() {
                vec![actor.subject.authority.work_context.clone()]
            } else {
                definition
                    .audience
                    .iter()
                    .map(|context| {
                        projection::context(
                            &actor.catalog,
                            &actor.subject.authority.tenant,
                            context,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };
            if definition.published.is_some() {
                let published = projection::definitions(&state, &actor, vec![definition.clone()])
                    .await?
                    .pop()
                    .and_then(|d| d.published_digest)
                    .ok_or_else(Fault::unavailable)?;
                definition.draft = state
                    .store()
                    .agent_authored_revision(&actor.authority, &id, published.hex())
                    .await?
                    .content;
                definition.draft_digest = published.hex().to_owned();
            }
            let (validation, _) =
                validation::check(&state, &actor, &definition, &audience, &headers).await?;
            if !validation.findings.is_empty() {
                return Err(Fault::Validation(validation));
            }
        }
        Ok(state
            .store()
            .mutate_agent_definition(
                &actor.authority,
                &id,
                request.request_id,
                Some(request.expected_revision),
                mutation,
            )
            .await?)
    }
    .await;
    finish(&state, &actor, request.request_id, result).await
}
pub(super) async fn disable(
    State(state): State<AgentManagementState>,
    Path((profile, id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(request): Json<wire::RevisionRequest>,
) -> Api<wire::Definition> {
    status(
        state,
        profile,
        id,
        subject,
        request,
        headers,
        domain::AgentDefinitionStatus::Disabled,
    )
    .await
}
pub(super) async fn enable(
    State(state): State<AgentManagementState>,
    Path((profile, id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(request): Json<wire::RevisionRequest>,
) -> Api<wire::Definition> {
    status(
        state,
        profile,
        id,
        subject,
        request,
        headers,
        domain::AgentDefinitionStatus::Enabled,
    )
    .await
}
pub(super) async fn archive(
    State(state): State<AgentManagementState>,
    Path((profile, id)): Path<(String, String)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(request): Json<wire::RevisionRequest>,
) -> Api<wire::Definition> {
    status(
        state,
        profile,
        id,
        subject,
        request,
        headers,
        domain::AgentDefinitionStatus::Archived,
    )
    .await
}
