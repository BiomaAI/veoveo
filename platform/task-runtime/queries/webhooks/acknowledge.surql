LET $_ack_event = SELECT * FROM ONLY $event;
IF $_ack_event = NONE OR $_ack_event.provider_job != $job OR $_ack_event.tenant != $tenant
    OR $_ack_event.provider != $provider { THROW 'task_webhook_event_conflict'; };
IF $_webhook_job.terminal_event = $event AND (
    $_webhook_job.observed_at = NONE OR $_ack_event.payload != $_webhook_job.provider_payload
) { THROW 'task_webhook_terminal_receipt_conflict'; };
IF $_webhook_job.terminal_event = $event AND $_webhook_task.status NOT IN ['succeeded', 'failed', 'cancelled'] {
    THROW 'task_webhook_outcome_unsettled';
};
UPDATE ONLY $event SET processed_at = $now, processing_error = NONE WHERE processed_at = NONE RETURN NONE;
COMMIT TRANSACTION;
