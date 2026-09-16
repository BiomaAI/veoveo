//! Message admission owns response intent; browser lifetime never splits it.
use super::*;
use veoveo_platform_store::workspace::{
    WorkspaceReplyTarget, WorkspaceRunFailure, WorkspaceTurnRequest,
};

pub(super) async fn send(
    State(state): State<RunState>,
    Path((profile, chat)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    headers: HeaderMap,
    Json(request): Json<wire::SendMessage>,
) -> Api<wire::Message> {
    let actor = authority::admit(&state.workspace, &subject).await?;
    let profile = GatewayProfileId::new(profile).map_err(|_| StatusCode::NOT_FOUND)?;
    let caller = Caller::new(profile, subject.clone(), &headers)?;
    let chat = WorkspaceChatId::from_uuid(chat);
    let turn = state
        .workspace
        .store
        .send_workspace_turn(
            &actor,
            chat,
            WorkspaceTurnRequest {
                id: WorkspaceMessageId::from_uuid(request.id.0),
                text: request.text,
                attachments: request
                    .attachments
                    .into_iter()
                    .map(|value| match value {
                        wire::ChatAttachment::Artifact { id, name } => {
                            veoveo_platform_store::workspace::WorkspaceAttachment {
                                artifact: veoveo_platform_store::ArtifactId::from_uuid(
                                    id.as_uuid(),
                                ),
                                name,
                            }
                        }
                    })
                    .collect(),
                reply_to: request.reply_to.map(|target| match target {
                    wire::ReplyTarget::Message { id } => {
                        WorkspaceReplyTarget::Message(WorkspaceMessageId::from_uuid(id.0))
                    }
                    wire::ReplyTarget::Response { id } => {
                        WorkspaceReplyTarget::Response(WorkspaceRunId::from_uuid(id.0))
                    }
                }),
                addressed_agents: request
                    .addressed_agents
                    .into_iter()
                    .map(|id| WorkspaceAgentId::from_uuid(id.0))
                    .collect(),
                deadline: subject
                    .access_token
                    .expires_at
                    .min(Utc::now() + TimeDelta::seconds(120)),
            },
        )
        .await
        .map_err(fault)?;
    let response = super::super::projection::message(turn.message)?;
    if !turn
        .runs
        .iter()
        .any(|run| run.state == WorkspaceRunState::Queued)
    {
        return Ok(Json(response));
    }
    // One owned task also survives a lost HTTP response. A restart leaves the
    // durable queued lease to reconciliation; it never guesses whether to retry.
    tokio::spawn(async move {
        let Ok(actor) = authority::admit_live(
            &state.workspace,
            &state.gateway,
            &state.catalog.current(),
            &caller.profile,
            &caller.subject,
        )
        .await
        else {
            return;
        };
        let Ok(agents) = state.workspace.store.workspace_agents(&actor, chat).await else {
            return;
        };
        for run in turn
            .runs
            .into_iter()
            .filter(|run| run.state == WorkspaceRunState::Queued)
        {
            let definition = agents
                .iter()
                .find(|agent| agent.id == run.agent && agent.active)
                .and_then(|agent| {
                    state.definitions.iter().find(|definition| {
                        definition.id == agent.definition
                            && definition.permits(&caller.subject)
                            && definition.digest() == run.definition_digest
                    })
                })
                .cloned();
            let permit = state.limits.clone().try_acquire_owned();
            let failure = match (definition, permit) {
                (Some(definition), Ok(permit)) => {
                    tokio::spawn(worker::execute(
                        state.clone(),
                        caller.clone(),
                        definition,
                        run,
                        permit,
                    ));
                    continue;
                }
                (None, _) => WorkspaceRunFailure::PermissionChanged,
                (_, Err(_)) => WorkspaceRunFailure::Capacity,
            };
            if let Ok(id) = super::super::projection::uuid(&run.id) {
                let _ = state
                    .workspace
                    .store
                    .reject_workspace_run(&actor, chat, WorkspaceRunId::from_uuid(id), failure)
                    .await;
            }
        }
    });
    Ok(Json(response))
}
