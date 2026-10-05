IF !fn::workspace_member($authority, $command) { THROW 'workspace_not_found'; };              RETURN SELECT * FROM workspace_agent WHERE chat = $command LIMIT 64;
