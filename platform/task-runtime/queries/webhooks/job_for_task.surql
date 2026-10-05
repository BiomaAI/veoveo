SELECT * FROM provider_job WHERE task = $task AND provider = $provider AND tenant = $tenant
    AND fn::kernel::tasks::lifecycle_v1(task, $server, tenant, $task_types, NONE) != NONE;
