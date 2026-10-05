SELECT dispatch FROM media_task WHERE task = $task
    AND identity.tenant = $tenant AND identity.owner_context = $owner_context
    AND identity.request = $request
    AND fn::kernel::tasks::lifecycle_v1($task, $server, $tenant, $task_types, NONE) != NONE
    AND fn::kernel::tasks::input_matches_v1($task, $request);
