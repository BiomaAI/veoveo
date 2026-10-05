SELECT * FROM time_acquisition
WHERE tenant = $tenant AND owner = $owner AND idempotency_key = $key
LIMIT 1;
