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
        reply_to: value
            .reply_to
            .as_ref()
            .map(uuid)
            .transpose()?
            .map(wire::MessageId),
        sequence: value.sequence,
        created_at: value.created_at,
    })
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
