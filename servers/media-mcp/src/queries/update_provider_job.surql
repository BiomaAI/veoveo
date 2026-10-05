BEGIN TRANSACTION; UPDATE ONLY $job SET state = $state, provider_payload = $payload, completed_at = IF $terminal { $now } ELSE { completed_at }, updated_at = $now
WHERE tenant = $tenant AND task = $task AND provider = $provider AND state IN ['submitted', 'waiting', 'cancel_requested']
RETURN NONE; COMMIT TRANSACTION;
