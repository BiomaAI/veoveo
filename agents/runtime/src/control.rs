//! Authenticated external control over one registered agent's durable inbox.
//!
//! The gateway supplies actor and Work Context authority. This module resolves
//! the target inside that exact tenant/context/profile tuple, then commits the
//! wake and outbox edge atomically. It never acquires the scheduler lease.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use uuid::Uuid;
use veoveo_platform_store::{
    AgentInputRequestId, AgentInputRequestRecord, AgentInputRequestState, AgentRecord, AgentState,
    OpenObject, OutboxDraft, PlatformStore, StoreAuthLevel, WakeId, WakeKind, WakeRecord,
    WakeState, deterministic_tenant_id, deterministic_work_context_id,
};

use crate::{AgentRuntimeError, InputRequestAnswer, Result, object, uuid_from_record};

const EVENT_SCHEMA_VERSION: i64 = 1;
const MAX_OPERATOR_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_ACTOR_ID_BYTES: usize = 2_048;

#[derive(Clone)]
pub struct AgentControl {
    store: PlatformStore,
}

#[derive(Clone, Debug)]
pub struct AgentControlTarget {
    pub tenant_key: String,
    pub work_context_key: String,
    pub profile: String,
    pub agent_key: String,
}

#[derive(Clone, Debug)]
pub struct OperatorMessageDraft {
    pub request_id: Uuid,
    pub message: String,
    pub actor_id: String,
}

#[derive(Clone, Debug)]
pub struct InputRequestDecisionDraft {
    pub request_id: Uuid,
    pub input_request_id: AgentInputRequestId,
    pub answer: InputRequestAnswer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentControlReceipt {
    pub request_id: Uuid,
    pub wake_id: WakeId,
    pub agent_key: String,
    pub work_context_key: String,
    pub accepted_at: DateTime<Utc>,
}

#[derive(Clone, Debug)]
pub struct GovernedInputRequest {
    pub input_request_id: AgentInputRequestId,
    pub message: String,
    pub requested_schema: Option<OpenObject>,
    pub requested_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct ExternalWakeContent {
    tenant: RecordId,
    agent: RecordId,
    kind: WakeKind,
    state: WakeState,
    dedupe_key: Option<String>,
    payload: OpenObject,
    available_at: DateTime<Utc>,
    claimed_by: Option<String>,
    claimed_at: Option<DateTime<Utc>>,
    claim_expires_at: Option<DateTime<Utc>>,
    claim_fence: Option<i64>,
    attempts: i64,
    acked_at: Option<DateTime<Utc>>,
    acked_by_episode: Option<RecordId>,
    last_error: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    revision: i64,
    coalesced_into: Option<RecordId>,
}

impl AgentControl {
    pub fn new(store: PlatformStore) -> Result<Self> {
        if store.config().auth_level() != StoreAuthLevel::Database {
            return Err(AgentRuntimeError::DatabaseCredentialsRequired);
        }
        Ok(Self { store })
    }

    pub async fn send_operator_message(
        &self,
        target: &AgentControlTarget,
        draft: OperatorMessageDraft,
    ) -> Result<AgentControlReceipt> {
        validate_request_id(draft.request_id)?;
        validate_message(&draft.message)?;
        validate_actor(&draft.actor_id)?;
        let agent = self.resolve_target(target).await?;
        let now = Utc::now();
        let wake_id = WakeId::from_uuid(draft.request_id);
        let payload = object([
            ("request_id".to_owned(), serde_json::json!(draft.request_id)),
            ("text".to_owned(), serde_json::json!(draft.message)),
            ("actor_id".to_owned(), serde_json::json!(draft.actor_id)),
            (
                "work_context".to_owned(),
                serde_json::json!(target.work_context_key),
            ),
        ]);
        let content = wake_content(
            &agent,
            WakeKind::OperatorMessage,
            format!("operator:{}", draft.request_id),
            payload,
            now,
        );
        let event = OutboxDraft::now(
            Some(agent.tenant.clone()),
            "wake",
            wake_id.to_string(),
            "wake.operator_message_enqueued",
            EVENT_SCHEMA_VERSION,
            object([
                ("agent_key".to_owned(), serde_json::json!(agent.agent_key)),
                ("actor_id".to_owned(), serde_json::json!(draft.actor_id)),
                (
                    "work_context".to_owned(),
                    serde_json::json!(target.work_context_key),
                ),
            ]),
        );
        let accepted_at = self
            .create_wake_idempotently(wake_id, &content, event)
            .await?;
        Ok(receipt(target, wake_id, draft.request_id, accepted_at))
    }

