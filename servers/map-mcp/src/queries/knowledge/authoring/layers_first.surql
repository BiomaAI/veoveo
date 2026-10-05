SELECT * FROM map_feature_layer WHERE tenant = $tenant AND work_context = $context AND $labels CONTAINSALL data_labels  ORDER BY layer_key LIMIT $limit;
