use surrealdb::types::{RecordId, SurrealValue};

use super::{Result, WorkspaceAuthority, WorkspaceError, WorkspaceInvitation};
use crate::{PlatformStore, WorkContextId, WorkContextRecord, WorkspaceChatId};

#[derive(Clone, Debug, SurrealValue)]
pub struct WorkspaceContext {
    pub context: WorkContextRecord,
    pub digest: String,
}

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

impl PlatformStore {
    /// Only the named invitee receives this bounded disclosure before joining.
    pub async fn workspace_invitation_inbox(
        &self,
        authority: &WorkspaceAuthority,
    ) -> Result<Vec<WorkspaceInvitationSummary>> {
        self.workspace_query(authority, false,
            "RETURN SELECT { id: id, chat: chat, inviter: inviter, invitee: invitee, state: state, created_at: created_at, expires_at: expires_at } AS invitation, \
             chat.title AS chat_title, inviter.display_name AS inviter_name \
             FROM workspace_invitation WHERE invitee = $authority.principal \
             AND state = 'pending' AND expires_at > time::now() \
             AND chat.tenant = $authority.tenant AND chat.work_context = $authority.work_context \
             LIMIT 100;").await
    }

    pub async fn workspace_identity(
        &self,
        authority: &WorkspaceAuthority,
    ) -> Result<WorkspaceIdentity> {
        self.workspace_query(
            authority,
            false,
            "LET $person = SELECT id, display_name FROM ONLY $authority.principal; \
             RETURN { person: $person, tenant_name: $authority.tenant.name, \
             work_context_title: $authority.work_context.title };",
        )
        .await
    }

    /// Trusted admission input. The digest is computed from the exact policy
    /// record returned, rather than a second read which could observe new rules.
    pub async fn workspace_context(&self, context: WorkContextId) -> Result<WorkspaceContext> {
        let mut response = self
            .client()
            .query(
                "LET $current = SELECT * FROM ONLY $context; \
             RETURN { context: $current, digest: crypto::sha256(<string> \
             [$current.policy_revision, $current.memberships, $current.output_policy]) };",
            )
            .bind(("context", context.record_id()))
            .await
            .map_err(|_| WorkspaceError::Unavailable)?
            .check()
            .map_err(|_| WorkspaceError::Unavailable)?;
        response
            .take::<Option<WorkspaceContext>>(1)
            .map_err(|_| WorkspaceError::Unavailable)?
            .ok_or(WorkspaceError::Unavailable)
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
            "RETURN SELECT id, display_name FROM principal WHERE tenant = $authority.tenant \
             AND enabled = true AND kind = 'user' \
             AND string::contains(string::lowercase(display_name), $command.search) \
             ORDER BY display_name ASC LIMIT 20;",
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
            PeopleQuery { chat: Some(chat.record_id()), search: String::new() },
            "IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; }; \
             RETURN SELECT id, display_name FROM principal WHERE tenant = $authority.tenant \
             AND id IN (SELECT VALUE principal FROM workspace_member WHERE chat = $command.chat LIMIT 256) LIMIT 256;",
        ).await
    }
}
