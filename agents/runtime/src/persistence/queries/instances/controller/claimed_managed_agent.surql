IF !fn::managed_agent_claim($command) { THROW 'agent_conflict'; }; RETURN SELECT * FROM ONLY $command.instance;
