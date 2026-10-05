SELECT * FROM time_acquisition
WHERE tenant = $tenant AND staged_release_key = $release_key
LIMIT 1;
