SELECT VALUE candidate FROM (SELECT type::string(epoch_version) AS candidate FROM time_mission_epoch
WHERE tenant = $tenant AND ($parent_key = NONE OR epoch_key = $parent_key) AND string::lowercase(type::string(epoch_version)) CONTAINS $needle GROUP BY candidate
ORDER BY candidate ASC
LIMIT $limit);
