SELECT derivation_key, created_by, created_at FROM map_derivation WHERE tenant = $tenant AND work_context = $context AND kind = $kind ORDER BY derivation_key ASC LIMIT $limit;
