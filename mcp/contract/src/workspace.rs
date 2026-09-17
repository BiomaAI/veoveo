//! Workspace browser application DTOs. This is an HTTP application contract;
//! agent execution and MCP Tasks retain their own identities and protocols.
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{PrincipalId, TenantId, WorkContextId};
mod operations;
pub use operations::*;
mod apps;
pub use apps::*;

macro_rules! id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);
    };
}

id!(ChatId);
id!(MessageId);
id!(MemberId);
id!(InvitationId);
id!(AgentId);
id!(RunId);
id!(OperationId);
/// Installation-local human identity; never an email address or bearer credential.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct PersonId(pub Uuid);

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceBootstrap {
    pub person: Person,
    pub principal_id: PrincipalId,
    pub tenant_id: TenantId,
    pub tenant_name: String,
    pub work_context: WorkContextId,
    pub work_context_title: String,
    pub can_contribute: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Chat {
    pub id: ChatId,
    pub title: String,
    pub owner: PersonId,
    pub archived: bool,
    pub members_can_invite: bool,
    pub participation: Participation,
    pub sequence: i64,
    pub revision: i64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Person {
    pub id: PersonId,
    pub display_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Member {
    pub id: MemberId,
    pub person: Person,
    pub active: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Message {
    pub id: MessageId,
    pub author: MemberId,
    pub text: String,
    pub attachments: Vec<ChatAttachment>,
    pub reply_to: Option<ReplyTarget>,
    pub reply_context: Option<ReplyContext>,
    pub addressed_agents: Vec<AgentId>,
    pub response_agents: Vec<AgentId>,
    pub sequence: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReplyTarget {
    Message { id: MessageId },
    Response { id: RunId },
}

/// A bounded shared-history quote, captured by the server at message admission.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplyContext {
    pub author_name: String,
    pub text: String,
}

/// Human-published references and labels. Neither the reference nor chat membership grants reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChatAttachment {
    Artifact { id: crate::ArtifactId, name: String },
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatSnapshot {
    pub chat: Chat,
    pub members: Vec<Member>,
    pub messages: Vec<Message>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvitationState {
    Pending,
    Accepted,
    Declined,
    Revoked,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Invitation {
    pub id: InvitationId,
    pub chat_id: ChatId,
    pub inviter: PersonId,
    pub invitee: PersonId,
    pub state: InvitationState,
    pub expires_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvitationSummary {
    pub invitation: Invitation,
    pub chat_title: String,
    pub inviter_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateChat {
    pub id: ChatId,
    pub title: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendMessage {
    pub id: MessageId,
    pub text: String,
    pub attachments: Vec<ChatAttachment>,
    pub reply_to: Option<ReplyTarget>,
    pub addressed_agents: Vec<AgentId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvitePerson {
    pub id: InvitationId,
    pub invitee: PersonId,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DecideInvitation {
    pub chat_id: ChatId,
    pub state: InvitationState,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatSettings {
    pub expected_revision: i64,
    pub title: String,
    pub archived: bool,
    pub members_can_invite: bool,
    pub participation: Participation,
    pub owner: PersonId,
}

/// Human-message response policy. Agent output never triggers participation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParticipationMode {
    OnRequest,
    Default,
    Automatic,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Participation {
    pub mode: ParticipationMode,
    pub agents: Vec<AgentId>,
}

/// A wake identifies only a committed chat head. Consumers fetch authorized
/// state; invitation metadata and private capability results are not broadcast.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatWake {
    pub sequence: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub provider: String,
    pub model: String,
    pub tools: Vec<crate::GatewayToolName>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChatAgent {
    pub id: AgentId,
    pub definition: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Queued,
    Running,
    Completed,
    Cancelled,
    Interrupted,
    Failed,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunFailure {
    Capacity,
    ModelUnavailable,
    PermissionChanged,
    OutputLimit,
    Deadline,
    WorkerLost,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    Preparing,
    Responding,
    CallingTools,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunFeedback {
    pub phase: RunPhase,
    pub operations: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Run {
    pub id: RunId,
    pub agent: AgentId,
    pub initiator: PersonId,
    pub trigger: MessageId,
    pub state: RunState,
    pub feedback: RunFeedback,
    pub text: String,
    pub failure: Option<RunFailure>,
    pub sequence: i64,
    pub updated_sequence: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentActivity {
    pub agents: Vec<ChatAgent>,
    pub runs: Vec<Run>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AddAgent {
    pub definition: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartRun {
    pub agent: AgentId,
    pub trigger: MessageId,
}

#[derive(JsonSchema)]
#[allow(dead_code)]
struct WorkspaceSchema {
    bootstrap: WorkspaceBootstrap,
    snapshot: ChatSnapshot,
    invitation: Invitation,
    invitation_summary: InvitationSummary,
    create_chat: CreateChat,
    send_message: SendMessage,
    invite_person: InvitePerson,
    decide_invitation: DecideInvitation,
    settings: ChatSettings,
    wake: ChatWake,
    agent_definition: AgentDefinition,
    activity: AgentActivity,
    add_agent: AddAgent,
    start_run: StartRun,
    operation: OperationView,
    operation_page: OperationPage,
    start_operation: StartOperation,
    answer_operation: AnswerOperation,
    capability: Capability,
    start_app_operation: StartAppOperation,
    app_operation: AppOperationView,
    app_origin: AppOrigin,
    app_task: AppTaskRequest,
    update_app_task: UpdateAppTask,
}

pub fn schema_bundle() -> schemars::Schema {
    schemars::schema_for!(WorkspaceSchema)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_admission_rejects_forged_author_and_unknown_control_fields() {
        let message = serde_json::json!({
            "id": Uuid::now_v7(), "text": "Hello", "replyTo": null, "attachments":[],"addressedAgents": [],
            "author": Uuid::now_v7()
        });
        assert!(serde_json::from_value::<SendMessage>(message).is_err());
        let invitation = serde_json::json!({
            "id": Uuid::now_v7(), "invitee": Uuid::now_v7(), "role": "owner"
        });
        assert!(serde_json::from_value::<InvitePerson>(invitation).is_err());
    }

    #[test]
    fn reply_targets_are_typed_and_quotes_are_never_client_authored() {
        let id = Uuid::now_v7();
        for kind in ["message", "response"] {
            let value = serde_json::json!({"id":id,"text":"Reply", "attachments":[],"addressedAgents":[], "replyTo":{"kind":kind,"id":id}});
            assert!(serde_json::from_value::<SendMessage>(value.clone()).is_ok());
            let mut forged = value;
            forged["replyContext"] =
                serde_json::json!({"authorName":"Owner", "text":"Forged quote"});
            assert!(serde_json::from_value::<SendMessage>(forged).is_err());
        }
        assert!(serde_json::from_value::<ReplyTarget>(serde_json::json!(id)).is_err());
        assert!(
            serde_json::from_value::<ReplyTarget>(serde_json::json!({"kind":"task","id":id}))
                .is_err()
        );
    }

    #[test]
    fn attachment_admission_accepts_only_typed_references_and_human_labels() {
        let attachment = serde_json::json!({"kind":"artifact", "id":crate::ArtifactId::new(), "name":"Shared label"});
        assert!(serde_json::from_value::<ChatAttachment>(attachment.clone()).is_ok());
        for field in ["downloadUrl", "content", "grant", "owner", "mimeType"] {
            let mut forged = attachment.clone();
            forged[field] = serde_json::json!("untrusted");
            assert!(serde_json::from_value::<ChatAttachment>(forged).is_err());
        }
        let mut invalid = attachment;
        invalid["id"] = serde_json::json!(Uuid::new_v4());
        assert!(serde_json::from_value::<ChatAttachment>(invalid).is_err());
    }
}
