LET $run = SELECT * FROM ONLY $command.run;
IF !fn::workspace_member($authority, $command.chat) OR $run.chat != $command.chat { THROW 'workspace_not_found'; };
IF $authority.membership = 'viewer' OR $run.initiator != $authority.principal { THROW 'workspace_forbidden'; };
IF $run.state = 'queued' {
    LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
    UPDATE ONLY $command.run SET state = 'failed', failure = $command.failure,
        updated_sequence = $head.sequence, updated_at = time::now();
    CREATE ONLY type::record('workspace_event', [$command.chat, $head.sequence]) SET
        chat = $command.chat, sequence = $head.sequence, kind = 'run', target = $command.run;
};
RETURN SELECT * FROM ONLY $command.run;
