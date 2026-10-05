SELECT * FROM task_input WHERE task IN (SELECT VALUE id FROM $task WHERE server = $server AND tenant = $tenant AND owner = $owner
    AND profile = $profile AND owner_context.principal_key = $principal_key
    AND owner_context.profile = $profile_key
    AND (owner_context.tenant_key ?? NONE) = $tenant_key
    AND owner_context.data_labels ALLINSIDE $labels) AND response = NONE ORDER BY created_at ASC;