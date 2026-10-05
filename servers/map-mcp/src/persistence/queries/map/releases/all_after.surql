SELECT * FROM map_dataset_release WHERE tenant = $tenant AND release_key > $after ORDER BY release_key ASC LIMIT $limit;
