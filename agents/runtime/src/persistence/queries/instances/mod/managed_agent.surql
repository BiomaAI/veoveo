LET $instance = SELECT * FROM ONLY $command; IF !fn::managed_agent_editor($authority, $instance) { THROW 'agent_not_found'; }; RETURN $instance;
