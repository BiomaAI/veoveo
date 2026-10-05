-- The common transaction wrapper checks current human/context authority.
-- Results and arguments never enter the personal observation projection.
LET $operations = SELECT id,
    IF phase = 'dispatching' AND dispatch_until <= time::now() { 'unconfirmed' } ELSE { phase } AS phase,
    task_id, revision, created_at FROM workspace_operation
    WHERE tenant = $authority.tenant AND work_context = $authority.work_context
    AND owner = $authority.principal AND profile = $command
    ORDER BY created_at DESC LIMIT 100;
LET $invitations = SELECT * FROM workspace_invitation WHERE invitee = $authority.principal
    AND state = 'pending' AND expires_at > time::now()
    AND chat.tenant = $authority.tenant AND chat.work_context = $authority.work_context
    ORDER BY created_at DESC LIMIT 100;
RETURN { operations: $operations, invitations: $invitations };
