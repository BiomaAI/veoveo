SELECT VALUE candidate FROM (SELECT calendar_key AS candidate FROM time_calendar_version
WHERE tenant = $tenant AND true AND string::lowercase(calendar_key) CONTAINS $needle GROUP BY candidate
ORDER BY candidate ASC
LIMIT $limit);
