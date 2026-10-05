SELECT * FROM map_feature_layer WHERE tenant = $tenant AND work_context = $context AND $labels CONTAINSALL data_labels AND ($after = NONE OR layer_key > $after) ORDER BY layer_key ASC LIMIT $limit;
