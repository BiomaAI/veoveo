SELECT * FROM task_input WHERE task IN (SELECT VALUE id FROM $task WHERE server = $server AND tenant = $tenant AND owner = $owner
    AND profile = $profile AND request.owner.principal_key = $principal_key
    AND request.owner.profile = $profile_key
    AND (request.owner.tenant_key ?? NONE) = $tenant_key
    AND request.owner.data_labels ALLINSIDE $labels) AND response = NONE ORDER BY created_at ASC;