SELECT * FROM wake WHERE agent = $agent AND state = 'pending' AND available_at <= $now ORDER BY available_at ASC, created_at ASC, id ASC LIMIT $limit;
