SELECT * FROM provider_event
WHERE provider = $provider AND processed_at = NONE
ORDER BY received_at ASC
LIMIT $limit;
