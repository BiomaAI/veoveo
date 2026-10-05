SELECT * FROM agent_input_request WHERE agent = $agent AND tenant = $tenant AND state = 'pending' ORDER BY requested_at ASC LIMIT 128;
