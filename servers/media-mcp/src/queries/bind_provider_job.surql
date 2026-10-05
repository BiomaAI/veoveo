BEGIN TRANSACTION; CREATE ONLY $job CONTENT $job_content
RETURN NONE; LET $updated = (UPDATE ONLY $task SET status = 'waiting', request = $request, progress = $progress, lease_owner = NONE, lease_expires_at = NONE, updated_at = $now
WHERE status = $expected_status AND updated_at = $expected_updated AND request = $expected_request AND owner_context = $expected_owner_context AND recovery_class = 'webhook_wait' AND lease_owner = $worker
RETURN AFTER); IF $updated = NONE { THROW 'media task changed before provider binding'; }; COMMIT TRANSACTION;
