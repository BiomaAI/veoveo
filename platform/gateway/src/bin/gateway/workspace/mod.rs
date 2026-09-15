mod authority;
pub(crate) mod events;
mod projection;
#[cfg(test)]
mod tests;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, Query, State},
    http::{HeaderValue, StatusCode, header::CACHE_CONTROL},
    routing::{delete, get, post},
};
use serde::Deserialize;
use tower_http::set_header::SetResponseHeaderLayer;
use uuid::Uuid;
use veoveo_mcp_contract::workspace as wire;
use veoveo_mcp_gateway::AuthenticatedSubject;
use veoveo_platform_store::{
    PlatformStore, PrincipalId, WorkspaceChatId, WorkspaceInvitationId, WorkspaceMessageId,
    workspace::{WorkspaceError, WorkspaceInvitationState, WorkspaceSettings},
};

#[derive(Clone)]
pub(crate) struct WorkspaceState {
    pub store: PlatformStore,
}

pub(crate) fn router(state: WorkspaceState) -> Router {
    Router::new()
        .route("/workspace-api/{profile}/session", get(bootstrap))
        .route("/workspace-api/{profile}/chats", get(list).post(create))
        .route(
            "/workspace-api/{profile}/chats/{chat}",
            get(snapshot).put(settings),
        )
        .route("/workspace-api/{profile}/chats/{chat}/messages", post(send))
        .route(
            "/workspace-api/{profile}/chats/{chat}/members/{person}",
            delete(remove),
        )
        .route(
            "/workspace-api/{profile}/chats/{chat}/invitations",
            post(invite),
        )
        .route("/workspace-api/{profile}/invitations", get(invitations))
        .route(
            "/workspace-api/{profile}/invitations/{invitation}",
            post(decide),
        )
        .route("/workspace-api/{profile}/people", get(people))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .with_state(state)
}

type Api<T> = Result<Json<T>, StatusCode>;

