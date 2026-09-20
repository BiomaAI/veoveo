BEGIN TRANSACTION;
LET $managed = array::first(SELECT * FROM managed_agent WHERE tenant = $agent.tenant AND key = $agent.agent_key LIMIT 1);
IF ($managed = NONE AND $managed_instance != NONE) OR ($managed != NONE AND $managed_instance = NONE) { THROW 'managed episode binding required'; };
LET $admitted = IF $managed != NONE {
    UPDATE ONLY $managed.id SET admission_count += 1
    WHERE id = $managed_instance AND generation = $managed_generation AND active_generation = generation
        AND active_revision != NONE AND desired = 'running' AND observed IN ['workload', 'ready']
        AND fn::managed_agent_enabled(id) RETURN AFTER
} ELSE { NONE };
IF $managed != NONE AND $admitted = NONE { THROW 'managed episode admission closed'; };
LET $binding = IF $admitted != NONE {
    { instance: $admitted.id, revision: $admitted.active_revision, generation: $admitted.generation, epoch: $admitted.dispatch_epoch }
} ELSE { NONE };
LET $leased = UPDATE ONLY $agent SET next_episode_sequence += 1, last_episode = $episode, state = 'running', revision += 1, updated_at = $now
    WHERE (last_episode = NONE OR last_episode.state != 'running') AND revision = $revision AND lease_owner = $owner AND fence = $fence AND lease_expires_at > $now RETURN AFTER;
IF $leased = NONE { THROW 'agent lease lost'; };
CREATE ONLY $episode CONTENT $content RETURN NONE;
UPDATE ONLY $episode SET managed = $binding RETURN NONE;
CREATE outbox_event CONTENT $event RETURN NONE;
RETURN $binding;
COMMIT TRANSACTION;
