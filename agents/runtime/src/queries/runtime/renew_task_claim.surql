UPDATE ONLY $task SET lease_expires_at = $expiry, updated_at = $now, revision += 1 WHERE agent = $agent AND state = 'watching' AND lease_owner = $owner AND lease_expires_at > $now RETURN AFTER;
