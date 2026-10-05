BEGIN TRANSACTION;
LET $current = SELECT * FROM ONLY $task;
IF $current = NONE OR $current.server != $server OR $current.recovery_class != 'webhook_wait'
    OR $current.status NOT IN ['succeeded','failed','cancelled'] {
    THROW 'task_webhook_outcome_unsettled';
};
LET $jobs = SELECT * FROM provider_job WHERE task = $task AND provider = $provider;
IF array::len($jobs) != 1 { THROW 'task_webhook_job_conflict'; };
LET $job = array::first($jobs);
IF type::is_object($job) {
    IF $job.tenant != $current.tenant { THROW 'task_webhook_job_conflict'; };
    IF $job.terminal_event = NONE OR $job.observed_at = NONE {
        THROW 'task_webhook_outcome_unresolved';
    };
    LET $event: record<provider_event> = $job.terminal_event;
    LET $events = SELECT * FROM $event WHERE id = $event AND provider_job = $job.id
        AND tenant = $job.tenant AND provider = $provider
        AND payload = $job.provider_payload AND processed_at != NONE;
    IF array::len($events) != 1 { THROW 'task_webhook_outcome_unprocessed'; };
    UPDATE ONLY $task SET retention_pins -= $provider_retention_pin
        WHERE server = $server AND tenant = $job.tenant
            AND recovery_class = 'webhook_wait' AND status IN ['succeeded','failed','cancelled'];
};
COMMIT TRANSACTION;
