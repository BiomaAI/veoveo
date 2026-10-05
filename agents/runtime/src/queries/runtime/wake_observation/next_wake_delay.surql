RETURN { now: time::now(),
                pending: array::first(SELECT VALUE available_at FROM wake WHERE agent = $agent AND state = 'pending' ORDER BY available_at ASC LIMIT 1),
                claimed: array::first(SELECT VALUE claim_expires_at FROM wake WHERE agent = $agent AND state = 'claimed' AND claim_expires_at != NONE ORDER BY claim_expires_at ASC LIMIT 1) };
