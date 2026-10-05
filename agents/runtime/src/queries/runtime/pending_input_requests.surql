SELECT * FROM agent_input_request WHERE agent = $agent AND state = 'pending' ORDER BY requested_at ASC;
