SELECT VALUE candidate FROM (SELECT release_key AS candidate FROM time_authority_release
WHERE tenant = $tenant AND true AND string::lowercase(release_key) CONTAINS $needle GROUP BY candidate
ORDER BY candidate ASC
LIMIT $limit);