    pub async fn pending_input_requests(
        &self,
        target: &AgentControlTarget,
    ) -> Result<Vec<GovernedInputRequest>> {
        let agent = self.resolve_target(target).await?;
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM agent_input_request WHERE agent = $agent AND tenant = $tenant AND state = 'pending' ORDER BY requested_at ASC LIMIT 128;")
            .bind(("agent", agent.id.clone()))
            .bind(("tenant", agent.tenant.clone()))
            .await?
            .check()?;
        let records: Vec<AgentInputRequestRecord> = response.take(0)?;
        records
            .into_iter()
            .map(|record| {
                Ok(GovernedInputRequest {
                    input_request_id: AgentInputRequestId::from_uuid(uuid_from_record(
                        &record.id,
                        "agent_input_request.id",
                    )?),
                    message: record.message,
                    requested_schema: record.requested_schema,
                    requested_at: record.requested_at,
                })
            })
            .collect()
    }

    pub async fn decide_input_request(
        &self,
        target: &AgentControlTarget,
        draft: InputRequestDecisionDraft,
    ) -> Result<AgentControlReceipt> {
        validate_request_id(draft.request_id)?;
        validate_actor(&draft.answer.answered_by)?;
        validate_input_request_answer(&draft.answer)?;
        let agent = self.resolve_target(target).await?;
        let existing = self
            .input_request_for_agent(draft.input_request_id, &agent)
            .await?;
        if existing.state != AgentInputRequestState::Pending {
            return self
                .existing_input_request_receipt(target, draft, existing)
                .await;
        }

        let now = Utc::now();
        let wake_id = WakeId::from_uuid(draft.request_id);
        let payload = object([
            (
                "input_request_id".to_owned(),
                serde_json::json!(draft.input_request_id),
            ),
            ("phase".to_owned(), serde_json::json!("answered")),
            ("request_id".to_owned(), serde_json::json!(draft.request_id)),
            (
                "actor_id".to_owned(),
                serde_json::json!(draft.answer.answered_by),
            ),
            (
                "work_context".to_owned(),
                serde_json::json!(target.work_context_key),
            ),
        ]);
        let wake_content = wake_content(
            &agent,
            WakeKind::InputRequest,
            format!("input_request:{}:answered", draft.input_request_id),
            payload,
            now,
        );
        let event = OutboxDraft::now(
            Some(agent.tenant.clone()),
            "agent_input_request",
            draft.input_request_id.to_string(),
            "agent_input_request.answered",
            EVENT_SCHEMA_VERSION,
            object([
                ("wake_id".to_owned(), serde_json::json!(wake_id)),
                (
                    "actor_id".to_owned(),
                    serde_json::json!(draft.answer.answered_by),
                ),
                (
                    "work_context".to_owned(),
                    serde_json::json!(target.work_context_key),
                ),
            ]),
        );
        let result = self
            .store
            .client()
            .query("BEGIN TRANSACTION; LET $answered = (UPDATE ONLY $input_request SET state = $state, answer = $answer, answered_by = $answered_by, answered_at = $now, revision += 1 WHERE agent = $agent AND tenant = $tenant AND state = 'pending' AND revision = $revision RETURN AFTER); IF $answered = NONE { THROW 'agent input_request answer conflict'; }; CREATE ONLY $wake CONTENT $wake_content RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("input_request", draft.input_request_id.record_id()))
            .bind(("agent", agent.id.clone()))
            .bind(("tenant", agent.tenant.clone()))
            .bind(("state", draft.answer.state))
            .bind(("answer", draft.answer.answer.clone()))
            .bind(("answered_by", draft.answer.answered_by.clone()))
            .bind(("now", now))
            .bind(("revision", existing.revision))
            .bind(("wake", wake_id.record_id()))
            .bind(("wake_content", wake_content))
            .bind(("event", event))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            let current = self
                .input_request_for_agent(draft.input_request_id, &agent)
                .await?;
            if current.state == draft.answer.state
                && current.answer == draft.answer.answer
                && current.answered_by.as_deref() == Some(&draft.answer.answered_by)
            {
                return self
                    .existing_input_request_receipt(target, draft, current)
                    .await;
            }
            return Err(AgentRuntimeError::Database(error));
        }
        Ok(receipt(target, wake_id, draft.request_id, now))
    }

