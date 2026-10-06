SELECT * FROM map_derivation WHERE tenant = $tenant AND work_context = $context AND kind = $kind AND derivation_key > $after ORDER BY derivation_key ASC LIMIT $limit;
