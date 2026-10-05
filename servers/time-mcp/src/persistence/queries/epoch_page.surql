SELECT * FROM time_mission_epoch
WHERE tenant = $tenant AND ($after_key = NONE OR epoch_key > $after_key OR (epoch_key = $after_key AND epoch_version < $after_version))
ORDER BY epoch_key ASC, epoch_version DESC
LIMIT $limit;
