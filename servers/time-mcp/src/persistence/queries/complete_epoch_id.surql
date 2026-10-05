SELECT VALUE candidate FROM (SELECT epoch_key AS candidate FROM time_mission_epoch
WHERE tenant = $tenant AND true AND string::lowercase(epoch_key) CONTAINS $needle GROUP BY candidate
ORDER BY candidate ASC
LIMIT $limit);
