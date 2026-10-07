//! Authenticated operator control messages for continuously scheduled agents.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentOperatorMessageRequest {
    /// Client-generated UUIDv7 used as the durable retry identity.
    pub request_id: Uuid,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "action",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentWakeReceipt {
    pub request_id: Uuid,
    pub wake_id: Uuid,
    pub agent_id: String,
    pub work_context: String,
    pub accepted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentInputRequestView {
    pub input_request_id: Uuid,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_schema: Option<Value>,
    pub requested_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum AgentConversationRole {
    Operator,
    Agent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum AgentConversationEntryState {
    Accepted,
    Running,
    Completed,
    BudgetTerminated,
    Stopped,
    Failed,
}

/// One actor-attributed projection of durable agent runtime state.
///
/// Conversation entries are not a second source of truth. Operator entries
/// project durable wakes and agent entries project durable episodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentConversationView {
    pub agent_id: String,
    pub entries: Vec<AgentConversationEntry>,
}

/// Source-owner operator-control contracts consumed by browser clients.
#[derive(JsonSchema)]
#[expect(dead_code, reason = "schema-only bundle selects owner contracts")]
struct AgentControlSchema {
    message: AgentOperatorMessageRequest,
    decision: AgentInputRequestDecision,
    receipt: AgentWakeReceipt,
    #[serde(rename = "inputRequest")]
    input_request: AgentInputRequestView,
    conversation: AgentConversationView,
}

pub fn schema_bundle() -> schemars::Schema {
    schemars::schema_for!(AgentControlSchema)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_contract_rejects_authority_injection() {
        let value = serde_json::json!({
            "requestId": Uuid::now_v7(),
            "message": "inspect the active route",
            "authority": "self_granted"
        });
        assert!(serde_json::from_value::<AgentOperatorMessageRequest>(value).is_err());
    }

    #[test]
    fn input_request_decisions_are_closed_and_tagged() {
        let value = serde_json::json!({
            "requestId": Uuid::now_v7(),
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
    #[test]
    fn control_wire_producers_refuse_retired_fields_in_both_decoders() {
        fn refuse<T: Serialize + serde::de::DeserializeOwned>(typed: T, fields: &[(&str, &str)]) {
            let current = serde_json::to_value(typed).unwrap();
            serde_json::from_value::<T>(current.clone()).unwrap_or_else(|e| panic!("current: {e}"));
            serde_json::from_slice::<T>(&serde_json::to_vec(&current).unwrap())
                .unwrap_or_else(|e| panic!("current JSON: {e}"));
            for (key, retired) in fields {
                for keep in [false, true] {
                    let mut bad = current.clone();
                    let object = bad.as_object_mut().unwrap();
                    object.insert((*retired).into(), object[*key].clone());
                    if !keep {
                        object.remove(*key);
                    }
                    assert!(
                        serde_json::from_value::<T>(bad.clone()).is_err(),
                        "{retired} mixed={keep}"
                    );
                    assert!(
                        serde_json::from_slice::<T>(&serde_json::to_vec(&bad).unwrap()).is_err(),
                        "JSON {retired} mixed={keep}"
                    );
                }
            }
        }
        let id = Uuid::now_v7();
        refuse(
            AgentOperatorMessageRequest {
                request_id: id,
                message: "inspect".into(),
            },
            &[("requestId", "request_id")],
        );
        for decision in [
            AgentInputRequestDecision::Accept {
                request_id: id,
                content: serde_json::json!({"provider_field":true}),
            },
            AgentInputRequestDecision::Decline { request_id: id },
            AgentInputRequestDecision::Cancel { request_id: id },
        ] {
            refuse(decision, &[("requestId", "request_id")]);
        }
        refuse(
            AgentWakeReceipt {
                request_id: id,
                wake_id: id,
                agent_id: "pilot".into(),
                work_context: "operations".into(),
                accepted_at: Utc::now(),
            },
            &[
                ("requestId", "request_id"),
                ("wakeId", "wake_id"),
                ("agentId", "agent_id"),
                ("workContext", "work_context"),
                ("acceptedAt", "accepted_at"),
            ],
        );
        refuse(
            AgentInputRequestView {
                input_request_id: id,
                message: "approve".into(),
                requested_schema: Some(serde_json::json!({"provider_field":true})),
                requested_at: Utc::now(),
            },
            &[
                ("inputRequestId", "input_request_id"),
                ("requestedSchema", "requested_schema"),
                ("requestedAt", "requested_at"),
            ],
        );
        let entry = AgentConversationEntry {
            entry_id: "episode:fixture".into(),
            role: AgentConversationRole::Agent,
            actor_id: "pilot".into(),
            content: "response".into(),
            state: AgentConversationEntryState::Completed,
            occurred_at: Utc::now(),
            request_id: Some(id),
            wake_id: Some(id),
            episode_id: Some(id),
            in_reply_to_request_ids: vec![id],
        };
        refuse(
            entry.clone(),
            &[
                ("entryId", "entry_id"),
                ("actorId", "actor_id"),
                ("occurredAt", "occurred_at"),
                ("requestId", "request_id"),
                ("wakeId", "wake_id"),
                ("episodeId", "episode_id"),
                ("inReplyToRequestIds", "in_reply_to_request_ids"),
            ],
        );
        let current = serde_json::to_value(AgentConversationView {
            agent_id: "pilot".into(),
            entries: vec![entry],
        })
        .unwrap();
        for keep in [false, true] {
            let mut bad = current.clone();
            let object = bad["entries"][0].as_object_mut().unwrap();
            object.insert("entry_id".into(), object["entryId"].clone());
            if !keep {
                object.remove("entryId");
            }
            assert!(serde_json::from_value::<AgentConversationView>(bad.clone()).is_err());
            assert!(
                serde_json::from_slice::<AgentConversationView>(&serde_json::to_vec(&bad).unwrap())
                    .is_err()
            );
        }
    }
}
