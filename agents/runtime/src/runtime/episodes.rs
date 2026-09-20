//! Episode admission, completion and native Task retention are atomic.
use super::*;

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

const COMPLETE_EPISODE_QUERY: &str = r#"
BEGIN TRANSACTION;
LET $lease = (SELECT * FROM ONLY $agent WHERE lease_owner = $owner AND fence = $fence AND lease_expires_at > $now);
IF $lease = NONE { THROW 'agent lease lost'; };
LET $current = SELECT * FROM ONLY $episode;
IF $current.agent = $agent AND $current.state = 'stopped' {
    UPDATE ONLY $agent SET state = 'idle', revision += 1, updated_at = $now WHERE last_episode = $episode AND lease_owner = $owner AND fence = $fence RETURN NONE;
    RETURN true;
};
LET $claimed_wakes = (SELECT VALUE id FROM wake WHERE id IN $wakes AND agent = $agent AND state = 'claimed' AND claimed_by = $owner AND claim_fence = $fence);
IF array::len($claimed_wakes) != array::len($wakes) { THROW 'wake claim lost'; };
fn::agent_consume_results($agent, $claimed_wakes, $episode, $now);
LET $finished = (UPDATE ONLY $episode SET state = $state, final_output = $output, summary = $summary, input_tokens = $input_tokens, output_tokens = $output_tokens, completion_calls = $completion_calls, tool_calls = $tool_calls, error = $error, finished_at = $now, revision += 1 WHERE state = 'running' RETURN AFTER);
IF $finished = NONE { THROW 'episode completion conflict'; };
UPDATE wake SET state = 'acked', acked_at = $now, acked_by_episode = $episode, claimed_by = NONE, claimed_at = NONE, claim_expires_at = NONE, claim_fence = NONE, updated_at = $now, revision += 1 WHERE id IN $claimed_wakes RETURN NONE;
UPDATE ONLY $agent SET state = 'idle', revision += 1, updated_at = $now WHERE lease_owner = $owner AND fence = $fence RETURN NONE;
CREATE outbox_event CONTENT $episode_event RETURN NONE;
CREATE outbox_event CONTENT $wake_event RETURN NONE;
COMMIT TRANSACTION;
"#;

impl AgentRuntime {
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
        let mut response = self
            .store
            .client()
            .query(include_str!("start_episode.surql"))
            .bind(("agent", self.agent_id.record_id()))
            .bind((
                "managed_instance",
                self.managed.as_ref().map(|m| m.instance.clone()),
            ))
            .bind((
                "managed_generation",
                self.managed.as_ref().map(|m| m.generation),
            ))
            .bind(("episode", episode_id.record_id()))
            .bind(("content", content))
            .bind(("now", now))
            .bind(("revision", agent.revision))
            .bind(("owner", self.instance_id.to_string()))
            .bind(("fence", fence))
            .bind(("event", event))
            .await?
            .check()?;
        let result = response.num_statements().saturating_sub(2);
        let managed = response.take(result)?;
        Ok(EpisodeHandle {
            episode_id,
            sequence,
            retention_pin,
            managed,
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
        let mut response = self
            .store
            .client()
            .query(COMPLETE_EPISODE_QUERY)
            .bind(("agent", self.agent_id.record_id()))
            .bind(("owner", self.instance_id.to_string()))
            .bind(("fence", fence))
            .bind(("now", now))
            .bind(("episode", episode_id.record_id()))
            .bind(("state", completion.state))
            .bind(("output", completion.final_output))
            .bind(("summary", completion.summary))
            .bind((
                "input_tokens",
                checked_i64(completion.input_tokens, "input_tokens")?,
            ))
            .bind((
                "output_tokens",
                checked_i64(completion.output_tokens, "output_tokens")?,
            ))
            .bind((
                "completion_calls",
                checked_i64(completion.completion_calls, "completion_calls")?,
            ))
            .bind((
                "tool_calls",
                checked_i64(completion.tool_calls, "tool_calls")?,
            ))
            .bind(("error", completion.error))
            .bind(("wakes", wake_records))
            .bind(("episode_event", episode_event))
            .bind(("wake_event", wake_event))
            .await?;
        let mut errors = response.take_errors().into_iter().collect::<Vec<_>>();
        errors.sort_by_key(|(statement, _)| *statement);
        if !errors.is_empty() {
            return Err(AgentRuntimeError::DatabaseOperation {
                operation: "complete episode",
                errors: errors
                    .into_iter()
                    .map(|(statement, error)| format!("statement {statement}: {error}"))
                    .collect::<Vec<_>>()
                    .join("; "),
            });
        }
        Ok(())
    }
}
