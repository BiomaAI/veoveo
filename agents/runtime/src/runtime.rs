//! Atomic durable operations for one autonomous agent replica.
//!
//! Lease, wake, episode, task, elicitation, and recovery mutations stay in one
//! module because they share the same fencing invariant and transaction state
//! machine. Contract types live in `types`; process and protocol wiring live in
//! their owning crates.

use std::str::FromStr;
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::{RecordId, SurrealValue};
use veoveo_platform_store::{
    AgentElicitationId, AgentElicitationRecord, AgentElicitationState, AgentEpisodeId,
    AgentEpisodeRecord, AgentEpisodeState, AgentId, AgentRecord, AgentState, AgentTaskId,
    AgentTaskRecord, AgentTaskWatchState, InvocationAuthorityRecord, OpenObject, OutboxDraft,
    PlatformIdentity, PlatformStore, PrincipalKind, StoreAuthLevel, TaskId, TaskRecord, WakeId,
    WakeKind, WakeRecord, WakeState, deterministic_work_context_id,
};
use veoveo_task_runtime::TaskRetentionPin;

use crate::types::{
    AgentInstanceId, AgentLease, AgentRuntimeError, AgentSpec, AgentTaskResult, ClaimedAgentTask,
    ClaimedWake, ElicitationAnswer, EpisodeCompletion, EpisodeHandle, NewAgentTask, NewElicitation,
    NewWake, ParkedElicitation, Result, checked_i64, object, task_id_from_record, uuid_from_record,
};

