LET $run = SELECT * FROM ONLY $command.target;
IF !fn::workspace_member($authority, $command.chat) OR $run.chat != $command.chat { THROW 'workspace_not_found'; };
IF $authority.membership = 'viewer' OR $run.initiator != $authority.principal { THROW 'workspace_forbidden'; };
RETURN {
    run: $run,
    trigger: (SELECT * FROM ONLY $run.trigger),
    members: (SELECT * FROM workspace_member WHERE chat = $command.chat LIMIT 256),
    people: (SELECT id, display_name FROM principal WHERE id IN
        (SELECT VALUE principal FROM workspace_member WHERE chat = $command.chat LIMIT 256)),
    messages: (SELECT * FROM workspace_message WHERE chat = $command.chat AND sequence <= $run.context_sequence
        ORDER BY sequence DESC LIMIT 100),
    completed_runs: (SELECT * FROM workspace_run WHERE chat = $command.chat AND state = 'completed'
        AND updated_sequence <= $run.context_sequence ORDER BY sequence DESC LIMIT 64),
    agents: (SELECT * FROM workspace_agent WHERE chat = $command.chat LIMIT 64),
};
