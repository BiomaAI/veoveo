SELECT * FROM map_dataset_release WHERE tenant = $tenant AND dataset_key = $dataset ORDER BY release_key ASC LIMIT $limit;
