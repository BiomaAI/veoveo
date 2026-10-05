SELECT count() AS count FROM agent_task WHERE agent = $agent AND state IN ['pending', 'watching'] GROUP ALL;
