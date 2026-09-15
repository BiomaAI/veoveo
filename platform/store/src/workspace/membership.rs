use chrono::{TimeDelta, Utc};
use surrealdb::types::{RecordId, SurrealValue};

use super::{
    Result, WorkspaceAuthority, WorkspaceChat, WorkspaceError, WorkspaceInvitation,
    WorkspaceInvitationState, WorkspaceMember, human_member, validate_text,
};
use crate::{PlatformStore, PrincipalId, WorkspaceChatId, WorkspaceInvitationId};

#[derive(Clone, SurrealValue)]
struct Invite {
    chat: RecordId,
    invitation: RecordId,
    invitee: RecordId,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Clone, SurrealValue)]
struct DecideInvitation {
    chat: RecordId,
    invitation: RecordId,
    member: RecordId,
    state: WorkspaceInvitationState,
}

#[derive(Clone, SurrealValue)]
struct RemoveMember {
    chat: RecordId,
    member: RecordId,
}

#[derive(Clone, SurrealValue)]
struct Settings {
    chat: RecordId,
    expected_revision: i64,
    title: String,
    archived: bool,
    members_can_invite: bool,
    owner: RecordId,
}

/// Full owner settings replacement with optimistic concurrency. The new owner
/// must already be an active human member; ownership cannot be abandoned.
pub struct WorkspaceSettings {
    pub expected_revision: i64,
    pub title: String,
    pub archived: bool,
    pub members_can_invite: bool,
    pub owner: PrincipalId,
}

impl PlatformStore {
    pub async fn invite_workspace_member(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        invitation: WorkspaceInvitationId,
        invitee: PrincipalId,
    ) -> Result<WorkspaceInvitation> {
        self.workspace_query(
            authority,
            Invite {
                chat: chat.record_id(),
                invitation: invitation.record_id(),
                invitee: invitee.record_id(),
                expires_at: Utc::now() + TimeDelta::days(7),
            },
            include_str!("queries/invite.surql"),
        )
        .await
    }

    pub async fn list_workspace_invitations(
        &self,
        authority: &WorkspaceAuthority,
    ) -> Result<Vec<WorkspaceInvitation>> {
        self.workspace_query(
            authority,
            false,
            "RETURN SELECT * FROM workspace_invitation WHERE invitee = $authority.principal \
             AND state = 'pending' AND expires_at > time::now() \
             AND chat.tenant = $authority.tenant AND chat.work_context = $authority.work_context \
             ORDER BY created_at DESC LIMIT 100;",
        )
        .await
    }

    pub async fn decide_workspace_invitation(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        invitation: WorkspaceInvitationId,
        state: WorkspaceInvitationState,
    ) -> Result<WorkspaceInvitation> {
        if state == WorkspaceInvitationState::Pending {
            return Err(WorkspaceError::Invalid("invitation decision"));
        }
        self.workspace_query(
            authority,
            DecideInvitation {
                chat: chat.record_id(),
                invitation: invitation.record_id(),
                member: human_member(chat, &authority.principal).record_id(),
                state,
            },
            include_str!("queries/decide_invitation.surql"),
        )
        .await
    }

    pub async fn remove_workspace_member(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        principal: PrincipalId,
    ) -> Result<WorkspaceMember> {
        self.workspace_query(
            authority,
            RemoveMember {
                chat: chat.record_id(),
                member: human_member(chat, &principal.record_id()).record_id(),
            },
            include_str!("queries/remove_member.surql"),
        )
        .await
    }

    pub async fn update_workspace_settings(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        settings: WorkspaceSettings,
    ) -> Result<WorkspaceChat> {
        validate_text(&settings.title, "title", 200)?;
        self.workspace_query(
            authority,
            Settings {
                chat: chat.record_id(),
                expected_revision: settings.expected_revision,
                title: settings.title,
                archived: settings.archived,
                members_can_invite: settings.members_can_invite,
                owner: settings.owner.record_id(),
            },
            include_str!("queries/settings.surql"),
        )
        .await
    }
}
