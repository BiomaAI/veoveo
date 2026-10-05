use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types as surrealdb_types;
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceAgent {
    pub id: RecordId,
    pub chat: RecordId,
    pub definition: String,
    pub definition_digest: String,
    pub display_name: String,
    pub provider: String,
    pub model: String,
    pub active: bool,
    pub joined_at: DateTime<Utc>,
}

impl SurrealValue for WorkspaceAgent {
    fn kind_of() -> surrealdb::types::Kind {
        surrealdb::types::Kind::Object
    }
    fn into_value(self) -> surrealdb::types::Value {
        let mut fields = surrealdb::types::Object::new();
        fields.insert("id", self.id.into_value());
        fields.insert("chat", self.chat.into_value());
        fields.insert("definition", self.definition.into_value());
        fields.insert("definition_digest", self.definition_digest.into_value());
        fields.insert("display_name", self.display_name.into_value());
        fields.insert("provider", self.provider.into_value());
        fields.insert("model", self.model.into_value());
        fields.insert("active", self.active.into_value());
        fields.insert("joined_at", self.joined_at.into_value());
        surrealdb::types::Value::Object(fields)
    }
    fn from_value(value: surrealdb::types::Value) -> Result<Self, surrealdb::types::Error> {
        let invalid =
            || surrealdb::types::Error::internal("invalid stored WorkspaceAgent snapshot".into());
        let surrealdb::types::Value::Object(mut fields) = value else {
            return Err(invalid());
        };
        if fields.len() != 9 {
            return Err(invalid());
        }
        let row = Self {
            id: RecordId::from_value(fields.remove("id").ok_or_else(invalid)?)?,
            chat: RecordId::from_value(fields.remove("chat").ok_or_else(invalid)?)?,
            definition: String::from_value(fields.remove("definition").ok_or_else(invalid)?)?,
            definition_digest: String::from_value(
                fields.remove("definition_digest").ok_or_else(invalid)?,
            )?,
            display_name: String::from_value(fields.remove("display_name").ok_or_else(invalid)?)?,
            provider: String::from_value(fields.remove("provider").ok_or_else(invalid)?)?,
            model: String::from_value(fields.remove("model").ok_or_else(invalid)?)?,
            active: bool::from_value(fields.remove("active").ok_or_else(invalid)?)?,
            joined_at: DateTime::<Utc>::from_value(
                fields.remove("joined_at").ok_or_else(invalid)?,
            )?,
        };
        row.validate_snapshot().map_err(|_| invalid())?;
        Ok(row)
    }
}
impl WorkspaceAgent {
    pub(super) fn validate_snapshot(&self) -> super::super::Result<()> {
        use surrealdb::types::RecordIdKey;
        if self.id.table.as_str() != "workspace_agent"
            || self.chat.table.as_str() != "workspace_chat"
        {
            return Err(super::super::WorkspaceError::Invalid(
                "agent snapshot identity",
            ));
        }
        let RecordIdKey::Uuid(chat) = &self.chat.key else {
            return Err(super::super::WorkspaceError::Invalid(
                "chat snapshot identity",
            ));
        };
        let chat = Uuid::parse_str(&chat.to_string())
            .map_err(|_| super::super::WorkspaceError::Invalid("chat snapshot identity"))?;
        let expected = crate::persistence::WorkspaceAgentId::from_uuid(Uuid::new_v5(
            &chat,
            self.definition.as_bytes(),
        ))
        .record_id();
        if self.id != expected {
            return Err(super::super::WorkspaceError::Invalid(
                "agent snapshot relationship",
            ));
        }
        for (value, field, bound) in [
            (&self.definition, "agent definition", 128),
            (&self.display_name, "agent name", 200),
            (&self.provider, "provider", 200),
            (&self.model, "model", 256),
        ] {
            super::super::validate_text(value, field, bound)?;
        }
        if self.definition_digest.len() != 64
            || !self
                .definition_digest
                .bytes()
                .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
        {
            return Err(super::super::WorkspaceError::Invalid(
                "agent snapshot digest",
            ));
        }
        Ok(())
    }
    pub(super) fn check_receipt(
        &self,
        chat: RecordId,
        agent: RecordId,
        definition: &str,
        digest: &str,
    ) -> super::super::Result<()> {
        self.validate_snapshot()?;
        if self.chat != chat
            || self.id != agent
            || self.definition != definition
            || self.definition_digest != digest
        {
            return Err(super::super::WorkspaceError::Invalid(
                "agent receipt relationship",
            ));
        }
        Ok(())
    }
}

