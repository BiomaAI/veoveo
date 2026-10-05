SELECT * FROM provider_event
WHERE tenant = $tenant AND provider = $provider AND event_id = $event_id
LIMIT 1;
