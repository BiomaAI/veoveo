SELECT * FROM agent_episode WHERE agent = $agent AND tenant = $tenant ORDER BY started_at DESC LIMIT 256;