    async fn existing_input_request_receipt(
        &self,
        target: &AgentControlTarget,
        draft: InputRequestDecisionDraft,
        existing: AgentInputRequestRecord,
    ) -> Result<AgentControlReceipt> {
        if existing.state != draft.answer.state
            || existing.answer != draft.answer.answer
            || existing.answered_by.as_deref() != Some(&draft.answer.answered_by)
        {
            return Err(AgentRuntimeError::Conflict {
                entity: "agent_input_request",
            });
        }
        let wake_id = WakeId::from_uuid(draft.request_id);
        let wake = self
            .wake(wake_id)
            .await?
            .ok_or(AgentRuntimeError::Conflict {
                entity: "agent_input_request",
            })?;
        let expected_dedupe_key = format!("input_request:{}:answered", draft.input_request_id);
        if wake.agent != existing.agent
            || wake.kind != WakeKind::InputRequest
            || wake.dedupe_key.as_deref() != Some(expected_dedupe_key.as_str())
        {
            return Err(AgentRuntimeError::Conflict {
                entity: "agent_input_request",
            });
        }
        Ok(receipt(
            target,
            wake_id,
            draft.request_id,
            existing.answered_at.unwrap_or(existing.requested_at),
        ))
    }

    async fn resolve_target(&self, target: &AgentControlTarget) -> Result<AgentRecord> {
        if target.agent_key.trim().is_empty() || target.profile.trim().is_empty() {
            return Err(AgentRuntimeError::InvalidField {
                field: "agent control target",
                reason: "agent_key and profile must not be empty".to_owned(),
            });
        }
        let tenant = deterministic_tenant_id(&target.tenant_key)?.record_id();
        let work_context =
            deterministic_work_context_id(&target.tenant_key, &target.work_context_key)?
                .record_id();
        let profile = RecordId::new("profile", target.profile.clone());
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM agent WHERE tenant = $tenant AND work_context = $work_context AND profile = $profile AND agent_key = $agent_key LIMIT 2;")
            .bind(("tenant", tenant))
            .bind(("work_context", work_context))
            .bind(("profile", profile))
            .bind(("agent_key", target.agent_key.clone()))
            .await?
            .check()?;
        let mut records: Vec<AgentRecord> = response.take(0)?;
        if records.len() != 1 {
            return Err(AgentRuntimeError::NotFound { entity: "agent" });
        }
        let agent = records.pop().expect("length checked");
        if agent.state == AgentState::Disabled {
            return Err(AgentRuntimeError::Conflict { entity: "agent" });
        }
        Ok(agent)
    }

    async fn input_request_for_agent(
        &self,
        input_request_id: AgentInputRequestId,
        agent: &AgentRecord,
    ) -> Result<AgentInputRequestRecord> {
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM ONLY $input_request WHERE agent = $agent AND tenant = $tenant;")
            .bind(("input_request", input_request_id.record_id()))
            .bind(("agent", agent.id.clone()))
            .bind(("tenant", agent.tenant.clone()))
            .await?
            .check()?;
        response
            .take::<Option<AgentInputRequestRecord>>(0)?
            .ok_or(AgentRuntimeError::NotFound {
                entity: "agent_input_request",
            })
    }

    async fn create_wake_idempotently(
        &self,
        wake_id: WakeId,
        content: &ExternalWakeContent,
        event: OutboxDraft,
    ) -> Result<DateTime<Utc>> {
        let result = self
            .store
            .client()
            .query("BEGIN TRANSACTION; CREATE ONLY $wake CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("wake", wake_id.record_id()))
            .bind(("content", content.clone()))
            .bind(("event", event))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            let Some(existing) = self.wake(wake_id).await? else {
                return Err(AgentRuntimeError::Database(error));
            };
            if !same_wake(&existing, content) {
                return Err(AgentRuntimeError::Conflict { entity: "wake" });
            }
            return Ok(existing.created_at);
        }
        Ok(content.created_at)
    }

