RETURN $keys.map(|$key| { (SELECT * FROM time_mission_epoch
WHERE tenant = $tenant AND epoch_key = $key
ORDER BY epoch_version DESC
LIMIT 1)[0] }).filter(|$row| $row != NONE);
