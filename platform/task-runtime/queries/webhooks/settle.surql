LET $_settlement_event = SELECT * FROM ONLY $event;
IF $_settlement_event = NONE OR $_settlement_event.provider_job != $job
    OR $_settlement_event.tenant != $tenant OR $_settlement_event.provider != $provider
    OR $_webhook_job.terminal_event != $event OR $_webhook_job.observed_at = NONE
    OR $_settlement_event.payload != $_webhook_job.provider_payload
    OR $_webhook_job.provider_payload != $payload {
    THROW 'task_webhook_terminal_receipt_conflict';
};
LET $_already_terminal = $_webhook_task.status IN ['succeeded', 'failed', 'cancelled'];
LET $_webhook_updated = IF $_already_terminal THEN $_webhook_task ELSE (
    UPDATE ONLY $task SET status = $next, request = $next_request, progress = $progress,
        result = $result, result_uri = $result_uri, error = $error, completed_at = $now,
        lease_owner = NONE, lease_expires_at = NONE, updated_at = $now
    WHERE updated_at = $expected_updated_at AND status = $expected_status
        AND status IN ['queued', 'running', 'waiting', 'cancel_requested']
        AND (status != 'cancel_requested' OR $next = 'cancelled')
    RETURN AFTER
) END;
IF $_webhook_updated = NONE { THROW 'task_webhook_settlement_conflict'; };
