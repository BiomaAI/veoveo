SELECT * FROM agent_task WHERE agent = $agent AND state = 'pending' AND next_retry_at <= $now ORDER BY next_retry_at ASC, created_at ASC LIMIT $limit;
