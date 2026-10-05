SELECT VALUE candidate FROM (SELECT type::string(calendar_version) AS candidate FROM time_calendar_version
WHERE tenant = $tenant AND ($parent_key = NONE OR calendar_key = $parent_key) AND string::lowercase(type::string(calendar_version)) CONTAINS $needle GROUP BY candidate
ORDER BY candidate ASC
LIMIT $limit);
