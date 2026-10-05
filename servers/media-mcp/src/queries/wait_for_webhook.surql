BEGIN TRANSACTION; UPDATE ONLY $task SET status = 'waiting', request = $request, progress = $progress, lease_owner = NONE, lease_expires_at = NONE, updated_at = $now
WHERE updated_at = $expected_updated AND request = $expected_request AND owner_context = $expected_owner_context AND status IN ['queued', 'running'] AND recovery_class = 'webhook_wait'
RETURN NONE; COMMIT TRANSACTION;
