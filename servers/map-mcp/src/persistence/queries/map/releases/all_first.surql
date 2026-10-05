SELECT * FROM map_dataset_release WHERE tenant = $tenant ORDER BY release_key ASC LIMIT $limit;
