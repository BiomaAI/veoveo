IF !fn::workspace_operation_owner($authority, $command.operation) { THROW 'workspace_not_found'; };
LET $operation = SELECT * FROM ONLY $command.operation;
IF !fn::workspace_member($authority, $operation.chat) { THROW 'workspace_not_found'; };
IF $authority.membership = 'viewer' { THROW 'workspace_forbidden'; };
LET $chat = SELECT * FROM ONLY $operation.chat;
IF $chat.archived OR $operation.phase != 'dispatching' OR $operation.fence != $command.fence
    OR $operation.dispatch_until <= time::now() { THROW 'workspace_conflict'; };
IF $command.run_fence != NONE {
    LET $run = SELECT * FROM ONLY $operation.run;
    LET $agent = SELECT * FROM ONLY $run.agent;
    IF $run = NONE OR $run.chat != $operation.chat OR $run.initiator != $authority.principal
        OR $run.state != 'running' OR $run.fence != $command.run_fence
        OR $run.lease_until <= time::now() OR $run.deadline <= time::now()
        OR !$agent.active OR !fn::agent_chat_revision($authority.tenant, $authority.work_context, $agent.definition, $run.definition_digest, false) { THROW 'workspace_conflict'; };
};
UPDATE ONLY $operation.chat SET updated_at = time::now();
RETURN true;
