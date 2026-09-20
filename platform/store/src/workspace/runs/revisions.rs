//! Explicit participant adoption with replay and expected executable revisions.
use super::super::{Result, WorkspaceAuthority, WorkspaceError};
use super::{WorkspaceAgent, WorkspaceAgentAdmission};
use crate::{PlatformStore, WorkspaceAgentId, WorkspaceChatId};
use sha2::{Digest, Sha256};
use surrealdb::types::{RecordId, SurrealValue, ToSql};
use uuid::Uuid;

pub(super) fn receipt(
    authority: &WorkspaceAuthority,
    chat: WorkspaceChatId,
    request: Uuid,
    admission: &WorkspaceAgentAdmission,
    expected: Option<&str>,
) -> Result<(RecordId, String)> {
    if request.get_version_num() != 7 {
        return Err(WorkspaceError::Invalid("request identity"));
    }
    let id = Uuid::new_v5(&request, authority.principal.to_sql().as_bytes());
    let fingerprint = hex::encode(Sha256::digest(
        serde_json::to_vec(&(
            chat.as_uuid(),
            &admission.definition,
            &admission.definition_digest,
            expected,
        ))
        .map_err(|_| WorkspaceError::Unavailable)?,
    ));
    Ok((
        RecordId::new(
            "workspace_agent_revision_receipt",
            surrealdb::types::Uuid::from(id),
        ),
        fingerprint,
    ))
}

impl PlatformStore {
    pub async fn workspace_agent_for_update(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        agent: WorkspaceAgentId,
    ) -> Result<WorkspaceAgent> {
        self.workspace_query(authority, super::Target { chat: chat.record_id(), target: agent.record_id() },
            "IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; }; LET $chat = SELECT * FROM ONLY $command.chat; IF $chat.owner != $authority.principal OR $authority.membership = 'viewer' { THROW 'workspace_forbidden'; }; IF $chat.archived { THROW 'workspace_conflict'; }; LET $agent = SELECT * FROM ONLY $command.target; IF $agent.chat != $command.chat OR !$agent.active { THROW 'workspace_not_found'; }; RETURN $agent;"
        ).await
    }

    pub async fn update_workspace_agent_revision(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        request_id: Uuid,
        expected: &str,
        admission: WorkspaceAgentAdmission,
    ) -> Result<WorkspaceAgent> {
        for value in [expected, &admission.definition_digest] {
            if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(WorkspaceError::Invalid("agent revision"));
            }
        }
        let (receipt, fingerprint) =
            receipt(authority, chat, request_id, &admission, Some(expected))?;
        #[derive(Clone, SurrealValue)]
        struct Adopt {
            chat: RecordId,
            agent: RecordId,
            receipt: RecordId,
            fingerprint: String,
            expected: String,
            admission: WorkspaceAgentAdmission,
        }
        let agent = WorkspaceAgentId::from_uuid(Uuid::new_v5(
            &chat.as_uuid(),
            admission.definition.as_bytes(),
        ));
        self.workspace_query(
            authority,
            Adopt {
                chat: chat.record_id(),
                agent: agent.record_id(),
                receipt,
                fingerprint,
                expected: expected.to_owned(),
                admission,
            },
            include_str!("adopt.surql"),
        )
        .await
    }
}
