LET $old = SELECT * FROM ONLY $command.operation;
IF $old != NONE {
    IF !fn::workspace_operation_owner($authority, $command.operation) { THROW 'workspace_not_found'; };
    IF $old.chat != $command.chat OR $old.run != $command.run OR $old.profile != $command.profile
        OR $old.app_uri != $command.app_uri
        OR $old.tool != $command.tool OR $old.arguments != $command.arguments { THROW 'workspace_conflict'; };
} ELSE {
    IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; };
    IF $authority.membership = 'viewer' { THROW 'workspace_forbidden'; };
    LET $chat = SELECT * FROM ONLY $command.chat;
    IF $chat.archived { THROW 'workspace_conflict'; };
    IF $command.run != NONE {
        LET $run = SELECT * FROM ONLY $command.run;
        LET $agent = SELECT * FROM ONLY $run.agent;
        IF $run.chat != $command.chat OR $run.initiator != $authority.principal
            OR $run.state != 'running' OR $run.fence != $command.run_fence
            OR $run.lease_until <= time::now() OR $run.deadline <= time::now()
            OR !$agent.active OR $agent.definition_digest != $run.definition_digest { THROW 'workspace_conflict'; };
    };
    IF count(SELECT id FROM workspace_operation WHERE owner = $authority.principal AND chat = $command.chat
        AND tenant = $authority.tenant AND phase = 'dispatching' AND dispatch_until > time::now()) >= 8 {
        THROW 'workspace_conflict';
    };
    -- Serialize admission with membership changes and other chat mutations.
    UPDATE ONLY $command.chat SET updated_at = time::now();
    CREATE ONLY $command.operation SET tenant = $authority.tenant, work_context = $authority.work_context,
        owner = $authority.principal, chat = $command.chat, run = $command.run,
        profile = $command.profile, app_uri = $command.app_uri, tool = $command.tool, arguments = $command.arguments,
        phase = 'dispatching', fence = $command.fence, dispatch_until = time::now() + 90s;
};
RETURN SELECT *, IF run = NONE { NONE } ELSE { { id: run.agent, name: run.agent.display_name } } AS agent FROM ONLY $command.operation;
