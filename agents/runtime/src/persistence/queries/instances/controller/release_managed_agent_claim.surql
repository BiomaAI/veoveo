IF !fn::managed_agent_claim($command) { RETURN false; }; UPDATE ONLY $command.operation SET lease_owner = NONE, lease_expires_at = NONE; RETURN true;
