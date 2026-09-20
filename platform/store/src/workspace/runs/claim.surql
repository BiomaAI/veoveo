LET $run = SELECT * FROM ONLY $command.run;
LET $agent = SELECT * FROM ONLY $run.agent;
LET $chat = SELECT * FROM ONLY $command.chat;
IF !fn::workspace_member($authority, $command.chat) OR $run.chat != $command.chat { THROW 'workspace_not_found'; };
IF $authority.membership = 'viewer' OR $run.initiator != $authority.principal { THROW 'workspace_forbidden'; };
IF $chat.archived OR !$agent.active OR !fn::agent_chat_revision($authority.tenant, $authority.work_context, $agent.definition, $run.definition_digest, false)
    OR $run.state != 'queued' OR $run.lease_until <= time::now() OR $run.deadline <= time::now() {
    THROW 'workspace_conflict';
};
LET $head = UPDATE ONLY $command.chat SET sequence += 1, updated_at = time::now();
UPDATE ONLY $command.run SET state = 'running', fence = $command.fence,
    lease_until = time::now() + 20s, updated_sequence = $head.sequence, updated_at = time::now();
CREATE ONLY type::record('workspace_event', [$command.chat, $head.sequence]) SET
    chat = $command.chat, sequence = $head.sequence, kind = 'run', target = $command.run;
RETURN SELECT * FROM ONLY $command.run;
