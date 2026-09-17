IF !fn::workspace_operation_owner($authority, $command.operation) { THROW 'workspace_not_found'; };
LET $old = SELECT * FROM ONLY $command.operation;
IF $authority.membership = 'viewer' { THROW 'workspace_forbidden'; };
IF !fn::workspace_member($authority, $old.chat) { THROW 'workspace_not_found'; };
LET $chat = SELECT * FROM ONLY $old.chat;
IF $chat.archived OR $old.phase != 'input_required' OR $old.revision != $command.revision
    OR $old.round >= 10 { THROW 'workspace_conflict'; };
-- A human explicitly answering input continues the operation independently of
-- the model response that originally requested it. Task updates use native MCP.
UPDATE ONLY $old.chat SET updated_at = time::now();
UPDATE ONLY $command.operation SET phase = 'dispatching', fence = $command.fence, progress = NONE,
    revision += 1, round += 1, dispatch_until = time::now() + 90s, updated_at = time::now();
RETURN SELECT *, IF run = NONE { NONE } ELSE { { id: run.agent, name: run.agent.display_name } } AS agent FROM ONLY $command.operation;
