LET $current_chat = SELECT * FROM ONLY $command.chat;
IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; };
IF $authority.membership = 'viewer' { THROW 'workspace_forbidden'; };
LET $existing = SELECT * FROM ONLY $command.message;
IF $existing != NONE {
    IF $existing.chat != $command.chat { THROW 'workspace_not_found'; };
    IF $existing.author != $command.member OR $existing.text != $command.text
        OR $existing.reply_to != $command.reply_to
        OR ($existing.addressed_agents ?? []) != $command.addressed_agents {
        THROW 'workspace_conflict';
    };
} ELSE {
    IF $current_chat.archived { THROW 'workspace_conflict'; };
    LET $reply = IF $command.reply_to = NONE { NONE } ELSE { SELECT * FROM ONLY $command.reply_to };
    IF $command.reply_to != NONE AND $reply.chat != $command.chat {
        THROW 'workspace_not_found';
    };
    LET $policy = $current_chat.participation ?? { mode: 'on_request', agents: [] };
    LET $targets = IF $policy.mode = 'automatic' {
        array::union($command.addressed_agents, $policy.agents)
    } ELSE IF $policy.mode = 'default' AND array::len($command.addressed_agents) = 0 {
        $policy.agents
    } ELSE { $command.addressed_agents };
    IF array::len($targets) > 4 { THROW 'workspace_conflict'; };
    IF array::len($targets) > 0 AND ($command.deadline <= time::now()
        OR $command.deadline > time::now() + 180s) { THROW 'workspace_conflict'; };
    FOR $target IN $targets {
        LET $agent = SELECT * FROM ONLY $target;
        IF $agent.chat != $command.chat OR !$agent.active { THROW 'workspace_not_found'; };
        IF count($command.candidates[WHERE agent = $target]) != 1 { THROW 'workspace_conflict'; };
    };
    LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
    LET $sequence = $head.sequence;
    CREATE ONLY type::record('workspace_event', [$command.chat, $sequence]) SET
        chat = $command.chat, sequence = $sequence, kind = 'message', target = $command.message;
    CREATE ONLY $command.message SET chat = $command.chat, author = $command.member,
        text = $command.text, reply_to = $command.reply_to, sequence = $sequence,
        addressed_agents = $command.addressed_agents, response_agents = $targets;
    FOR $target IN $targets {
        LET $agent = SELECT * FROM ONLY $target;
        LET $run = $command.candidates[WHERE agent = $target][0].run;
        LET $full = count(SELECT id FROM workspace_run WHERE chat = $command.chat
            AND state IN ['queued', 'running'] AND lease_until > time::now()) >= 4;
        LET $run_head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
        CREATE ONLY $run SET chat = $command.chat, agent = $target,
            definition_digest = $agent.definition_digest, initiator = $authority.principal,
            trigger = $command.message, context_sequence = $sequence, sequence = $run_head.sequence,
            updated_sequence = $run_head.sequence, state = IF $full { 'failed' } ELSE { 'queued' },
            failure = IF $full { 'capacity' } ELSE { NONE },
            lease_until = time::now() + 20s, deadline = $command.deadline;
        CREATE ONLY type::record('workspace_event', [$command.chat, $run_head.sequence]) SET
            chat = $command.chat, sequence = $run_head.sequence, kind = 'run', target = $run;
    };
};
LET $message = SELECT * FROM ONLY $command.message;
RETURN { message: $message, runs: (SELECT * FROM workspace_run WHERE trigger = $command.message
    AND agent IN ($message.response_agents ?? []) ORDER BY sequence LIMIT 4) };
