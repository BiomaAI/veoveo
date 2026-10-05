SELECT * FROM agent WHERE tenant = $tenant AND work_context = $work_context AND agent_key = $agent_key LIMIT 2;
