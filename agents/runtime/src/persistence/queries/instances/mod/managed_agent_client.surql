RETURN array::first(SELECT * FROM managed_agent WHERE identity.client_id = $command AND public_key != NONE AND active_generation > 0 AND fn::managed_agent_enabled(id) LIMIT 1);
