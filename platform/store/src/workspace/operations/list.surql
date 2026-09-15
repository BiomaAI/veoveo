IF $command.before != NONE {
    IF !fn::workspace_operation_owner($authority, $command.before) { THROW 'workspace_not_found'; };
};
LET $cursor = IF $command.before != NONE { SELECT * FROM ONLY $command.before } ELSE { NONE };
RETURN SELECT * FROM workspace_operation WHERE tenant = $authority.tenant
    AND work_context = $authority.work_context AND owner = $authority.principal
    AND ($command.chat = NONE OR chat = $command.chat)
    AND ($cursor = NONE OR created_at < $cursor.created_at OR (created_at = $cursor.created_at AND id < $cursor.id))
    ORDER BY created_at DESC, id DESC LIMIT 100;
