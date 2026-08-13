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
}
