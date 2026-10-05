SELECT * FROM map_route WHERE tenant = $tenant AND status != 'invalidated' AND facility_ids CONTAINS $dependency AND ($after = NONE OR route_key > $after) ORDER BY route_key ASC LIMIT $limit;
