BEGIN TRANSACTION; CREATE ONLY $event CONTENT $event_content
RETURN NONE; UPSERT ONLY $job CONTENT $job_content
RETURN NONE; IF $update_task { LET $updated = (UPDATE ONLY $task SET status = 'waiting', request = $request, progress = $progress, lease_owner = NONE, lease_expires_at = NONE, updated_at = $now
WHERE updated_at = $expected_updated AND request = $expected_request AND owner_context = $expected_owner_context AND status IN ['queued', 'running', 'waiting'] AND recovery_class = 'webhook_wait'
RETURN AFTER); IF $updated = NONE { THROW 'media webhook task changed'; }; }; COMMIT TRANSACTION;
