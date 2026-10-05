IF $_webhook_job = NONE { CREATE ONLY $job CONTENT $job_content RETURN NONE; };
LET $_webhook_event = SELECT * FROM ONLY $event;
IF $_webhook_event != NONE AND (
    $_webhook_event.tenant != $tenant OR $_webhook_event.provider_job != $job
    OR $_webhook_event.provider != $provider OR $_webhook_event.event_id != $event_id
    OR $_webhook_event.payload != $payload
) { THROW 'task_webhook_event_conflict'; };
IF $_webhook_event = NONE { CREATE ONLY $event CONTENT $event_content RETURN NONE; };
IF $is_terminal AND ($_webhook_job = NONE OR $_webhook_job.terminal_event = NONE) {
    UPDATE ONLY $job SET state = $terminal_state, provider_payload = $payload,
        terminal_event = $event, observed_at = $now, completed_at = $now, updated_at = $now;
};
LET $_received_job = SELECT * FROM ONLY $job;
LET $_received_event = SELECT * FROM ONLY $event;
RETURN {
    job: $_received_job,
    event: $_received_event,
    inserted: $_webhook_event = NONE,
    authoritative: $_received_job.terminal_event = $event
};
COMMIT TRANSACTION;