/// Server-validated catalog admission, never a browser-submitted model config.
#[derive(Clone, Serialize, SurrealValue)]
pub struct WorkspaceAgentAdmission {
    pub definition: String,
    pub definition_digest: String,
    pub display_name: String,
    pub provider: String,
    pub model: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
pub enum WorkspaceRunState {
    #[surreal(value = "queued")]
    Queued,
    #[surreal(value = "running")]
    Running,
    #[surreal(value = "completed")]
    Completed,
    #[surreal(value = "cancelled")]
    Cancelled,
    #[surreal(value = "interrupted")]
    Interrupted,
    #[surreal(value = "failed")]
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
pub enum WorkspaceRunFailure {
    #[surreal(value = "capacity")]
    Capacity,
    #[surreal(value = "model_unavailable")]
    ModelUnavailable,
    #[surreal(value = "permission_changed")]
    PermissionChanged,
    #[surreal(value = "output_limit")]
    OutputLimit,
    #[surreal(value = "deadline")]
    Deadline,
    #[surreal(value = "worker_lost")]
    WorkerLost,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
#[serde(rename_all = "snake_case")]
#[surreal(untagged)]
pub enum WorkspaceRunPhase {
    #[default]
    #[surreal(value = "preparing")]
    Preparing,
    #[surreal(value = "responding")]
    Responding,
    #[surreal(value = "calling_tools")]
    CallingTools,
}

/// Shared execution facts contain no capability names, arguments or results.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceRunFeedback {
    pub phase: WorkspaceRunPhase,
    pub operations: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, SurrealValue)]
pub struct WorkspaceRun {
    pub id: RecordId,
    pub chat: RecordId,
    pub agent: RecordId,
    pub definition_digest: String,
    pub initiator: RecordId,
    pub trigger: RecordId,
    pub context_sequence: i64,
    pub sequence: i64,
    pub updated_sequence: i64,
    pub state: WorkspaceRunState,
    pub feedback: WorkspaceRunFeedback,
    pub text: String,
    pub failure: Option<WorkspaceRunFailure>,
    pub fence: Option<Uuid>,
    pub lease_until: DateTime<Utc>,
    pub deadline: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Provider-independent, irreversible publication/heartbeat request. The fence
/// comes from the successful claim, never from a public browser request.
#[derive(Clone, SurrealValue)]
pub struct WorkspaceRunUpdate {
    pub fence: Uuid,
    pub text: String,
    pub state: WorkspaceRunState,
    pub feedback: WorkspaceRunFeedback,
    pub failure: Option<WorkspaceRunFailure>,
}

/// Immutable prompt boundary. Human messages are immutable; only agent results
/// completed before admission are eligible. In-flight and later output is absent.
#[derive(Clone, Debug, PartialEq, SurrealValue)]
pub struct WorkspaceRunContext {
    pub run: WorkspaceRun,
    pub trigger: super::super::WorkspaceMessage,
    pub members: Vec<super::super::WorkspaceMember>,
    pub people: Vec<super::super::WorkspacePerson>,
    pub messages: Vec<super::super::WorkspaceMessage>,
    pub completed_runs: Vec<WorkspaceRun>,
    pub agents: Vec<WorkspaceAgent>,
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use crate::persistence::{WorkspaceAgentId, WorkspaceChatId};
    use surrealdb::types::Value;
    fn snapshot() -> WorkspaceAgent {
        let chat = WorkspaceChatId::new();
        let definition = "assistant";
        WorkspaceAgent {
            id: WorkspaceAgentId::from_uuid(Uuid::new_v5(&chat.as_uuid(), definition.as_bytes()))
                .record_id(),
            chat: chat.record_id(),
            definition: definition.into(),
            definition_digest: "a".repeat(64),
            display_name: "Assistant".into(),
            provider: "provider".into(),
            model: "model".into(),
            active: true,
            joined_at: Utc::now(),
        }
    }
    #[test]
    fn retained_snapshot_rejects_unknown_fields_invalid_identity_and_mismatched_replay() {
        let row = snapshot();
        assert_eq!(
            WorkspaceAgent::from_value(row.clone().into_value()).unwrap(),
            row
        );
        for extra in [Value::None, Value::Null, true.into_value()] {
            let Value::Object(mut fields) = row.clone().into_value() else {
                unreachable!()
            };
            fields.insert("extra", extra);
            assert!(WorkspaceAgent::from_value(Value::Object(fields)).is_err());
        }
        let invalid = WorkspaceAgent {
            id: WorkspaceAgentId::new().record_id(),
            ..row.clone()
        };
        assert!(WorkspaceAgent::from_value(invalid.into_value()).is_err());
        assert!(
            row.check_receipt(
                row.chat.clone(),
                row.id.clone(),
                &row.definition,
                &row.definition_digest
            )
            .is_ok()
        );
        assert!(
            row.check_receipt(
                WorkspaceChatId::new().record_id(),
                row.id.clone(),
                &row.definition,
                &row.definition_digest
            )
            .is_err()
        );
        assert!(
            row.check_receipt(
                row.chat.clone(),
                row.id.clone(),
                &row.definition,
                &"b".repeat(64)
            )
            .is_err()
        );
    }
}
