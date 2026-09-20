//! Only a chat owner can inspect and adopt a participant's executable revision.
use super::*;
use axum::extract::Query;
use veoveo_mcp_contract::Sha256Digest;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Target {
    revision: Option<Sha256Digest>,
}

pub(super) async fn preview(
    State(state): State<RunState>,
    Path((profile, chat, agent)): Path<(String, Uuid, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Query(target): Query<Target>,
) -> Api<wire::AgentRevisionPreview> {
    let actor = authority::admit(&state.workspace, &subject).await?;
    let profile = GatewayProfileId::new(profile).map_err(|_| StatusCode::NOT_FOUND)?;
    let admitted = state
        .workspace
        .store
        .workspace_agent_for_update(
            &actor,
            WorkspaceChatId::from_uuid(chat),
            WorkspaceAgentId::from_uuid(agent),
        )
        .await
        .map_err(fault)?;
    let target = state
        .agents
        .resolve(
            &profile,
            &subject,
            &admitted.definition,
            target.revision.as_ref().map(|r| r.hex()),
        )
        .await?;
    let current = state
        .agents
        .revision_view(
            &profile,
            &subject,
            &admitted.definition,
            &admitted.definition_digest,
        )
        .await?;
    let target = state
        .agents
        .revision_view(&profile, &subject, &admitted.definition, &target.revision)
        .await?;
    Ok(Json(wire::AgentRevisionPreview { current, target }))
}

pub(super) async fn adopt(
    State(state): State<RunState>,
    Path((profile, chat, agent)): Path<(String, Uuid, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<wire::UpdateChatAgent>,
) -> Api<wire::ChatAgent> {
    let actor = authority::admit(&state.workspace, &subject).await?;
    let profile = GatewayProfileId::new(profile).map_err(|_| StatusCode::NOT_FOUND)?;
    let chat = WorkspaceChatId::from_uuid(chat);
    let admitted = state
        .workspace
        .store
        .workspace_agent_for_update(&actor, chat, WorkspaceAgentId::from_uuid(agent))
        .await
        .map_err(fault)?;
    let target = state
        .agents
        .resolve(
            &profile,
            &subject,
            &admitted.definition,
            Some(request.revision.hex()),
        )
        .await?;
    let result = state
        .workspace
        .store
        .update_workspace_agent_revision(
            &actor,
            chat,
            request.request_id,
            request.expected_revision.hex(),
            target.admission(),
        )
        .await
        .map_err(fault)?;
    Ok(Json(projection::agent(result)?))
}
