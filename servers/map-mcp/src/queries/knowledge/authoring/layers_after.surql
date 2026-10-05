SELECT * FROM map_feature_layer WHERE tenant = $tenant AND work_context = $context AND $labels CONTAINSALL data_labels AND layer_key > $key ORDER BY layer_key LIMIT $limit;
