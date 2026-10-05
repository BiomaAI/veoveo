LET $definition = SELECT * FROM ONLY $command; IF !fn::agent_definition_editor($authority, $definition) { THROW 'agent_not_found'; }; RETURN $definition;
