LET $chat = SELECT * FROM ONLY $command.chat;
LET $agent = SELECT * FROM ONLY $command.target;
IF !fn::workspace_member($authority, $command.chat) OR $agent.chat != $command.chat { THROW 'workspace_not_found'; };
IF $chat.owner != $authority.principal OR $authority.membership = 'viewer' { THROW 'workspace_forbidden'; };
IF $agent.active {
    UPDATE ONLY $command.target SET active = false;
    IF $command.target IN ($chat.participation.agents ?? []) {
        LET $remaining = array::complement($chat.participation.agents, [$command.target]);
        UPDATE ONLY $command.chat SET participation = {
            mode: IF array::len($remaining) = 0 { 'on_request' } ELSE { $chat.participation.mode },
            agents: $remaining
        }, revision += 1;
    };
    LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
    CREATE ONLY type::record('workspace_event', [$command.chat, $head.sequence]) SET
        chat = $command.chat, sequence = $head.sequence, kind = 'agent', target = $command.target;
};
RETURN SELECT * FROM ONLY $command.target;
