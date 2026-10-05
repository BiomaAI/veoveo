BEGIN TRANSACTION;
LET $_webhook_task = SELECT * FROM ONLY $task;
IF $_webhook_task = NONE
    OR $_webhook_task.server != $server OR $_webhook_task.tenant != $tenant
    OR $_webhook_task.task_type != $task_type OR $_webhook_task.recovery_class != 'webhook_wait'
    OR $_webhook_task.request != $expected_request
    OR $_webhook_task.owner_context != $expected_owner_context
    OR fn::kernel::tasks::lifecycle_v1($task, $server, $tenant, [$task_type], NONE) = NONE {
    THROW 'task_webhook_identity_conflict';
};
LET $_webhook_job = SELECT * FROM ONLY $job;
IF $_webhook_job != NONE AND (
    $_webhook_job.task != $task OR $_webhook_job.tenant != $tenant
    OR $_webhook_job.provider != $provider OR $_webhook_job.external_job_id != $external_job_id
) { THROW 'task_webhook_job_conflict'; };
