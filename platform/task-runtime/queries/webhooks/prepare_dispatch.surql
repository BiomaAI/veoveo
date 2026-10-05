BEGIN TRANSACTION;
LET $_dispatch_task = SELECT * FROM ONLY $task;
IF $_dispatch_task = NONE
    OR $_dispatch_task.server != $server OR $_dispatch_task.tenant != $tenant
    OR $_dispatch_task.task_type != $task_type OR $_dispatch_task.recovery_class != 'webhook_wait'
    OR $_dispatch_task.request != $expected_request
    OR $_dispatch_task.owner_context != $expected_owner_context
    OR $_dispatch_task.updated_at != $expected_updated_at
    OR fn::kernel::tasks::lifecycle_v1($task, $server, $tenant, [$task_type], NONE) = NONE {
    THROW 'task_dispatch_fence_conflict';
};
