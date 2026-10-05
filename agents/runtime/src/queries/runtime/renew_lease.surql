UPDATE ONLY $agent SET lease_expires_at = $expires, heartbeat_at = $now, revision += 1, updated_at = $now WHERE lease_owner = $owner AND fence = $fence AND lease_expires_at > $now RETURN AFTER;