    async fn wake(&self, wake_id: WakeId) -> Result<Option<WakeRecord>> {
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM ONLY $wake;")
            .bind(("wake", wake_id.record_id()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
}

fn wake_content(
    agent: &AgentRecord,
    kind: WakeKind,
    dedupe_key: String,
    payload: OpenObject,
    now: DateTime<Utc>,
) -> ExternalWakeContent {
    ExternalWakeContent {
        tenant: agent.tenant.clone(),
        agent: agent.id.clone(),
        kind,
        state: WakeState::Pending,
        dedupe_key: Some(dedupe_key),
        payload,
        available_at: now,
        claimed_by: None,
        claimed_at: None,
        claim_expires_at: None,
        claim_fence: None,
        attempts: 0,
        acked_at: None,
        acked_by_episode: None,
        last_error: None,
        created_at: now,
        updated_at: now,
        revision: 0,
        coalesced_into: None,
    }
}

fn same_wake(record: &WakeRecord, expected: &ExternalWakeContent) -> bool {
    record.tenant == expected.tenant
        && record.agent == expected.agent
        && record.kind == expected.kind
        && record.dedupe_key == expected.dedupe_key
        && record.payload == expected.payload
}

fn receipt(
    target: &AgentControlTarget,
    wake_id: WakeId,
    request_id: Uuid,
    accepted_at: DateTime<Utc>,
) -> AgentControlReceipt {
    AgentControlReceipt {
        request_id,
        wake_id,
        agent_key: target.agent_key.clone(),
        work_context_key: target.work_context_key.clone(),
        accepted_at,
    }
}

fn validate_request_id(request_id: Uuid) -> Result<()> {
    if request_id.get_version_num() != 7 {
        return Err(AgentRuntimeError::InvalidField {
            field: "request_id",
            reason: "must be UUIDv7".to_owned(),
        });
    }
    Ok(())
}

fn validate_message(message: &str) -> Result<()> {
    if message.trim().is_empty()
        || message.len() > MAX_OPERATOR_MESSAGE_BYTES
        || message
            .chars()
            .any(|value| value.is_control() && !matches!(value, '\n' | '\r' | '\t'))
    {
        return Err(AgentRuntimeError::InvalidField {
            field: "message",
            reason: format!(
                "must contain 1..={MAX_OPERATOR_MESSAGE_BYTES} UTF-8 bytes and no binary controls"
            ),
        });
    }
    Ok(())
}

fn validate_actor(actor_id: &str) -> Result<()> {
    if actor_id.trim().is_empty()
        || actor_id.len() > MAX_ACTOR_ID_BYTES
        || actor_id.chars().any(char::is_control)
    {
        return Err(AgentRuntimeError::InvalidField {
            field: "actor_id",
            reason: "must be a bounded non-empty principal id".to_owned(),
        });
    }
    Ok(())
}

fn validate_input_request_answer(answer: &InputRequestAnswer) -> Result<()> {
    let valid = match answer.state {
        AgentInputRequestState::Answered => answer.answer.is_some(),
        AgentInputRequestState::Declined | AgentInputRequestState::Cancelled => {
            answer.answer.is_none()
        }
        AgentInputRequestState::Pending => false,
    };
    if !valid {
        return Err(AgentRuntimeError::InvalidField {
            field: "input_request answer",
            reason: "state and content do not form a terminal answer".to_owned(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_messages_are_bounded_and_request_ids_are_v7() {
        assert!(validate_request_id(Uuid::now_v7()).is_ok());
        assert!(validate_request_id(Uuid::new_v4()).is_err());
        assert!(validate_message("inspect the active route").is_ok());
        assert!(validate_message("").is_err());
        assert!(validate_message("bad\u{0}input").is_err());
        assert!(validate_message(&"x".repeat(MAX_OPERATOR_MESSAGE_BYTES + 1)).is_err());
    }
}
