IF !fn::managed_agent_claim($command) { THROW 'agent_conflict'; }; UPDATE ONLY $command.operation SET lease_expires_at = time::now() + 30s; RETURN true;
