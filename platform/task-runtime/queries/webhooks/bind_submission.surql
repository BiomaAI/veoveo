IF $_webhook_job = NONE { CREATE ONLY $job CONTENT $job_content RETURN NONE; };
IF $_webhook_job = NONE OR $_webhook_job.terminal_event = NONE {
    UPDATE ONLY $task SET status = 'waiting', lease_owner = NONE, lease_expires_at = NONE,
        request.status_message = $waiting_message, updated_at = $now
    WHERE status IN ['queued', 'running', 'waiting'] AND cancel_requested_at = NONE;
};
RETURN SELECT * FROM ONLY $job;
COMMIT TRANSACTION;
