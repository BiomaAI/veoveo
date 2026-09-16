use axum::http::StatusCode;
use uuid::Uuid;
use veoveo_mcp_contract::workspace as wire;
use veoveo_platform_store::{RecordId, RecordIdKey, workspace as stored};

pub(super) fn uuid(record: &RecordId) -> Result<Uuid, StatusCode> {
    match &record.key {
        RecordIdKey::Uuid(value) => Ok(**value),
        _ => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

pub(super) fn chat(value: stored::WorkspaceChat) -> Result<wire::Chat, StatusCode> {
    Ok(wire::Chat {
        id: wire::ChatId(uuid(&value.id)?),
        title: value.title,
        owner: wire::PersonId(uuid(&value.owner)?),
        archived: value.archived,
        members_can_invite: value.members_can_invite,
        participation: participation(value.participation.unwrap_or_default())?,
        sequence: value.sequence,
        revision: value.revision,
        updated_at: value.updated_at,
    })
}

pub(super) fn message(value: stored::WorkspaceMessage) -> Result<wire::Message, StatusCode> {
    Ok(wire::Message {
        id: wire::MessageId(uuid(&value.id)?),
        author: wire::MemberId(uuid(&value.author)?),
        text: value.text,
        attachments: attachments(value.attachments.unwrap_or_default())?,
        reply_to: value.reply_to.as_ref().map(reply_target).transpose()?,
        reply_context: value.reply_context.map(|context| wire::ReplyContext {
            author_name: context.author_name,
            text: context.text,
        }),
        sequence: value.sequence,
        addressed_agents: agent_ids(value.addressed_agents.unwrap_or_default())?,
        response_agents: agent_ids(value.response_agents.unwrap_or_default())?,
        created_at: value.created_at,
    })
}

pub(super) fn attachments(
    values: Vec<stored::WorkspaceAttachment>,
) -> Result<Vec<wire::ChatAttachment>, StatusCode> {
    values
        .into_iter()
        .map(|value| {
            Ok(wire::ChatAttachment::Artifact {
                id: veoveo_mcp_contract::ArtifactId::parse(value.artifact.to_string())
                    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?,
                name: value.name,
            })
        })
        .collect()
}

pub(super) fn reply_target(value: &RecordId) -> Result<wire::ReplyTarget, StatusCode> {
    match value.table.as_str() {
        "workspace_message" => Ok(wire::ReplyTarget::Message {
            id: wire::MessageId(uuid(value)?),
        }),
        "workspace_run" => Ok(wire::ReplyTarget::Response {
            id: wire::RunId(uuid(value)?),
        }),
        _ => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

pub(super) fn person(value: stored::WorkspacePerson) -> Result<wire::Person, StatusCode> {
    Ok(wire::Person {
        id: wire::PersonId(uuid(&value.id)?),
        display_name: value.display_name,
    })
}

pub(super) fn invitation(
    value: stored::WorkspaceInvitation,
) -> Result<wire::Invitation, StatusCode> {
    use stored::WorkspaceInvitationState as State;
    Ok(wire::Invitation {
        id: wire::InvitationId(uuid(&value.id)?),
        chat_id: wire::ChatId(uuid(&value.chat)?),
        inviter: wire::PersonId(uuid(&value.inviter)?),
        invitee: wire::PersonId(uuid(&value.invitee)?),
        state: match value.state {
            State::Pending => wire::InvitationState::Pending,
            State::Accepted => wire::InvitationState::Accepted,
            State::Declined => wire::InvitationState::Declined,
            State::Revoked => wire::InvitationState::Revoked,
        },
        expires_at: value.expires_at,
    })
}

fn agent_ids(values: Vec<RecordId>) -> Result<Vec<wire::AgentId>, StatusCode> {
    values
        .iter()
        .map(|id| uuid(id).map(wire::AgentId))
        .collect()
}
fn participation(value: stored::WorkspaceParticipation) -> Result<wire::Participation, StatusCode> {
    Ok(wire::Participation {
        mode: match value.mode {
            stored::WorkspaceParticipationMode::OnRequest => wire::ParticipationMode::OnRequest,
            stored::WorkspaceParticipationMode::Default => wire::ParticipationMode::Default,
            stored::WorkspaceParticipationMode::Automatic => wire::ParticipationMode::Automatic,
        },
        agents: agent_ids(value.agents)?,
    })
}
pub(super) fn participation_request(value: wire::Participation) -> stored::WorkspaceParticipation {
    stored::WorkspaceParticipation {
        mode: match value.mode {
            wire::ParticipationMode::OnRequest => stored::WorkspaceParticipationMode::OnRequest,
            wire::ParticipationMode::Default => stored::WorkspaceParticipationMode::Default,
            wire::ParticipationMode::Automatic => stored::WorkspaceParticipationMode::Automatic,
        },
        agents: value
            .agents
            .into_iter()
            .map(|id| veoveo_platform_store::WorkspaceAgentId::from_uuid(id.0).record_id())
            .collect(),
    }
}
