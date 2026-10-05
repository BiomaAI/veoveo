use surrealdb::types::{RecordId, SurrealValue};

use super::{Result, WorkspaceAuthority, WorkspaceError, WorkspaceInvitation};

use crate::persistence::{WorkspaceChatId, WorkspaceRepository};

#[derive(Clone, Debug, PartialEq, SurrealValue)]
pub struct WorkspacePerson {
    pub id: RecordId,
    pub display_name: String,
}

#[derive(Clone, Debug, SurrealValue)]
pub struct WorkspaceIdentity {
    pub person: WorkspacePerson,
    pub tenant_name: String,
    pub work_context_title: String,
}

#[derive(Clone, Debug, SurrealValue)]
pub struct WorkspaceInvitationSummary {
    pub invitation: WorkspaceInvitation,
    pub chat_title: String,
    pub inviter_name: String,
}

#[derive(Clone, SurrealValue)]
struct PeopleQuery {
    chat: Option<RecordId>,
    search: String,
}

impl WorkspaceRepository {
    /// Only the named invitee receives this bounded disclosure before joining.
    pub async fn workspace_invitation_inbox(
        &self,
        authority: &WorkspaceAuthority,
    ) -> Result<Vec<WorkspaceInvitationSummary>> {
        self.workspace_query(
            authority,
            false,
            include_str!("queries/people/workspace_invitation_inbox.surql"),
        )
        .await
    }

    pub async fn workspace_identity(
        &self,
        authority: &WorkspaceAuthority,
    ) -> Result<WorkspaceIdentity> {
        self.workspace_query(
            authority,
            false,
            include_str!("queries/people/workspace_identity.surql"),
        )
        .await
    }

    pub async fn search_workspace_people(
        &self,
        authority: &WorkspaceAuthority,
        search: &str,
    ) -> Result<Vec<WorkspacePerson>> {
        let search = search.trim();
        if search.chars().count() < 2 || search.len() > 128 {
            return Err(WorkspaceError::Invalid("people search"));
        }
        self.workspace_query(
            authority,
            PeopleQuery {
                chat: None,
                search: search.to_lowercase(),
            },
            include_str!("queries/people/search_workspace_people.surql"),
        )
        .await
    }

    pub async fn workspace_member_people(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
    ) -> Result<Vec<WorkspacePerson>> {
        self.workspace_query(
            authority,
            PeopleQuery {
                chat: Some(chat.record_id()),
                search: String::new(),
            },
            include_str!("queries/people/workspace_member_people.surql"),
        )
        .await
    }
}
