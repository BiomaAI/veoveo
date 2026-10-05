SELECT * FROM wake WHERE agent = $agent AND state = 'claimed' AND (claim_expires_at = NONE OR claim_expires_at <= $now);
