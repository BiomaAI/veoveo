SELECT * FROM agent_task WHERE agent = $agent AND state = 'watching' AND (lease_expires_at = NONE OR lease_expires_at <= $now);
