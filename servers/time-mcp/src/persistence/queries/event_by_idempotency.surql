SELECT * FROM time_temporal_event
WHERE tenant = $tenant AND owner = $owner AND idempotency_key = $key
LIMIT 1;
