LET $run = SELECT * FROM ONLY $command.target;
LET $chat = SELECT * FROM ONLY $command.chat;
IF !fn::workspace_member($authority, $command.chat) OR $run.chat != $command.chat { THROW 'workspace_not_found'; };
IF $authority.membership = 'viewer' OR ($run.initiator != $authority.principal AND $chat.owner != $authority.principal) {
    THROW 'workspace_forbidden';
};
IF $run.state IN ['queued', 'running'] {
    LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
    UPDATE ONLY $command.target SET state = 'cancelled', fence = NONE, updated_sequence = $head.sequence, updated_at = time::now();
    CREATE ONLY type::record('workspace_event', [$command.chat, $head.sequence]) SET
        chat = $command.chat, sequence = $head.sequence, kind = 'run', target = $command.target;
};
RETURN SELECT * FROM ONLY $command.target;
