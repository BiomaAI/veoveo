UPDATE map_route SET base_release_ids=[] WHERE tenant=$tenant AND route_key=$route_key RETURN NONE;
