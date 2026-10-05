SELECT * FROM agent_task WHERE agent = $agent AND state IN ['resolved', 'failed', 'cancelled'] AND consumed_by_episode = NONE AND result != NONE ORDER BY resolved_at ASC;
