LET $operation = SELECT * FROM ONLY $command; IF $operation = NONE OR !fn::managed_agent_editor($authority, $operation.instance.*) { THROW 'agent_not_found'; }; RETURN $operation;
