SELECT * FROM map_dataset_release
WHERE tenant = $tenant AND release_key IN $releases
ORDER BY release_key ASC;
