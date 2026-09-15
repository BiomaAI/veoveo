LET $chat = SELECT * FROM ONLY $command.chat;
LET $agent = SELECT * FROM ONLY $command.agent;
LET $trigger = SELECT * FROM ONLY $command.trigger;
IF !fn::workspace_member($authority, $command.chat) OR $agent.chat != $command.chat
    OR $trigger.chat != $command.chat { THROW 'workspace_not_found'; };
IF $authority.membership = 'viewer' OR $trigger.author != $command.member { THROW 'workspace_forbidden'; };
LET $existing = SELECT * FROM ONLY $command.run;
IF $existing = NONE {
    IF $chat.archived OR !$agent.active OR $agent.definition_digest != $command.definition_digest {
        THROW 'workspace_conflict';
    };
    IF $command.deadline <= time::now() OR $command.deadline > time::now() + 180s { THROW 'workspace_conflict'; };
    IF count(SELECT id FROM workspace_run WHERE chat = $command.chat AND state IN ['queued', 'running']
        AND lease_until > time::now()) >= 4 { THROW 'workspace_conflict'; };
    LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
    CREATE ONLY $command.run SET chat = $command.chat, agent = $command.agent,
        definition_digest = $agent.definition_digest, initiator = $authority.principal,
        trigger = $command.trigger, context_sequence = $chat.sequence, sequence = $head.sequence,
        updated_sequence = $head.sequence, state = 'queued',
        lease_until = time::now() + 20s, deadline = $command.deadline;
    CREATE ONLY type::record('workspace_event', [$command.chat, $head.sequence]) SET
        chat = $command.chat, sequence = $head.sequence, kind = 'run', target = $command.run;
} ELSE {
    IF $existing.chat != $command.chat { THROW 'workspace_not_found'; };
    IF $existing.initiator != $authority.principal OR $existing.definition_digest != $command.definition_digest {
        THROW 'workspace_conflict';
    };
};
RETURN SELECT * FROM ONLY $command.run;
