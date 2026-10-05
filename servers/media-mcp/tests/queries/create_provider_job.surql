CREATE ONLY $job CONTENT $record RETURN NONE;
CREATE ONLY $event_id CONTENT $event RETURN NONE;
CREATE media_task SET task = $task, task_type = 'run', created_at = $created_at,
    identity = {tenant: $tenant, owner_context: $owner_context, request: $request},
    association = {job: $job, provider: 'media', prediction: $prediction} RETURN NONE;
