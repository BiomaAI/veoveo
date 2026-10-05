BEGIN TRANSACTION; LET $updated = (UPDATE ONLY $task SET status = $status, request = $request, progress = $progress, result = $result, error = $error, completed_at = $now, lease_owner = NONE, lease_expires_at = NONE, updated_at = $now
WHERE updated_at = $expected_updated AND request = $expected_request AND owner_context = $expected_owner_context AND status IN ['queued', 'running', 'waiting'] AND recovery_class = 'webhook_wait'
RETURN AFTER); IF $updated = NONE { THROW 'media webhook completion conflict'; }; UPDATE ONLY $event SET processed_at = $now, processing_error = NONE
WHERE processed_at = NONE
RETURN NONE; UPDATE ONLY $job SET state = $job_state, completed_at = $now, updated_at = $now
RETURN NONE; COMMIT TRANSACTION;
