SELECT * FROM task WHERE server = $server AND tenant = $tenant AND owner = $owner
    AND profile = $profile AND request.owner.principal_key = $principal_key
    AND request.owner.profile = $profile_key
    AND (request.owner.tenant_key ?? NONE) = $tenant_key
    AND request.owner.data_labels ALLINSIDE $labels AND task_type IN $task_types AND (created_at > $after_created_at OR
                (created_at = $after_created_at AND id > $after_task)) ORDER BY created_at ASC, id ASC LIMIT $limit;