async fn bootstrap(
    State(state): State<WorkspaceState>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<wire::WorkspaceBootstrap> {
    let actor = authority::admit(&state, &subject).await?;
    let identity = state
        .store
        .workspace_identity(&actor)
        .await
        .map_err(fault)?;
    Ok(Json(wire::WorkspaceBootstrap {
        person: projection::person(identity.person)?,
        principal_id: subject.principal.id,
        tenant_id: subject.authority.tenant,
        tenant_name: identity.tenant_name,
        work_context: subject.authority.work_context,
        work_context_title: identity.work_context_title,
        can_contribute: actor.membership
            != veoveo_platform_store::WorkContextMembershipLevel::Viewer,
    }))
}

fn fault(error: WorkspaceError) -> StatusCode {
    match error {
        WorkspaceError::Invalid(_) => StatusCode::BAD_REQUEST,
        WorkspaceError::NotFound => StatusCode::NOT_FOUND,
        WorkspaceError::Forbidden => StatusCode::FORBIDDEN,
        WorkspaceError::Conflict => StatusCode::CONFLICT,
        WorkspaceError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

async fn list(
    State(state): State<WorkspaceState>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<Vec<wire::Chat>> {
    let actor = authority::admit(&state, &subject).await?;
    let chats = state
        .store
        .list_workspace_chats(&actor)
        .await
        .map_err(fault)?;
    Ok(Json(
        chats
            .into_iter()
            .map(projection::chat)
            .collect::<Result<_, _>>()?,
    ))
}

async fn create(
    State(state): State<WorkspaceState>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<wire::CreateChat>,
) -> Api<wire::Chat> {
    let actor = authority::admit(&state, &subject).await?;
    let chat = state
        .store
        .create_workspace_chat(
            &actor,
            WorkspaceChatId::from_uuid(request.id.0),
            &request.title,
        )
        .await
        .map_err(fault)?;
    Ok(Json(projection::chat(chat)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    after: Option<i64>,
    before: Option<i64>,
}

async fn snapshot(
    State(state): State<WorkspaceState>,
    Path((_profile, chat)): Path<(String, Uuid)>,
    Query(page): Query<Page>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<wire::ChatSnapshot> {
    let actor = authority::admit(&state, &subject).await?;
    let chat = WorkspaceChatId::from_uuid(chat);
    let snapshot = match (page.after, page.before) {
        (Some(after), None) => {
            state
                .store
                .workspace_snapshot(&actor, chat, after, 100)
                .await
        }
        (None, before) => {
            state
                .store
                .workspace_recent_snapshot(&actor, chat, before, 100)
                .await
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    }
    .map_err(fault)?;
    let people = state
        .store
        .workspace_member_people(&actor, chat)
        .await
        .map_err(fault)?;
    let members = snapshot
        .members
        .into_iter()
        .map(|member| {
            let person = people
                .iter()
                .find(|person| person.id == member.principal)
                .cloned()
                .ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
            Ok(wire::Member {
                id: wire::MemberId(projection::uuid(&member.id)?),
                person: projection::person(person)?,
                active: member.active,
            })
        })
        .collect::<Result<_, StatusCode>>()?;
    Ok(Json(wire::ChatSnapshot {
        chat: projection::chat(snapshot.chat)?,
        members,
        messages: snapshot
            .messages
            .into_iter()
            .map(projection::message)
            .collect::<Result<_, _>>()?,
    }))
}

async fn send(
    State(state): State<WorkspaceState>,
    Path((_profile, chat)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<wire::SendMessage>,
) -> Api<wire::Message> {
    let actor = authority::admit(&state, &subject).await?;
    let message = state
        .store
        .send_workspace_message(
            &actor,
            WorkspaceChatId::from_uuid(chat),
            WorkspaceMessageId::from_uuid(request.id.0),
            &request.text,
            request
                .reply_to
                .map(|id| WorkspaceMessageId::from_uuid(id.0)),
        )
        .await
        .map_err(fault)?;
    Ok(Json(projection::message(message)?))
}

async fn invite(
    State(state): State<WorkspaceState>,
    Path((_profile, chat)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<wire::InvitePerson>,
) -> Api<wire::Invitation> {
    let actor = authority::admit(&state, &subject).await?;
    let invitation = state
        .store
        .invite_workspace_member(
            &actor,
            WorkspaceChatId::from_uuid(chat),
            WorkspaceInvitationId::from_uuid(request.id.0),
            PrincipalId::from_uuid(request.invitee.0),
        )
        .await
        .map_err(fault)?;
    Ok(Json(projection::invitation(invitation)?))
}

async fn invitations(
    State(state): State<WorkspaceState>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<Vec<wire::InvitationSummary>> {
    let actor = authority::admit(&state, &subject).await?;
    let invitations = state
        .store
        .workspace_invitation_inbox(&actor)
        .await
        .map_err(fault)?;
    Ok(Json(
        invitations
            .into_iter()
            .map(|value| {
                Ok::<_, StatusCode>(wire::InvitationSummary {
                    invitation: projection::invitation(value.invitation)?,
                    chat_title: value.chat_title,
                    inviter_name: value.inviter_name,
                })
            })
            .collect::<Result<_, _>>()?,
    ))
}

async fn decide(
    State(state): State<WorkspaceState>,
    Path((_profile, invitation)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<wire::DecideInvitation>,
) -> Api<wire::Invitation> {
    let actor = authority::admit(&state, &subject).await?;
    let decision = match request.state {
        wire::InvitationState::Pending => return Err(StatusCode::BAD_REQUEST),
        wire::InvitationState::Accepted => WorkspaceInvitationState::Accepted,
        wire::InvitationState::Declined => WorkspaceInvitationState::Declined,
        wire::InvitationState::Revoked => WorkspaceInvitationState::Revoked,
    };
    let result = state
        .store
        .decide_workspace_invitation(
            &actor,
            WorkspaceChatId::from_uuid(request.chat_id.0),
            WorkspaceInvitationId::from_uuid(invitation),
            decision,
        )
        .await
        .map_err(fault)?;
    Ok(Json(projection::invitation(result)?))
}

async fn remove(
    State(state): State<WorkspaceState>,
    Path((_profile, chat, person)): Path<(String, Uuid, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Result<StatusCode, StatusCode> {
    let actor = authority::admit(&state, &subject).await?;
    state
        .store
        .remove_workspace_member(
            &actor,
            WorkspaceChatId::from_uuid(chat),
            PrincipalId::from_uuid(person),
        )
        .await
        .map_err(fault)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn settings(
    State(state): State<WorkspaceState>,
    Path((_profile, chat)): Path<(String, Uuid)>,
    Extension(subject): Extension<AuthenticatedSubject>,
    Json(request): Json<wire::ChatSettings>,
) -> Api<wire::Chat> {
    let actor = authority::admit(&state, &subject).await?;
    let result = state
        .store
        .update_workspace_settings(
            &actor,
            WorkspaceChatId::from_uuid(chat),
            WorkspaceSettings {
                expected_revision: request.expected_revision,
                title: request.title,
                archived: request.archived,
                members_can_invite: request.members_can_invite,
                owner: PrincipalId::from_uuid(request.owner.0),
            },
        )
        .await
        .map_err(fault)?;
    Ok(Json(projection::chat(result)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    q: String,
}

async fn people(
    State(state): State<WorkspaceState>,
    Query(search): Query<Search>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Api<Vec<wire::Person>> {
    let actor = authority::admit(&state, &subject).await?;
    let people = state
        .store
        .search_workspace_people(&actor, &search.q)
        .await
        .map_err(fault)?;
    Ok(Json(
        people
            .into_iter()
            .map(projection::person)
            .collect::<Result<_, _>>()?,
    ))
}
