IF $authority.membership = 'viewer' { THROW 'workspace_forbidden'; };
LET $existing = SELECT * FROM ONLY $command.chat;
IF $existing != NONE {
    IF !fn::workspace_member($authority, $command.chat) { THROW 'workspace_not_found'; };
    IF $existing.created_by != $authority.principal OR $existing.initial_title != $command.title {
        THROW 'workspace_conflict';
    };
} ELSE {
    CREATE ONLY $command.chat SET tenant = $authority.tenant,
        work_context = $authority.work_context, owner = $authority.principal, created_by = $authority.principal,
        title = $command.title, initial_title = $command.title;
    CREATE ONLY $command.member SET chat = $command.chat, principal = $authority.principal;
    CREATE ONLY type::record('workspace_event', [$command.chat, 1]) SET
        chat = $command.chat, sequence = 1, kind = 'created', target = $command.chat;
};
RETURN SELECT * FROM ONLY $command.chat;
