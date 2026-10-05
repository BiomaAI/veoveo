SELECT VALUE count() FROM map_feature_head WHERE tenant = $tenant AND work_context = $context AND layer_key = $layer GROUP ALL;
