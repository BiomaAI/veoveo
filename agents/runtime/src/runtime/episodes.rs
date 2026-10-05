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

        let mut response = self
            .store
            .client()
            .query(include_str!(
                "../queries/runtime/episodes/start_episode.surql"
            ))
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
            .query(include_str!(
                "../queries/runtime/episodes/episodes_started_since.surql"
            ))
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

        let mut response = self
            .store
            .client()
            .query(include_str!(
                "../queries/runtime/episodes/complete_episode.surql"
            ))
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
