SELECT VALUE candidate FROM (SELECT event_key AS candidate FROM time_temporal_event
WHERE tenant = $tenant AND owner = $owner AND string::lowercase(event_key) CONTAINS $needle GROUP BY candidate
ORDER BY candidate ASC
LIMIT $limit);