const EVENT_SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct AgentContent {
    tenant: RecordId,
    agent_key: String,
    display_name: String,
    profile: RecordId,
    work_context: RecordId,
    policy_revision: String,
    authority: InvocationAuthorityRecord,
    state: AgentState,
    manifest: OpenObject,
    memory_database: String,
    last_episode: Option<RecordId>,
    next_episode_sequence: i64,
    lease_owner: Option<String>,
    lease_expires_at: Option<DateTime<Utc>>,
    heartbeat_at: Option<DateTime<Utc>>,
    fence: i64,
    revision: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct WakeContent {
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

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct EpisodeContent {
    tenant: RecordId,
    agent: RecordId,
    sequence: i64,
    retention_pin: String,
    wake_note: String,
    state: AgentEpisodeState,
    final_output: Option<String>,
    summary: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    completion_calls: i64,
    tool_calls: i64,
    error: Option<String>,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    revision: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct AgentTaskContent {
    tenant: RecordId,
    agent: RecordId,
    task: RecordId,
    tool_name: String,
    descriptor: OpenObject,
    descriptor_complete: bool,
    state: AgentTaskWatchState,
    result: Option<OpenObject>,
    result_is_error: bool,
    result_wake: Option<RecordId>,
    retention_pin: String,
    retention_pin_active: bool,
    attempt_count: i64,
    next_retry_at: DateTime<Utc>,
    lease_owner: Option<String>,
    lease_expires_at: Option<DateTime<Utc>>,
    started_by_episode: RecordId,
    consumed_by_episode: Option<RecordId>,
    last_error: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    revision: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
struct ElicitationContent {
    tenant: RecordId,
    agent: RecordId,
    related_task: Option<RecordId>,
    message: String,
    requested_schema: Option<OpenObject>,
    state: AgentElicitationState,
    answer: Option<OpenObject>,
    answered_by: Option<String>,
    requested_at: DateTime<Utc>,
    answered_at: Option<DateTime<Utc>>,
    revision: i64,
}

/// One replica's handle to a registered autonomous agent.
#[derive(Clone)]
pub struct AgentRuntime {
    store: PlatformStore,
    identity: PlatformIdentity,
    agent_id: AgentId,
    instance_id: AgentInstanceId,
    active_fence: Arc<AtomicI64>,
    active_manifest: OpenObject,
}

impl AgentRuntime {
    pub async fn register(
        store: PlatformStore,
        spec: AgentSpec,
        instance_id: AgentInstanceId,
    ) -> Result<Self> {
        if store.config().auth_level() != StoreAuthLevel::Database {
            return Err(AgentRuntimeError::DatabaseCredentialsRequired);
        }
        validate_spec(&spec)?;
        let identity = store
            .ensure_identity(
                &spec.tenant_key,
                &format!("agent:{}", spec.agent_key),
                "veoveo://agent-runtime",
                &spec.agent_key,
                PrincipalKind::Service,
            )
            .await?;

        if let Some(record) =
            find_agent(&store, identity.tenant_id.record_id(), &spec.agent_key).await?
        {
            validate_agent_record(&record, &spec)?;
            return Ok(Self {
                store,
                identity,
                agent_id: agent_id_from_record(&record.id)?,
                instance_id,
                active_fence: Arc::new(AtomicI64::new(0)),
                active_manifest: record.manifest,
            });
        }

        let agent_id = AgentId::new();
        let now = Utc::now();
        let work_context =
            deterministic_work_context_id(&spec.tenant_key, &spec.authority.context_key)?;
        let content = AgentContent {
            tenant: identity.tenant_id.record_id(),
            agent_key: spec.agent_key.clone(),
            display_name: spec.display_name.clone(),
            profile: RecordId::new("profile", spec.profile.clone()),
            work_context: work_context.record_id(),
            policy_revision: spec.authority.policy_revision.clone(),
            authority: spec.authority.clone(),
            state: AgentState::Idle,
            manifest: spec.manifest.clone(),
            memory_database: spec.memory_database.clone(),
            last_episode: None,
            next_episode_sequence: 1,
            lease_owner: None,
            lease_expires_at: None,
            heartbeat_at: None,
            fence: 0,
            revision: 0,
            created_at: now,
            updated_at: now,
        };
        let event = outbox(
            &identity,
            "agent",
            agent_id.to_string(),
            "agent.registered",
            object([("agent_key".to_owned(), serde_json::json!(spec.agent_key))]),
        );
        let created = store
            .client()
            .query("BEGIN TRANSACTION; CREATE ONLY $agent CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("agent", agent_id.record_id()))
            .bind(("content", content))
            .bind(("event", event))
            .await
            .and_then(|response| response.check());
        if let Err(error) = created
            && find_agent(&store, identity.tenant_id.record_id(), &spec.agent_key)
                .await?
                .is_none()
        {
            return Err(AgentRuntimeError::Database(error));
        }
        let record = find_agent(&store, identity.tenant_id.record_id(), &spec.agent_key)
            .await?
            .ok_or(AgentRuntimeError::NotFound { entity: "agent" })?;
        validate_agent_record(&record, &spec)?;
        Ok(Self {
            store,
            identity,
            agent_id: agent_id_from_record(&record.id)?,
            instance_id,
            active_fence: Arc::new(AtomicI64::new(0)),
            active_manifest: record.manifest,
        })
    }

    pub fn platform_store(&self) -> &PlatformStore {
        &self.store
    }

    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }

    pub fn instance_id(&self) -> AgentInstanceId {
        self.instance_id
    }

    pub fn active_manifest(&self) -> &OpenObject {
        &self.active_manifest
    }

    pub async fn acquire_lease(&self, duration: Duration) -> Result<Option<AgentLease>> {
        let record = self.agent_record().await?;
        let now = Utc::now();
        let owner = self.instance_id.to_string();
        let held_by_other = record
            .lease_owner
            .as_deref()
            .is_some_and(|value| value != owner)
            && record.lease_expires_at.is_some_and(|expiry| expiry > now);
        if held_by_other || record.state == AgentState::Disabled {
            return Ok(None);
        }
        let fence = if record.lease_owner.as_deref() == Some(owner.as_str())
            && record.lease_expires_at.is_some_and(|expiry| expiry > now)
        {
            record.fence
        } else {
            record.fence + 1
        };
        let expires_at = deadline(now, duration)?;
        let event = outbox(
            &self.identity,
            "agent",
            self.agent_id.to_string(),
            "agent.lease_acquired",
            object([
                ("instance_id".to_owned(), serde_json::json!(owner)),
                ("fence".to_owned(), serde_json::json!(fence)),
            ]),
        );
        let result = self
            .store
            .client()
            .query("BEGIN TRANSACTION; LET $leased = (UPDATE ONLY $agent SET lease_owner = $owner, lease_expires_at = $expires, heartbeat_at = $now, fence = $fence, revision += 1, updated_at = $now WHERE revision = $revision AND state != 'disabled' AND (lease_owner = NONE OR lease_expires_at = NONE OR lease_expires_at <= $now OR lease_owner = $owner) RETURN AFTER); IF $leased = NONE { THROW 'agent lease conflict'; }; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("owner", owner))
            .bind(("expires", expires_at))
            .bind(("now", now))
            .bind(("fence", fence))
            .bind(("revision", record.revision))
            .bind(("event", event))
            .await
            .and_then(|response| response.check());
        if result.is_err() {
            return Ok(None);
        }
        self.active_fence.store(fence, Ordering::Release);
        self.recover_after_lease_acquisition().await?;
        Ok(Some(AgentLease { fence, expires_at }))
    }

    pub async fn renew_lease(&self, duration: Duration) -> Result<AgentLease> {
        let fence = self.fence()?;
        let now = Utc::now();
        let expires_at = deadline(now, duration)?;
        let mut response = self.store
            .client()
            .query("UPDATE ONLY $agent SET lease_expires_at = $expires, heartbeat_at = $now, revision += 1, updated_at = $now WHERE lease_owner = $owner AND fence = $fence AND lease_expires_at > $now RETURN AFTER;")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("owner", self.instance_id.to_string()))
            .bind(("fence", fence))
            .bind(("expires", expires_at))
            .bind(("now", now))
            .await?
            .check()?;
        let renewed: Option<AgentRecord> = response.take(0)?;
        if renewed.is_none() {
            self.active_fence.store(0, Ordering::Release);
            return Err(AgentRuntimeError::LeaseLost);
        }
        Ok(AgentLease { fence, expires_at })
    }

    pub async fn release_lease(&self) -> Result<()> {
        let fence = self.fence()?;
        let now = Utc::now();
        let event = outbox(
            &self.identity,
            "agent",
            self.agent_id.to_string(),
            "agent.lease_released",
            object([("fence".to_owned(), serde_json::json!(fence))]),
        );
        self.store
            .client()
            .query("BEGIN TRANSACTION; LET $released = (UPDATE ONLY $agent SET lease_owner = NONE, lease_expires_at = NONE, heartbeat_at = $now, state = 'idle', revision += 1, updated_at = $now WHERE lease_owner = $owner AND fence = $fence RETURN AFTER); IF $released = NONE { THROW 'agent lease lost'; }; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("owner", self.instance_id.to_string()))
            .bind(("fence", fence))
            .bind(("now", now))
            .bind(("event", event))
            .await?
            .check()?;
        self.active_fence.store(0, Ordering::Release);
        Ok(())
    }

    pub async fn start_episode(&self, wake_note: &str) -> Result<EpisodeHandle> {
        let fence = self.fence()?;
        let agent = self.agent_record().await?;
        let episode_id = AgentEpisodeId::new();
        let sequence = agent.next_episode_sequence;
        let now = Utc::now();
        let retention_pin =
            TaskRetentionPin::new(format!("agent:{}:episode:{}", self.agent_id, episode_id))
                .map_err(|error| AgentRuntimeError::InvalidField {
                    field: "episode retention pin",
                    reason: error.to_string(),
                })?;
        let content = EpisodeContent {
            tenant: self.identity.tenant_id.record_id(),
            agent: self.agent_id.record_id(),
            sequence,
            retention_pin: retention_pin.to_string(),
            wake_note: wake_note.to_owned(),
            state: AgentEpisodeState::Running,
            final_output: None,
            summary: None,
            input_tokens: 0,
            output_tokens: 0,
            completion_calls: 0,
            tool_calls: 0,
            error: None,
            started_at: now,
            finished_at: None,
            revision: 0,
        };
        let event = outbox(
            &self.identity,
            "agent_episode",
            episode_id.to_string(),
            "agent_episode.started",
            object([
                ("sequence".to_owned(), serde_json::json!(sequence)),
                ("retention_pin".to_owned(), serde_json::json!(retention_pin)),
            ]),
        );
        self.store
            .client()
            .query("BEGIN TRANSACTION; LET $leased = (UPDATE ONLY $agent SET next_episode_sequence += 1, last_episode = $episode, state = 'running', revision += 1, updated_at = $now WHERE revision = $revision AND lease_owner = $owner AND fence = $fence AND lease_expires_at > $now RETURN AFTER); IF $leased = NONE { THROW 'agent lease lost'; }; CREATE ONLY $episode CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("episode", episode_id.record_id()))
            .bind(("content", content))
            .bind(("now", now))
            .bind(("revision", agent.revision))
            .bind(("owner", self.instance_id.to_string()))
            .bind(("fence", fence))
            .bind(("event", event))
            .await?
            .check()?;
        Ok(EpisodeHandle {
            episode_id,
            sequence,
            retention_pin,
        })
    }

    pub async fn episode_retention_pin(
        &self,
        episode_id: AgentEpisodeId,
    ) -> Result<TaskRetentionPin> {
        let episode = self.episode_record(episode_id).await?;
        TaskRetentionPin::new(episode.retention_pin).map_err(|error| {
            AgentRuntimeError::InvalidField {
                field: "agent_episode.retention_pin",
                reason: error.to_string(),
            }
        })
    }

    pub async fn episodes_started_since(&self, since: DateTime<Utc>) -> Result<i64> {
        let mut response = self
            .store
            .client()
            .query("SELECT count() AS count FROM agent_episode WHERE agent = $agent AND started_at >= $since GROUP ALL;")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("since", since))
            .await?
            .check()?;
        #[derive(Deserialize, SurrealValue)]
        struct Count {
            count: i64,
        }
        let counts: Vec<Count> = response.take(0)?;
        Ok(counts.first().map_or(0, |count| count.count))
    }

    pub async fn complete_episode(
        &self,
        episode_id: AgentEpisodeId,
        completion: EpisodeCompletion,
        wakes: &[WakeId],
    ) -> Result<()> {
        let fence = self.fence()?;
        let now = Utc::now();
        let wake_records = wakes.iter().map(|id| id.record_id()).collect::<Vec<_>>();
        let episode_event = outbox(
            &self.identity,
            "agent_episode",
            episode_id.to_string(),
            "agent_episode.completed",
            object([
                ("state".to_owned(), serde_json::json!(completion.state)),
                ("wake_ids".to_owned(), serde_json::json!(wakes)),
            ]),
        );
        let wake_event = outbox(
            &self.identity,
            "wake",
            episode_id.to_string(),
            "wake.batch_acked",
            object([("wake_ids".to_owned(), serde_json::json!(wakes))]),
        );
        self.store
            .client()
            .query("BEGIN TRANSACTION; LET $lease = (SELECT * FROM ONLY $agent WHERE lease_owner = $owner AND fence = $fence AND lease_expires_at > $now); IF $lease = NONE { THROW 'agent lease lost'; }; LET $finished = (UPDATE ONLY $episode SET state = $state, final_output = $output, summary = $summary, input_tokens = $input_tokens, output_tokens = $output_tokens, completion_calls = $completion_calls, tool_calls = $tool_calls, error = $error, finished_at = $now, revision += 1 WHERE state = 'running' RETURN AFTER); IF $finished = NONE { THROW 'episode completion conflict'; }; UPDATE wake SET state = 'acked', acked_at = $now, acked_by_episode = $episode, claimed_by = NONE, claimed_at = NONE, claim_expires_at = NONE, claim_fence = NONE, updated_at = $now, revision += 1 WHERE id IN $wakes AND agent = $agent AND state = 'claimed' AND claimed_by = $owner AND claim_fence = $fence RETURN NONE; LET $deliveries = (SELECT task, retention_pin FROM agent_task WHERE agent = $agent AND result_wake IN $wakes AND consumed_by_episode = NONE); FOR $delivery IN $deliveries { UPDATE ONLY $delivery.task SET retention_pins -= $delivery.retention_pin RETURN NONE; }; UPDATE agent_task SET consumed_by_episode = $episode, retention_pin_active = false, lease_owner = NONE, lease_expires_at = NONE, updated_at = $now, revision += 1 WHERE agent = $agent AND result_wake IN $wakes AND consumed_by_episode = NONE RETURN NONE; UPDATE ONLY $agent SET state = 'idle', revision += 1, updated_at = $now WHERE lease_owner = $owner AND fence = $fence RETURN NONE; CREATE outbox_event CONTENT $episode_event RETURN NONE; CREATE outbox_event CONTENT $wake_event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("owner", self.instance_id.to_string()))
            .bind(("fence", fence))
            .bind(("now", now))
            .bind(("episode", episode_id.record_id()))
            .bind(("state", completion.state))
            .bind(("output", completion.final_output))
            .bind(("summary", completion.summary))
            .bind(("input_tokens", checked_i64(completion.input_tokens, "input_tokens")?))
            .bind(("output_tokens", checked_i64(completion.output_tokens, "output_tokens")?))
            .bind(("completion_calls", checked_i64(completion.completion_calls, "completion_calls")?))
            .bind(("tool_calls", checked_i64(completion.tool_calls, "tool_calls")?))
            .bind(("error", completion.error))
            .bind(("wakes", wake_records))
            .bind(("episode_event", episode_event))
            .bind(("wake_event", wake_event))
            .await?
            .check()?;
        Ok(())
    }

    /// Persist an accepted wake and its outbox event before any process-local hint is sent.
    pub async fn enqueue_wake(&self, wake: NewWake) -> Result<WakeId> {
        let now = Utc::now();
        let content = self.wake_content(&wake, now);
        let event = outbox(
            &self.identity,
            "wake",
            wake.wake_id.to_string(),
            "wake.enqueued",
            object([
                ("kind".to_owned(), serde_json::json!(wake.kind)),
                ("dedupe_key".to_owned(), serde_json::json!(wake.dedupe_key)),
            ]),
        );
        self.store
            .client()
            .query("BEGIN TRANSACTION; CREATE ONLY $wake CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("wake", wake.wake_id.record_id()))
            .bind(("content", content))
            .bind(("event", event))
            .await?
            .check()?;
        Ok(wake.wake_id)
    }

    pub async fn claim_wakes(&self, limit: u32, duration: Duration) -> Result<Vec<ClaimedWake>> {
        if limit == 0 || limit > 1_000 {
            return Err(AgentRuntimeError::InvalidField {
                field: "wake claim limit",
                reason: "must be in 1..=1000".to_owned(),
            });
        }
        self.recover_expired_wakes().await?;
        let now = Utc::now();
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM wake WHERE agent = $agent AND state = 'pending' AND available_at <= $now ORDER BY available_at ASC, created_at ASC LIMIT $limit;")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("now", now))
            .bind(("limit", i64::from(limit)))
            .await?
            .check()?;
        let candidates: Vec<WakeRecord> = response.take(0)?;
        let mut claimed = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if let Some(record) = self.claim_wake(candidate, duration).await? {
                claimed.push(claimed_wake(record)?);
            }
        }
        Ok(claimed)
    }

    pub async fn coalesce_wake(&self, wake: WakeId, winner: WakeId) -> Result<()> {
        let fence = self.fence()?;
        let now = Utc::now();
        let event = outbox(
            &self.identity,
            "wake",
            wake.to_string(),
            "wake.coalesced",
            object([("winner".to_owned(), serde_json::json!(winner))]),
        );
        self.store
            .client()
            .query("BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $wake SET state = 'coalesced', coalesced_into = $winner, claimed_by = NONE, claimed_at = NONE, claim_expires_at = NONE, claim_fence = NONE, acked_at = $now, updated_at = $now, revision += 1 WHERE agent = $agent AND state = 'claimed' AND claimed_by = $owner AND claim_fence = $fence RETURN AFTER); IF $updated = NONE { THROW 'wake claim lost'; }; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("wake", wake.record_id()))
            .bind(("winner", winner.record_id()))
            .bind(("agent", self.agent_id.record_id()))
            .bind(("owner", self.instance_id.to_string()))
            .bind(("fence", fence))
            .bind(("now", now))
            .bind(("event", event))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn retry_wake(
        &self,
        wake: WakeId,
        available_at: DateTime<Utc>,
        error: &str,
    ) -> Result<()> {
        let fence = self.fence()?;
        let now = Utc::now();
        let event = outbox(
            &self.identity,
            "wake",
            wake.to_string(),
            "wake.retry_scheduled",
            object([
                ("available_at".to_owned(), serde_json::json!(available_at)),
                ("error".to_owned(), serde_json::json!(error)),
            ]),
        );
        self.store
            .client()
            .query("BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $wake SET state = 'pending', available_at = $available, claimed_by = NONE, claimed_at = NONE, claim_expires_at = NONE, claim_fence = NONE, last_error = $error, updated_at = $now, revision += 1 WHERE agent = $agent AND state = 'claimed' AND claimed_by = $owner AND claim_fence = $fence RETURN AFTER); IF $updated = NONE { THROW 'wake claim lost'; }; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("wake", wake.record_id()))
            .bind(("available", available_at))
            .bind(("error", error.to_owned()))
            .bind(("now", now))
            .bind(("agent", self.agent_id.record_id()))
            .bind(("owner", self.instance_id.to_string()))
            .bind(("fence", fence))
            .bind(("event", event))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn record_task(&self, draft: NewAgentTask) -> Result<AgentTaskId> {
        self.fence()?;
        if let Some(existing) = self.task_by_task_id(draft.task_id).await? {
            if existing.retention_pin != draft.retention_pin.to_string()
                || existing.started_by_episode != draft.started_by_episode.record_id()
            {
                return Err(AgentRuntimeError::Conflict {
                    entity: "agent_task",
                });
            }
            return agent_task_id_from_record(&existing.id);
        }

        let agent_task_id = AgentTaskId::new();
        let now = Utc::now();
        let content = AgentTaskContent {
            tenant: self.identity.tenant_id.record_id(),
            agent: self.agent_id.record_id(),
            task: draft.task_id.record_id(),
            tool_name: draft.tool_name.clone(),
            descriptor: draft.descriptor,
            descriptor_complete: draft.descriptor_complete,
            state: AgentTaskWatchState::Pending,
            result: None,
            result_is_error: false,
            result_wake: None,
            retention_pin: draft.retention_pin.to_string(),
            retention_pin_active: true,
            attempt_count: 0,
            next_retry_at: now,
            lease_owner: None,
            lease_expires_at: None,
            started_by_episode: draft.started_by_episode.record_id(),
            consumed_by_episode: None,
            last_error: None,
            created_at: now,
            updated_at: now,
            resolved_at: None,
            revision: 0,
        };
        let event = outbox(
            &self.identity,
            "agent_task",
            agent_task_id.to_string(),
            "agent_task.recorded",
            object([
                ("task_id".to_owned(), serde_json::json!(draft.task_id)),
                (
                    "retention_pin".to_owned(),
                    serde_json::json!(draft.retention_pin),
                ),
            ]),
        );
        let result = self
            .store
            .client()
            .query("BEGIN TRANSACTION; LET $pinned = (SELECT * FROM ONLY $task WHERE retention_pins CONTAINS $retention_pin); IF $pinned = NONE { THROW 'canonical task retention pin is missing'; }; CREATE ONLY $agent_task CONTENT $content RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("task", draft.task_id.record_id()))
            .bind(("retention_pin", draft.retention_pin.to_string()))
            .bind(("agent_task", agent_task_id.record_id()))
            .bind(("content", content))
            .bind(("event", event))
            .await
            .and_then(|response| response.check());
        if let Err(error) = result {
            if let Some(existing) = self.task_by_task_id(draft.task_id).await? {
                return agent_task_id_from_record(&existing.id);
            }
            return Err(AgentRuntimeError::Database(error));
        }
        Ok(agent_task_id)
    }

    pub async fn complete_task_descriptor(
        &self,
        task_id: TaskId,
        descriptor: OpenObject,
    ) -> Result<()> {
        let now = Utc::now();
        let mut response = self
            .store
            .client()
            .query("UPDATE ONLY agent_task SET descriptor = $descriptor, descriptor_complete = true, updated_at = $now, revision += 1 WHERE agent = $agent AND task = $task AND state IN ['pending', 'watching'] RETURN AFTER;")
            .bind(("descriptor", descriptor))
            .bind(("now", now))
            .bind(("agent", self.agent_id.record_id()))
            .bind(("task", task_id.record_id()))
            .await?
            .check()?;
        let updated: Option<AgentTaskRecord> = response.take(0)?;
        if updated.is_none() {
            return Err(AgentRuntimeError::NotFound {
                entity: "agent_task",
            });
        }
        Ok(())
    }

    pub async fn claim_tasks(
        &self,
        limit: u32,
        duration: Duration,
    ) -> Result<Vec<ClaimedAgentTask>> {
        if limit == 0 || limit > 1_000 {
            return Err(AgentRuntimeError::InvalidField {
                field: "task claim limit",
                reason: "must be in 1..=1000".to_owned(),
            });
        }
        self.recover_pinned_tasks().await?;
        self.recover_expired_tasks().await?;
        let now = Utc::now();
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM agent_task WHERE agent = $agent AND state = 'pending' AND next_retry_at <= $now ORDER BY next_retry_at ASC, created_at ASC LIMIT $limit;")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("now", now))
            .bind(("limit", i64::from(limit)))
            .await?
            .check()?;
        let candidates: Vec<AgentTaskRecord> = response.take(0)?;
        let mut claimed = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if let Some(record) = self.claim_task(candidate, duration).await? {
                claimed.push(claimed_task(record)?);
            }
        }
        Ok(claimed)
    }

    /// Recover tasks that were atomically pinned by a server but whose
    /// `ToolTaskStarted` hook did not run before the agent process died.
    pub async fn recover_pinned_tasks(&self) -> Result<usize> {
        self.fence()?;
        let prefix = format!("agent:{}:episode:", self.agent_id);
        let mut response = self
            .store
            .client()
            .query(
                "SELECT * FROM task WHERE array::len(retention_pins) > 0 ORDER BY created_at ASC;",
            )
            .await?
            .check()?;
        let tasks: Vec<TaskRecord> = response.take(0)?;
        let mut recovered = 0;
        for task in tasks {
            let Some(pin) = task
                .retention_pins
                .iter()
                .find(|pin| pin.starts_with(&prefix))
            else {
                continue;
            };
            let task_id = task_id_from_record(&task.id)?;
            if self.task_by_task_id(task_id).await?.is_some() {
                continue;
            }
            let episode_id = AgentEpisodeId::from_str(&pin[prefix.len()..]).map_err(|error| {
                AgentRuntimeError::InvalidField {
                    field: "task retention pin episode",
                    reason: error.to_string(),
                }
            })?;
            let retention_pin = TaskRetentionPin::new(pin.clone()).map_err(|error| {
                AgentRuntimeError::InvalidField {
                    field: "task retention pin",
                    reason: error.to_string(),
                }
            })?;
            let descriptor = object([
                ("backend".to_owned(), serde_json::json!("mcp")),
                ("task_id".to_owned(), serde_json::json!(task_id)),
                (
                    "tool_name".to_owned(),
                    serde_json::json!(task.task_type.clone()),
                ),
            ]);
            self.record_task(NewAgentTask {
                task_id,
                tool_name: task.task_type,
                descriptor,
                descriptor_complete: true,
                retention_pin,
                started_by_episode: episode_id,
            })
            .await?;
            recovered += 1;
        }
        Ok(recovered)
    }

    pub async fn renew_task_claim(
        &self,
        agent_task_id: AgentTaskId,
        duration: Duration,
    ) -> Result<()> {
        let fence = self.fence()?;
        let now = Utc::now();
        let expiry = deadline(now, duration)?;
        let mut response = self
            .store
            .client()
            .query("UPDATE ONLY $task SET lease_expires_at = $expiry, updated_at = $now, revision += 1 WHERE agent = $agent AND state = 'watching' AND lease_owner = $owner AND lease_expires_at > $now RETURN AFTER;")
            .bind(("task", agent_task_id.record_id()))
            .bind(("agent", self.agent_id.record_id()))
            .bind(("owner", task_lease_owner(self.instance_id, fence)))
            .bind(("expiry", expiry))
            .bind(("now", now))
            .await?
            .check()?;
        let updated: Option<AgentTaskRecord> = response.take(0)?;
        if updated.is_none() {
            return Err(AgentRuntimeError::LeaseLost);
        }
        Ok(())
    }

    pub async fn retry_task(
        &self,
        task: &ClaimedAgentTask,
        next_retry_at: DateTime<Utc>,
        error: &str,
    ) -> Result<()> {
        let fence = self.fence()?;
        let now = Utc::now();
        let event = outbox(
            &self.identity,
            "agent_task",
            task.agent_task_id.to_string(),
            "agent_task.retry_scheduled",
            object([
                ("next_retry_at".to_owned(), serde_json::json!(next_retry_at)),
                (
                    "attempt".to_owned(),
                    serde_json::json!(task.attempt_count + 1),
                ),
            ]),
        );
        self.store
            .client()
            .query("BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET state = 'pending', attempt_count += 1, next_retry_at = $retry, lease_owner = NONE, lease_expires_at = NONE, last_error = $error, updated_at = $now, revision += 1 WHERE agent = $agent AND state = 'watching' AND lease_owner = $owner RETURN AFTER); IF $updated = NONE { THROW 'agent task lease lost'; }; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("task", task.agent_task_id.record_id()))
            .bind(("retry", next_retry_at))
            .bind(("error", error.to_owned()))
            .bind(("now", now))
            .bind(("agent", self.agent_id.record_id()))
            .bind(("owner", task_lease_owner(self.instance_id, fence)))
            .bind(("event", event))
            .await?
            .check()?;
        Ok(())
    }

    /// Store a terminal task result and create its wake in the same transaction.
    pub async fn resolve_task(
        &self,
        task: &ClaimedAgentTask,
        result: OpenObject,
        is_error: bool,
    ) -> Result<WakeId> {
        self.settle_task(task, result, is_error, false).await
    }

    pub async fn fail_task(&self, task: &ClaimedAgentTask, result: OpenObject) -> Result<WakeId> {
        self.settle_task(task, result, true, true).await
    }

    pub async fn resolve_task_in_episode(
        &self,
        task_id: TaskId,
        episode_id: AgentEpisodeId,
        result: OpenObject,
        is_error: bool,
    ) -> Result<()> {
        let existing = self
            .task_by_task_id(task_id)
            .await?
            .ok_or(AgentRuntimeError::NotFound {
                entity: "agent_task",
            })?;
        let now = Utc::now();
        let state = if is_error {
            AgentTaskWatchState::Failed
        } else {
            AgentTaskWatchState::Resolved
        };
        self.store
            .client()
            .query("BEGIN TRANSACTION; UPDATE ONLY $canonical_task SET retention_pins -= $retention_pin RETURN NONE; LET $settled = (UPDATE ONLY $agent_task SET state = $state, result = $result, result_is_error = $is_error, consumed_by_episode = $episode, retention_pin_active = false, lease_owner = NONE, lease_expires_at = NONE, resolved_at = $now, updated_at = $now, revision += 1 WHERE agent = $agent AND state IN ['pending', 'watching'] RETURN AFTER); IF $settled = NONE { THROW 'agent task settlement conflict'; }; COMMIT TRANSACTION;")
            .bind(("canonical_task", task_id.record_id()))
            .bind(("retention_pin", existing.retention_pin))
            .bind(("agent_task", existing.id))
            .bind(("state", state))
            .bind(("result", result))
            .bind(("is_error", is_error))
            .bind(("episode", episode_id.record_id()))
            .bind(("now", now))
            .bind(("agent", self.agent_id.record_id()))
            .await?
            .check()?;
        Ok(())
    }

    pub async fn unconsumed_task_results(&self) -> Result<Vec<AgentTaskResult>> {
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM agent_task WHERE agent = $agent AND state IN ['resolved', 'failed', 'cancelled'] AND consumed_by_episode = NONE AND result != NONE ORDER BY resolved_at ASC;")
            .bind(("agent", self.agent_id.record_id()))
            .await?
            .check()?;
        let records: Vec<AgentTaskRecord> = response.take(0)?;
        records
            .into_iter()
            .map(|record| {
                Ok(AgentTaskResult {
                    task_id: task_id_from_record(&record.task)?,
                    tool_name: record.tool_name,
                    result: record.result.ok_or(AgentRuntimeError::InvalidField {
                        field: "agent_task.result",
                        reason: "terminal unconsumed task has no result".to_owned(),
                    })?,
                    is_error: record.result_is_error,
                })
            })
            .collect()
    }

    pub async fn pending_task_count(&self) -> Result<usize> {
        let mut response = self
            .store
            .client()
            .query("SELECT count() AS count FROM agent_task WHERE agent = $agent AND state IN ['pending', 'watching'] GROUP ALL;")
            .bind(("agent", self.agent_id.record_id()))
            .await?
            .check()?;
        #[derive(Deserialize, SurrealValue)]
        struct Count {
            count: i64,
        }
        let counts: Vec<Count> = response.take(0)?;
        Ok(counts
            .first()
            .map_or(0, |count| count.count.max(0) as usize))
    }

    pub async fn park_elicitation(&self, draft: NewElicitation) -> Result<WakeId> {
        let now = Utc::now();
        let content = ElicitationContent {
            tenant: self.identity.tenant_id.record_id(),
            agent: self.agent_id.record_id(),
            related_task: draft.related_task.map(TaskId::record_id),
            message: draft.message,
            requested_schema: draft.requested_schema,
            state: AgentElicitationState::Parked,
            answer: None,
            answered_by: None,
            requested_at: now,
            answered_at: None,
            revision: 0,
        };
        let wake = NewWake::now(
            WakeKind::Elicitation,
            Some(format!("elicitation:{}:pending", draft.elicitation_id)),
            object([
                (
                    "elicitation_id".to_owned(),
                    serde_json::json!(draft.elicitation_id),
                ),
                ("phase".to_owned(), serde_json::json!("pending")),
            ]),
        );
        let wake_content = self.wake_content(&wake, now);
        let event = outbox(
            &self.identity,
            "agent_elicitation",
            draft.elicitation_id.to_string(),
            "agent_elicitation.parked",
            object([("wake_id".to_owned(), serde_json::json!(wake.wake_id))]),
        );
        self.store
            .client()
            .query("BEGIN TRANSACTION; CREATE ONLY $elicitation CONTENT $content RETURN NONE; CREATE ONLY $wake CONTENT $wake_content RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("elicitation", draft.elicitation_id.record_id()))
            .bind(("content", content))
            .bind(("wake", wake.wake_id.record_id()))
            .bind(("wake_content", wake_content))
            .bind(("event", event))
            .await?
            .check()?;
        Ok(wake.wake_id)
    }

    pub async fn answer_elicitation(
        &self,
        elicitation_id: AgentElicitationId,
        answer: ElicitationAnswer,
    ) -> Result<WakeId> {
        if answer.state == AgentElicitationState::Parked {
            return Err(AgentRuntimeError::InvalidField {
                field: "elicitation answer state",
                reason: "must be terminal".to_owned(),
            });
        }
        let existing = self.elicitation_record(elicitation_id).await?;
        if existing.state != AgentElicitationState::Parked {
            return Err(AgentRuntimeError::Conflict {
                entity: "agent_elicitation",
            });
        }
        let now = Utc::now();
        let wake = NewWake::now(
            WakeKind::Elicitation,
            Some(format!("elicitation:{elicitation_id}:answered")),
            object([
                (
                    "elicitation_id".to_owned(),
                    serde_json::json!(elicitation_id),
                ),
                ("phase".to_owned(), serde_json::json!("answered")),
            ]),
        );
        let wake_content = self.wake_content(&wake, now);
        let event = outbox(
            &self.identity,
            "agent_elicitation",
            elicitation_id.to_string(),
            "agent_elicitation.answered",
            object([("wake_id".to_owned(), serde_json::json!(wake.wake_id))]),
        );
        self.store
            .client()
            .query("BEGIN TRANSACTION; LET $answered = (UPDATE ONLY $elicitation SET state = $state, answer = $answer, answered_by = $answered_by, answered_at = $now, revision += 1 WHERE state = 'parked' AND revision = $revision RETURN AFTER); IF $answered = NONE { THROW 'elicitation answer conflict'; }; CREATE ONLY $wake CONTENT $wake_content RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("elicitation", elicitation_id.record_id()))
            .bind(("state", answer.state))
            .bind(("answer", answer.answer))
            .bind(("answered_by", answer.answered_by))
            .bind(("now", now))
            .bind(("revision", existing.revision))
            .bind(("wake", wake.wake_id.record_id()))
            .bind(("wake_content", wake_content))
            .bind(("event", event))
            .await?
            .check()?;
        Ok(wake.wake_id)
    }

    pub async fn parked_elicitations(&self) -> Result<Vec<ParkedElicitation>> {
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM agent_elicitation WHERE agent = $agent AND state = 'parked' ORDER BY requested_at ASC;")
            .bind(("agent", self.agent_id.record_id()))
            .await?
            .check()?;
        let records: Vec<AgentElicitationRecord> = response.take(0)?;
        records
            .into_iter()
            .map(|record| {
                Ok(ParkedElicitation {
                    elicitation_id: elicitation_id_from_record(&record.id)?,
                    related_task: record
                        .related_task
                        .as_ref()
                        .map(task_id_from_record)
                        .transpose()?,
                    message: record.message,
                    requested_schema: record.requested_schema,
                })
            })
            .collect()
    }

    async fn settle_task(
        &self,
        task: &ClaimedAgentTask,
        result: OpenObject,
        is_error: bool,
        terminal_failure: bool,
    ) -> Result<WakeId> {
        let fence = self.fence()?;
        let now = Utc::now();
        let wake = NewWake::now(
            WakeKind::TaskResult,
            Some(format!("task:{}", task.task_id)),
            object([("task_id".to_owned(), serde_json::json!(task.task_id))]),
        );
        let wake_content = self.wake_content(&wake, now);
        let state = if is_error {
            AgentTaskWatchState::Failed
        } else {
            AgentTaskWatchState::Resolved
        };
        let event = outbox(
            &self.identity,
            "agent_task",
            task.agent_task_id.to_string(),
            if terminal_failure {
                "agent_task.failed"
            } else {
                "agent_task.resolved"
            },
            object([
                ("task_id".to_owned(), serde_json::json!(task.task_id)),
                ("wake_id".to_owned(), serde_json::json!(wake.wake_id)),
            ]),
        );
        self.store
            .client()
            .query("BEGIN TRANSACTION; LET $lease = (SELECT * FROM ONLY $agent WHERE lease_owner = $instance AND fence = $fence AND lease_expires_at > $now); IF $lease = NONE { THROW 'agent lease lost'; }; LET $settled = (UPDATE ONLY $agent_task SET state = $state, result = $result, result_is_error = $is_error, result_wake = $wake, lease_owner = NONE, lease_expires_at = NONE, resolved_at = $now, updated_at = $now, revision += 1 WHERE agent = $agent AND state = 'watching' AND lease_owner = $task_owner RETURN AFTER); IF $settled = NONE { THROW 'agent task lease lost'; }; CREATE ONLY $wake CONTENT $wake_content RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("instance", self.instance_id.to_string()))
            .bind(("fence", fence))
            .bind(("now", now))
            .bind(("agent_task", task.agent_task_id.record_id()))
            .bind(("state", state))
            .bind(("result", result))
            .bind(("is_error", is_error))
            .bind(("wake", wake.wake_id.record_id()))
            .bind(("task_owner", task_lease_owner(self.instance_id, fence)))
            .bind(("wake_content", wake_content))
            .bind(("event", event))
            .await?
            .check()?;
        Ok(wake.wake_id)
    }

    async fn claim_wake(
        &self,
        candidate: WakeRecord,
        duration: Duration,
    ) -> Result<Option<WakeRecord>> {
        let fence = self.fence()?;
        let now = Utc::now();
        let expiry = deadline(now, duration)?;
        let wake_id = wake_id_from_record(&candidate.id)?;
        let event = outbox(
            &self.identity,
            "wake",
            wake_id.to_string(),
            "wake.claimed",
            object([
                ("fence".to_owned(), serde_json::json!(fence)),
                (
                    "attempt".to_owned(),
                    serde_json::json!(candidate.attempts + 1),
                ),
            ]),
        );
        let result = self
            .store
            .client()
            .query("BEGIN TRANSACTION; LET $lease = (SELECT * FROM ONLY $agent WHERE lease_owner = $owner AND fence = $fence AND lease_expires_at > $now); IF $lease = NONE { THROW 'agent lease lost'; }; LET $claimed = (UPDATE ONLY $wake SET state = 'claimed', claimed_by = $owner, claimed_at = $now, claim_expires_at = $expiry, claim_fence = $fence, attempts += 1, updated_at = $now, revision += 1 WHERE agent = $agent AND state = 'pending' AND available_at <= $now AND revision = $revision RETURN AFTER); IF $claimed = NONE { THROW 'wake claim conflict'; }; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("owner", self.instance_id.to_string()))
            .bind(("fence", fence))
            .bind(("now", now))
            .bind(("wake", candidate.id.clone()))
            .bind(("expiry", expiry))
            .bind(("revision", candidate.revision))
            .bind(("event", event))
            .await
            .and_then(|response| response.check());
        if result.is_err() {
            return Ok(None);
        }
        self.wake_record(wake_id).await.map(Some)
    }

    async fn claim_task(
        &self,
        candidate: AgentTaskRecord,
        duration: Duration,
    ) -> Result<Option<AgentTaskRecord>> {
        let fence = self.fence()?;
        let now = Utc::now();
        let expiry = deadline(now, duration)?;
        let agent_task_id = agent_task_id_from_record(&candidate.id)?;
        let lease_owner = task_lease_owner(self.instance_id, fence);
        let event = outbox(
            &self.identity,
            "agent_task",
            agent_task_id.to_string(),
            "agent_task.claimed",
            object([
                ("fence".to_owned(), serde_json::json!(fence)),
                (
                    "attempt".to_owned(),
                    serde_json::json!(candidate.attempt_count),
                ),
            ]),
        );
        let result = self
            .store
            .client()
            .query("BEGIN TRANSACTION; LET $lease = (SELECT * FROM ONLY $agent WHERE lease_owner = $instance AND fence = $fence AND lease_expires_at > $now); IF $lease = NONE { THROW 'agent lease lost'; }; LET $claimed = (UPDATE ONLY $task SET state = 'watching', lease_owner = $task_owner, lease_expires_at = $expiry, updated_at = $now, revision += 1 WHERE agent = $agent AND state = 'pending' AND next_retry_at <= $now AND revision = $revision RETURN AFTER); IF $claimed = NONE { THROW 'agent task claim conflict'; }; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("instance", self.instance_id.to_string()))
            .bind(("fence", fence))
            .bind(("now", now))
            .bind(("task", candidate.id.clone()))
            .bind(("task_owner", lease_owner))
            .bind(("expiry", expiry))
            .bind(("revision", candidate.revision))
            .bind(("event", event))
            .await
            .and_then(|response| response.check());
        if result.is_err() {
            return Ok(None);
        }
        self.agent_task_record(agent_task_id).await.map(Some)
    }

    async fn recover_after_lease_acquisition(&self) -> Result<()> {
        let now = Utc::now();
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM agent_episode WHERE agent = $agent AND state = 'running';")
            .bind(("agent", self.agent_id.record_id()))
            .await?
            .check()?;
        let episodes: Vec<AgentEpisodeRecord> = response.take(0)?;
        for episode in episodes {
            let episode_id = episode_id_from_record(&episode.id)?;
            let event = outbox(
                &self.identity,
                "agent_episode",
                episode_id.to_string(),
                "agent_episode.crashed",
                object([(
                    "reason".to_owned(),
                    serde_json::json!("scheduler lease was recovered"),
                )]),
            );
            self.store
                .client()
                .query("BEGIN TRANSACTION; UPDATE ONLY $episode SET state = 'crashed', error = 'scheduler lease was recovered', finished_at = $now, revision += 1 WHERE state = 'running' RETURN NONE; CREATE outbox_event CONTENT $event RETURN NONE; COMMIT TRANSACTION;")
                .bind(("episode", episode_id.record_id()))
                .bind(("now", now))
                .bind(("event", event))
                .await?
                .check()?;
        }
        self.recover_expired_wakes().await?;
        self.recover_pinned_tasks().await?;
        self.recover_expired_tasks().await?;
        Ok(())
    }

    async fn recover_expired_wakes(&self) -> Result<()> {
        self.fence()?;
        let now = Utc::now();
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM wake WHERE agent = $agent AND state = 'claimed' AND (claim_expires_at = NONE OR claim_expires_at <= $now);")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("now", now))
            .await?
            .check()?;
        let expired: Vec<WakeRecord> = response.take(0)?;
        for record in expired {
            let wake_id = wake_id_from_record(&record.id)?;
            let event = outbox(
                &self.identity,
                "wake",
                wake_id.to_string(),
                "wake.claim_recovered",
                object([("attempts".to_owned(), serde_json::json!(record.attempts))]),
            );
            self.store
                .client()
                .query("BEGIN TRANSACTION; LET $recovered = (UPDATE ONLY $wake SET state = 'pending', claimed_by = NONE, claimed_at = NONE, claim_expires_at = NONE, claim_fence = NONE, available_at = $now, last_error = 'claim lease expired', updated_at = $now, revision += 1 WHERE state = 'claimed' AND revision = $revision RETURN AFTER); IF $recovered != NONE { CREATE outbox_event CONTENT $event RETURN NONE; }; COMMIT TRANSACTION;")
                .bind(("wake", record.id))
                .bind(("now", now))
                .bind(("revision", record.revision))
                .bind(("event", event))
                .await?
                .check()?;
        }
        Ok(())
    }

    async fn recover_expired_tasks(&self) -> Result<()> {
        self.fence()?;
        let now = Utc::now();
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM agent_task WHERE agent = $agent AND state = 'watching' AND (lease_expires_at = NONE OR lease_expires_at <= $now);")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("now", now))
            .await?
            .check()?;
        let expired: Vec<AgentTaskRecord> = response.take(0)?;
        for record in expired {
            let task_id = agent_task_id_from_record(&record.id)?;
            let event = outbox(
                &self.identity,
                "agent_task",
                task_id.to_string(),
                "agent_task.claim_recovered",
                object([(
                    "attempt_count".to_owned(),
                    serde_json::json!(record.attempt_count),
                )]),
            );
            self.store
                .client()
                .query("BEGIN TRANSACTION; LET $recovered = (UPDATE ONLY $task SET state = 'pending', lease_owner = NONE, lease_expires_at = NONE, next_retry_at = $now, last_error = 'claim lease expired', updated_at = $now, revision += 1 WHERE state = 'watching' AND revision = $revision RETURN AFTER); IF $recovered != NONE { CREATE outbox_event CONTENT $event RETURN NONE; }; COMMIT TRANSACTION;")
                .bind(("task", record.id))
                .bind(("now", now))
                .bind(("revision", record.revision))
                .bind(("event", event))
                .await?
                .check()?;
        }
        Ok(())
    }

    fn wake_content(&self, wake: &NewWake, now: DateTime<Utc>) -> WakeContent {
        WakeContent {
            tenant: self.identity.tenant_id.record_id(),
            agent: self.agent_id.record_id(),
            kind: wake.kind,
            state: WakeState::Pending,
            dedupe_key: wake.dedupe_key.clone(),
            payload: wake.payload.clone(),
            available_at: wake.available_at,
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

    async fn agent_record(&self) -> Result<AgentRecord> {
        select_only(&self.store, self.agent_id.record_id(), "agent").await
    }

    async fn episode_record(&self, episode_id: AgentEpisodeId) -> Result<AgentEpisodeRecord> {
        select_only(&self.store, episode_id.record_id(), "agent_episode").await
    }

    async fn wake_record(&self, wake_id: WakeId) -> Result<WakeRecord> {
        select_only(&self.store, wake_id.record_id(), "wake").await
    }

    async fn agent_task_record(&self, task_id: AgentTaskId) -> Result<AgentTaskRecord> {
        select_only(&self.store, task_id.record_id(), "agent_task").await
    }

    async fn elicitation_record(
        &self,
        elicitation_id: AgentElicitationId,
    ) -> Result<AgentElicitationRecord> {
        select_only(&self.store, elicitation_id.record_id(), "agent_elicitation").await
    }

    pub async fn elicitation(
        &self,
        elicitation_id: AgentElicitationId,
    ) -> Result<AgentElicitationRecord> {
        self.elicitation_record(elicitation_id).await
    }

    async fn task_by_task_id(&self, task_id: TaskId) -> Result<Option<AgentTaskRecord>> {
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM agent_task WHERE agent = $agent AND task = $task LIMIT 1;")
            .bind(("agent", self.agent_id.record_id()))
            .bind(("task", task_id.record_id()))
            .await?
            .check()?;
        let records: Vec<AgentTaskRecord> = response.take(0)?;
        Ok(records.into_iter().next())
    }

    fn fence(&self) -> Result<i64> {
        let fence = self.active_fence.load(Ordering::Acquire);
        if fence <= 0 {
            Err(AgentRuntimeError::LeaseLost)
        } else {
            Ok(fence)
        }
    }
}

async fn select_only<T>(store: &PlatformStore, record: RecordId, entity: &'static str) -> Result<T>
where
    T: SurrealValue,
{
    let mut response = store
        .client()
        .query("SELECT * FROM ONLY $record;")
        .bind(("record", record))
        .await?
        .check()?;
    response
        .take::<Option<T>>(0)?
        .ok_or(AgentRuntimeError::NotFound { entity })
}

async fn find_agent(
    store: &PlatformStore,
    tenant: RecordId,
    agent_key: &str,
) -> Result<Option<AgentRecord>> {
    let mut response = store
        .client()
        .query("SELECT * FROM agent WHERE tenant = $tenant AND agent_key = $agent_key LIMIT 1;")
        .bind(("tenant", tenant))
        .bind(("agent_key", agent_key.to_owned()))
        .await?
        .check()?;
    let records: Vec<AgentRecord> = response.take(0)?;
    Ok(records.into_iter().next())
}

fn validate_spec(spec: &AgentSpec) -> Result<()> {
    for (field, value) in [
        ("tenant_key", spec.tenant_key.as_str()),
        ("agent_key", spec.agent_key.as_str()),
        ("display_name", spec.display_name.as_str()),
        ("profile", spec.profile.as_str()),
        ("memory_database", spec.memory_database.as_str()),
    ] {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(AgentRuntimeError::InvalidField {
                field,
                reason: "must be non-empty and contain no control characters".to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_agent_record(record: &AgentRecord, spec: &AgentSpec) -> Result<()> {
    if record.agent_key != spec.agent_key
        || record.profile != RecordId::new("profile", spec.profile.clone())
        || record.work_context
            != deterministic_work_context_id(&spec.tenant_key, &spec.authority.context_key)?
                .record_id()
        || record.policy_revision != spec.authority.policy_revision
        || record.authority != spec.authority
        || record.memory_database != spec.memory_database
    {
        return Err(AgentRuntimeError::AgentConflict(spec.agent_key.clone()));
    }
    Ok(())
}

fn deadline(now: DateTime<Utc>, duration: Duration) -> Result<DateTime<Utc>> {
    let delta = TimeDelta::from_std(duration).map_err(|error| AgentRuntimeError::InvalidField {
        field: "lease duration",
        reason: error.to_string(),
    })?;
    Ok(now + delta)
}

fn outbox(
    identity: &PlatformIdentity,
    aggregate_type: &str,
    aggregate_id: String,
    event_type: &str,
    payload: OpenObject,
) -> OutboxDraft {
    OutboxDraft::now(
        Some(identity.tenant_id.record_id()),
        aggregate_type,
        aggregate_id,
        event_type,
        EVENT_SCHEMA_VERSION,
        payload,
    )
}

fn task_lease_owner(instance_id: AgentInstanceId, fence: i64) -> String {
    format!("{instance_id}:{fence}")
}

fn claimed_wake(record: WakeRecord) -> Result<ClaimedWake> {
    Ok(ClaimedWake {
        wake_id: wake_id_from_record(&record.id)?,
        kind: record.kind,
        dedupe_key: record.dedupe_key,
        payload: record.payload,
        attempts: record.attempts,
    })
}

fn claimed_task(record: AgentTaskRecord) -> Result<ClaimedAgentTask> {
    Ok(ClaimedAgentTask {
        agent_task_id: agent_task_id_from_record(&record.id)?,
        task_id: task_id_from_record(&record.task)?,
        tool_name: record.tool_name,
        descriptor: record.descriptor,
        descriptor_complete: record.descriptor_complete,
        attempt_count: record.attempt_count,
    })
}

fn agent_id_from_record(record: &RecordId) -> Result<AgentId> {
    Ok(AgentId::from_uuid(uuid_from_record(record, "agent.id")?))
}

fn wake_id_from_record(record: &RecordId) -> Result<WakeId> {
    Ok(WakeId::from_uuid(uuid_from_record(record, "wake.id")?))
}

fn episode_id_from_record(record: &RecordId) -> Result<AgentEpisodeId> {
    Ok(AgentEpisodeId::from_uuid(uuid_from_record(
        record,
        "agent_episode.id",
    )?))
}

fn agent_task_id_from_record(record: &RecordId) -> Result<AgentTaskId> {
    Ok(AgentTaskId::from_uuid(uuid_from_record(
        record,
        "agent_task.id",
    )?))
}

fn elicitation_id_from_record(record: &RecordId) -> Result<AgentElicitationId> {
    Ok(AgentElicitationId::from_uuid(uuid_from_record(
        record,
        "agent_elicitation.id",
    )?))
}
