//! Authenticated operator control messages for continuously scheduled agents.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentOperatorMessageRequest {
    /// Client-generated UUIDv7 used as the durable retry identity.
    pub request_id: Uuid,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentInputRequestDecision {
    Accept {
        request_id: Uuid,
        #[serde(default)]
        content: Value,
    },
    Decline {
        request_id: Uuid,
    },
    Cancel {
        request_id: Uuid,
    },
}

impl AgentInputRequestDecision {
    pub const fn request_id(&self) -> Uuid {
        match self {
            Self::Accept { request_id, .. }
            | Self::Decline { request_id }
            | Self::Cancel { request_id } => *request_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentWakeReceipt {
    pub request_id: Uuid,
    pub wake_id: Uuid,
    pub agent_id: String,
    pub work_context: String,
    pub accepted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentInputRequestView {
    pub input_request_id: Uuid,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_schema: Option<Value>,
    pub requested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentConversationRole {
    Operator,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentConversationEntryState {
    Accepted,
    Running,
    Completed,
    BudgetTerminated,
    Failed,
}

/// One actor-attributed projection of durable agent runtime state.
///
/// Conversation entries are not a second source of truth. Operator entries
/// project durable wakes and agent entries project durable episodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConversationEntry {
    pub entry_id: String,
    pub role: AgentConversationRole,
    pub actor_id: String,
    pub content: String,
    pub state: AgentConversationEntryState,
    pub occurred_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wake_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub in_reply_to_request_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentConversationView {
    pub agent_id: String,
    pub entries: Vec<AgentConversationEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_contract_rejects_authority_injection() {
        let value = serde_json::json!({
            "request_id": Uuid::now_v7(),
            "message": "inspect the active route",
            "authority": "self_granted"
        });
        assert!(serde_json::from_value::<AgentOperatorMessageRequest>(value).is_err());
    }

    #[test]
    fn input_request_decisions_are_closed_and_tagged() {
        let value = serde_json::json!({
            "request_id": Uuid::now_v7(),
            "action": "accept",
            "content": {"approved": true}
        });
        let request: AgentInputRequestDecision =
            serde_json::from_value(value).expect("decision parses");
        assert!(matches!(request, AgentInputRequestDecision::Accept { .. }));
    }

    #[test]
    fn conversation_contract_contains_no_domain_fields() {
        let value = serde_json::to_value(AgentConversationEntry {
            entry_id: "wake:019f0000-0000-7000-8000-000000000001".to_owned(),
            role: AgentConversationRole::Operator,
            actor_id: "https://idp.example#operator".to_owned(),
            content: "inspect the active route".to_owned(),
            state: AgentConversationEntryState::Accepted,
            occurred_at: Utc::now(),
            request_id: Some(Uuid::now_v7()),
            wake_id: Some(Uuid::now_v7()),
            episode_id: None,
            in_reply_to_request_ids: Vec::new(),
        })
        .expect("conversation entry serializes");
        assert!(value.get("vehicle_id").is_none());
        assert!(value.get("mission_id").is_none());
        assert!(value.get("fleet_id").is_none());
    }
}
