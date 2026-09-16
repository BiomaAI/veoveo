//! Owner response policy and atomic admission of a human message and its runs.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types as surrealdb_types;
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};
use uuid::Uuid;

use super::{
    Result, WorkspaceAttachment, WorkspaceAuthority, WorkspaceError, WorkspaceMessage,
    WorkspaceRun, human_member, validate_text,
};
use crate::{PlatformStore, WorkspaceAgentId, WorkspaceChatId, WorkspaceMessageId, WorkspaceRunId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
pub enum WorkspaceParticipationMode {
    #[default]
    #[surreal(value = "on_request")]
    OnRequest,
    #[surreal(value = "default")]
    Default,
    #[surreal(value = "automatic")]
    Automatic,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceParticipation {
    pub mode: WorkspaceParticipationMode,
    pub agents: Vec<RecordId>,
}

impl WorkspaceParticipation {
    pub(super) fn validate(&self) -> Result<()> {
        let valid = match self.mode {
            WorkspaceParticipationMode::OnRequest => self.agents.is_empty(),
            WorkspaceParticipationMode::Default => self.agents.len() == 1,
            WorkspaceParticipationMode::Automatic => (1..=4).contains(&self.agents.len()),
        };
        if !valid
            || self
                .agents
                .iter()
                .enumerate()
                .any(|(i, id)| self.agents[..i].contains(id))
        {
            return Err(WorkspaceError::Invalid("agent participation"));
        }
        Ok(())
    }
}

/// Trusted server deadline; explicit destinations come from the human's request.
pub struct WorkspaceTurnRequest {
    pub id: WorkspaceMessageId,
    pub text: String,
    pub attachments: Vec<WorkspaceAttachment>,
    pub reply_to: Option<WorkspaceReplyTarget>,
    pub addressed_agents: Vec<WorkspaceAgentId>,
    pub deadline: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceReplyTarget {
    Message(WorkspaceMessageId),
    Response(WorkspaceRunId),
}

impl WorkspaceReplyTarget {
    pub fn record_id(self) -> RecordId {
        match self {
            Self::Message(id) => id.record_id(),
            Self::Response(id) => id.record_id(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, SurrealValue)]
pub struct WorkspaceTurn {
    pub message: WorkspaceMessage,
    pub runs: Vec<WorkspaceRun>,
}

#[derive(Clone, SurrealValue)]
struct Candidate {
    agent: RecordId,
    run: RecordId,
}

#[derive(Clone, SurrealValue)]
struct Send {
    chat: RecordId,
    message: RecordId,
    member: RecordId,
    text: String,
    attachments: Vec<WorkspaceAttachment>,
    reply_to: Option<RecordId>,
    addressed_agents: Vec<RecordId>,
    candidates: Vec<Candidate>,
    deadline: DateTime<Utc>,
}

impl PlatformStore {
    pub async fn send_workspace_turn(
        &self,
        authority: &WorkspaceAuthority,
        chat: WorkspaceChatId,
        request: WorkspaceTurnRequest,
    ) -> Result<WorkspaceTurn> {
        if !request.text.is_empty() || request.attachments.is_empty() {
            validate_text(&request.text, "message", 32_768)?;
        }
        if request.attachments.len() > 8 {
            return Err(WorkspaceError::Invalid("attachments"));
        }
        for (index, attachment) in request.attachments.iter().enumerate() {
            validate_text(&attachment.name, "attachment name", 255)?;
            if attachment.artifact.as_uuid().get_version_num() != 7
                || attachment.name.chars().any(char::is_control)
                || request.attachments[..index]
                    .iter()
                    .any(|other| other.artifact == attachment.artifact)
            {
                return Err(WorkspaceError::Invalid("attachments"));
            }
        }
        if request.addressed_agents.len() > 4 {
            return Err(WorkspaceError::Invalid("agent destinations"));
        }
        let mut addressed_agents: Vec<_> = request
            .addressed_agents
            .into_iter()
            .map(WorkspaceAgentId::record_id)
            .collect();
        addressed_agents.sort();
        addressed_agents.dedup();
        // The transaction rechecks current policy and agent membership. These
        // deterministic keys are not authorization or a stale policy decision.
        let candidates = self
            .workspace_agents(authority, chat)
            .await?
            .into_iter()
            .map(|agent| {
                let RecordIdKey::Uuid(id) = &agent.id.key else {
                    return Err(WorkspaceError::Unavailable);
                };
                Ok(Candidate {
                    run: WorkspaceRunId::from_uuid(Uuid::new_v5(
                        &request.id.as_uuid(),
                        id.as_bytes(),
                    ))
                    .record_id(),
                    agent: agent.id,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.workspace_query(
            authority,
            Send {
                chat: chat.record_id(),
                message: request.id.record_id(),
                member: human_member(chat, &authority.principal).record_id(),
                text: request.text,
                attachments: request.attachments,
                reply_to: request.reply_to.map(WorkspaceReplyTarget::record_id),
                addressed_agents,
                candidates,
                deadline: request.deadline,
            },
            include_str!("queries/message.surql"),
        )
        .await
    }
}
