//! Per-chat agent admission and independently fenced execution records.
mod records;
pub use records::*;

use chrono::{DateTime, Utc};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

use super::{Result, WorkspaceAuthority, WorkspaceError, human_member, validate_text};
use crate::{PlatformStore, WorkspaceAgentId, WorkspaceChatId, WorkspaceMessageId, WorkspaceRunId};

#[derive(Clone, SurrealValue)]
struct AgentCommand {
    chat: RecordId,
    agent: RecordId,
    admission: WorkspaceAgentAdmission,
}
#[derive(Clone, SurrealValue)]
struct Target {
    chat: RecordId,
    target: RecordId,
}
#[derive(Clone, SurrealValue)]
struct Start {
    chat: RecordId,
    agent: RecordId,
    trigger: RecordId,
    run: RecordId,
    member: RecordId,
    definition_digest: String,
    deadline: DateTime<Utc>,
}
#[derive(Clone, SurrealValue)]
struct Claim {
    chat: RecordId,
    run: RecordId,
    fence: Uuid,
}
#[derive(Clone, SurrealValue)]
struct Update {
    chat: RecordId,
    run: RecordId,
    update: WorkspaceRunUpdate,
}

impl PlatformStore {
    pub async fn add_workspace_agent(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        admission: WorkspaceAgentAdmission,
    ) -> Result<WorkspaceAgent> {
        for (field, value, bound) in [
            ("agent definition", admission.definition.as_str(), 128),
            ("agent name", admission.display_name.as_str(), 200),
            ("provider", admission.provider.as_str(), 200),
            ("model", admission.model.as_str(), 256),
        ] {
            validate_text(value, field, bound)?;
        }
        if admission.definition_digest.len() != 64
            || !admission
                .definition_digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err(WorkspaceError::Invalid("agent definition digest"));
        }
        let agent = WorkspaceAgentId::from_uuid(Uuid::new_v5(
            &chat.as_uuid(),
            admission.definition.as_bytes(),
        ));
        self.workspace_query(
            authority,
            AgentCommand {
                chat: chat.record_id(),
                agent: agent.record_id(),
                admission,
            },
            include_str!("add.surql"),
        )
        .await
    }

    pub async fn workspace_agents(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
    ) -> Result<Vec<WorkspaceAgent>> {
        self.workspace_query(
            authority,
            chat.record_id(),
            "IF !fn::workspace_member($authority, $command) { THROW 'workspace_not_found'; }; \
             RETURN SELECT * FROM workspace_agent WHERE chat = $command LIMIT 64;",
        )
        .await
    }

    pub async fn remove_workspace_agent(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        agent: WorkspaceAgentId,
    ) -> Result<WorkspaceAgent> {
        self.workspace_query(
            authority,
            Target {
                chat: chat.record_id(),
                target: agent.record_id(),
            },
            include_str!("remove.surql"),
        )
        .await
    }

    /// Stable identity for an explicitly addressed human message. Retrying a lost
    /// response cannot create another run for that message and agent.
    pub async fn start_workspace_run(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        agent: WorkspaceAgentId,
        trigger: WorkspaceMessageId,
        definition_digest: &str,
        deadline: DateTime<Utc>,
    ) -> Result<WorkspaceRun> {
        let run =
            WorkspaceRunId::from_uuid(Uuid::new_v5(&trigger.as_uuid(), agent.as_uuid().as_bytes()));
        self.workspace_query(
            authority,
            Start {
                chat: chat.record_id(),
                agent: agent.record_id(),
                trigger: trigger.record_id(),
                run: run.record_id(),
                member: human_member(chat, &authority.principal).record_id(),
                definition_digest: definition_digest.to_owned(),
                deadline,
            },
            include_str!("start.surql"),
        )
        .await
    }

    pub async fn workspace_run_context(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        run: WorkspaceRunId,
    ) -> Result<WorkspaceRunContext> {
        self.workspace_query(
            authority,
            Target {
                chat: chat.record_id(),
                target: run.record_id(),
            },
            include_str!("context.surql"),
        )
        .await
    }

    pub async fn claim_workspace_run(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        run: WorkspaceRunId,
        fence: Uuid,
    ) -> Result<WorkspaceRun> {
        self.workspace_query(
            authority,
            Claim {
                chat: chat.record_id(),
                run: run.record_id(),
                fence,
            },
            include_str!("claim.surql"),
        )
        .await
    }

    pub async fn update_workspace_run(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        run: WorkspaceRunId,
        update: WorkspaceRunUpdate,
    ) -> Result<WorkspaceRun> {
        if update.text.len() > 32768 || update.text.contains('\0') || update.feedback.operations > 8
        {
            return Err(WorkspaceError::Invalid("agent output"));
        }
        if !matches!(
            update.state,
            WorkspaceRunState::Running | WorkspaceRunState::Completed | WorkspaceRunState::Failed
        ) || (update.state == WorkspaceRunState::Failed) != update.failure.is_some()
        {
            return Err(WorkspaceError::Invalid("run transition"));
        }
        self.workspace_query(
            authority,
            Update {
                chat: chat.record_id(),
                run: run.record_id(),
                update,
            },
            include_str!("update.surql"),
        )
        .await
    }

    pub async fn cancel_workspace_run(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        run: WorkspaceRunId,
    ) -> Result<WorkspaceRun> {
        self.workspace_query(
            authority,
            Target {
                chat: chat.record_id(),
                target: run.record_id(),
            },
            include_str!("cancel.surql"),
        )
        .await
    }

    /// The bounded run window is independent from human-message pagination.
    /// Streaming text is replaced by run identity, never appended as new messages.
    pub async fn workspace_runs(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
    ) -> Result<Vec<WorkspaceRun>> {
        self.workspace_query(
            authority,
            chat.record_id(),
            concat!(include_str!("reconcile.surql"), include_str!("list.surql")),
        )
        .await
    }
}

#[derive(Clone, SurrealValue)]
struct Reject {
    chat: RecordId,
    run: RecordId,
    failure: WorkspaceRunFailure,
}

impl PlatformStore {
    /// Reject only an unclaimed admission. A racing worker or cancellation wins
    /// by changing state; this path cannot overwrite any accepted claim.
    pub async fn reject_workspace_run(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        run: WorkspaceRunId,
        failure: WorkspaceRunFailure,
    ) -> Result<WorkspaceRun> {
        self.workspace_query(
            authority,
            Reject {
                chat: chat.record_id(),
                run: run.record_id(),
                failure,
            },
            include_str!("reject.surql"),
        )
        .await
    }
}
