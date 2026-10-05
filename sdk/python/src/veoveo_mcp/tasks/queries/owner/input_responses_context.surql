BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $input SET response = $response, responded_at = $now WHERE task = $task AND response = NONE RETURN AFTER); IF $updated != NONE { LET $task_updated = (UPDATE ONLY $task SET updated_at = $now WHERE server = $server AND status IN ['queued', 'running', 'waiting'] AND server = $server AND tenant = $tenant AND owner = $owner
    AND profile = $profile AND request.owner.principal_key = $principal_key
    AND request.owner.profile = $profile_key
    AND (request.owner.tenant_key ?? NONE) = $tenant_key
    AND request.owner.data_labels ALLINSIDE $labels AND work_context = $work_context
                AND authority.context_key = $work_context_key
                AND request.owner.authority.work_context = $work_context_key
                AND request.owner.authority.tenant = $authority_tenant RETURN AFTER); IF $task_updated = NONE { THROW 'task cannot accept input'; }; }; RETURN $updated; COMMIT TRANSACTION;