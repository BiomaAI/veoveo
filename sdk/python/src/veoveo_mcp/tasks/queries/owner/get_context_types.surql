SELECT * FROM $task WHERE server = $server AND tenant = $tenant AND owner = $owner
    AND profile = $profile AND request.owner.principal_key = $principal_key
    AND request.owner.profile = $profile_key
    AND (request.owner.tenant_key ?? NONE) = $tenant_key
    AND request.owner.data_labels ALLINSIDE $labels AND task_type IN $task_types AND work_context = $work_context
                AND authority.context_key = $work_context_key
                AND request.owner.authority.work_context = $work_context_key
                AND request.owner.authority.tenant = $authority_tenant LIMIT 1;