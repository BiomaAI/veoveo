SELECT * FROM agent_task WHERE agent = $agent AND tenant = $tenant ORDER BY created_at DESC LIMIT 512;
